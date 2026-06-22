use crate::engine::config::Config;
use crate::engine::config_write::write_config_in_place;
use crate::engine::skills::{embedded_skill_set, ROUTER_KEY};
use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use std::fs;
use std::path::Path;

#[derive(Subcommand)]
pub enum SkillsCommand {
    /// Install the generic verb skill set into the project
    Install {
        /// Target runtime: omit for both Claude and AGENTS.md
        #[arg(long, value_enum)]
        runtime: Option<Runtime>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Runtime {
    /// Place skills under .claude/skills/
    Claude,
    /// Concatenate prose into ./AGENTS.md
    AgentsMd,
}

/// Resolve the router entry name from `[skills] entry` when a config exists,
/// else the default `lazy`. Install never creates a config, so a missing one
/// just yields the default.
fn resolve_entry(root: &Path) -> Result<String> {
    let path = root.join(".lazyspec.toml");
    if !path.exists() {
        return Ok("lazy".to_string());
    }
    let src = fs::read_to_string(&path)?;
    let config = Config::parse(&src)?;
    Ok(config.skills.entry)
}

/// Rewrite the embedded router's `name:` frontmatter line to the resolved entry
/// so invoking the custom name dispatches the router (AC5).
fn rename_router(contents: &str, entry: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    let mut in_frontmatter = false;
    let mut rewrote = false;
    for line in contents.lines() {
        if line == "---" {
            in_frontmatter = !in_frontmatter;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_frontmatter && !rewrote && line.starts_with("name:") {
            out.push_str(&format!("name: {entry}\n"));
            rewrote = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn install_claude(root: &Path, entry: &str) -> Result<()> {
    let skills_root = root.join(".claude").join("skills");
    for (rel_path, contents) in embedded_skill_set() {
        let (dest_rel, body) = if rel_path.to_string_lossy() == ROUTER_KEY {
            (
                Path::new(entry).join("SKILL.md"),
                rename_router(contents, entry),
            )
        } else {
            (rel_path, contents.to_string())
        };
        let dest = skills_root.join(&dest_rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, body)?;
    }
    Ok(())
}

fn install_agents_md(root: &Path, entry: &str) -> Result<()> {
    let mut sections: Vec<String> = Vec::new();
    for (rel_path, contents) in embedded_skill_set() {
        let body = if rel_path.to_string_lossy() == ROUTER_KEY {
            rename_router(contents, entry)
        } else {
            contents.to_string()
        };
        sections.push(body.trim_end().to_string());
    }
    let joined = sections.join("\n\n---\n\n");
    fs::write(root.join("AGENTS.md"), format!("{joined}\n"))?;
    Ok(())
}

/// Record the resolved entry into an existing config. A no-op when no config is
/// present -- install never creates `.lazyspec.toml`.
fn record_entry(root: &Path, entry: &str) -> Result<()> {
    let path = root.join(".lazyspec.toml");
    if !path.exists() {
        return Ok(());
    }
    let src = fs::read_to_string(&path)?;
    let mut config = Config::parse(&src)?;
    config.skills.entry = entry.to_string();
    let out = write_config_in_place(&src, &config)?;
    fs::write(&path, out)?;
    Ok(())
}

pub fn run_install(root: &Path, runtime: Option<Runtime>) -> Result<()> {
    let entry = resolve_entry(root)?;

    let do_claude = !matches!(runtime, Some(Runtime::AgentsMd));
    let do_agents = !matches!(runtime, Some(Runtime::Claude));

    if do_claude {
        install_claude(root, &entry)?;
    }
    if do_agents {
        install_agents_md(root, &entry)?;
    }

    record_entry(root, &entry)?;

    println!("Installed skill set (entry: {entry})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn install_without_config_places_files_and_creates_no_config() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, None).unwrap();

        assert!(root.join(".claude/skills/lazy/SKILL.md").exists());
        assert!(root.join(".claude/skills/scaffold/SKILL.md").exists());
        let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(!agents.is_empty());
        assert!(agents.contains("name: scaffold"));
        assert!(!root.join(".lazyspec.toml").exists());
    }

    #[test]
    fn install_with_config_records_default_entry() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let cfg = Config::default();
        fs::write(root.join(".lazyspec.toml"), cfg.to_toml().unwrap()).unwrap();

        run_install(root, None).unwrap();

        let src = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        let parsed = Config::parse(&src).unwrap();
        assert_eq!(parsed.skills.entry, "lazy");
        assert!(root.join(".claude/skills/lazy/SKILL.md").exists());
    }

    #[test]
    fn install_is_idempotent() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, None).unwrap();
        run_install(root, None).unwrap();

        assert!(root.join(".claude/skills/lazy/SKILL.md").exists());
        assert!(root.join("AGENTS.md").exists());
    }

    #[test]
    fn custom_entry_renames_router_and_is_preserved() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let mut cfg = Config::default();
        cfg.skills.entry = "go".to_string();
        fs::write(root.join(".lazyspec.toml"), cfg.to_toml().unwrap()).unwrap();

        run_install(root, None).unwrap();

        let router = fs::read_to_string(root.join(".claude/skills/go/SKILL.md")).unwrap();
        assert!(router.contains("name: go"));
        assert!(!root.join(".claude/skills/lazy/SKILL.md").exists());
        assert!(root.join(".claude/skills/scaffold/SKILL.md").exists());

        let src = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert_eq!(Config::parse(&src).unwrap().skills.entry, "go");
    }

    #[test]
    fn runtime_claude_skips_agents_md() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, Some(Runtime::Claude)).unwrap();

        assert!(root.join(".claude/skills/lazy/SKILL.md").exists());
        assert!(!root.join("AGENTS.md").exists());
    }

    #[test]
    fn runtime_agents_md_skips_claude() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, Some(Runtime::AgentsMd)).unwrap();

        assert!(root.join("AGENTS.md").exists());
        assert!(!root.join(".claude/skills").exists());
    }
}
