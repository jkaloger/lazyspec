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
    use crate::engine::config::TypeSelector;
    use crate::engine::validation::to_phrase;
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
    fn shipped_router_derives_type_boundaries_from_the_edge_table() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, None).unwrap();

        let router = fs::read_to_string(root.join(".claude/skills/lazy/SKILL.md")).unwrap();
        let mut surfaces: Vec<(String, String)> = embedded_skill_set()
            .map(|(path, contents)| (path.to_string_lossy().into_owned(), contents.to_string()))
            .collect();
        surfaces.push((
            "AGENTS.md".to_string(),
            fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        ));
        for (surface, prose) in &surfaces {
            for retired in ["UNION", "parent-child rule", "`rules`"] {
                assert!(
                    !prose.contains(retired),
                    "{surface} still derives boundaries from `{retired}`"
                );
            }
        }
        assert!(
            router.contains("`edges`"),
            "router must name the config key it reads boundaries from"
        );
    }

    /// `configure-type` and `create-audit` ship only from `skills/` on disk --
    /// they are absent from `EMBEDDED_SKILLS`, so nothing here reads them.
    #[test]
    fn embedded_skills_state_the_pipeline_rule_identically_and_never_reach_configure_type_or_create_audit(
    ) {
        const STANDING_RULE: &str = "- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.";
        const CARRIERS: [&str; 7] = [
            "advance/SKILL.md",
            "co-write/SKILL.md",
            "execute/SKILL.md",
            "generate/SKILL.md",
            "lazy/SKILL.md",
            "review/SKILL.md",
            "scaffold/SKILL.md",
        ];

        let mut carried = Vec::new();
        let mut embedded = Vec::new();
        for (path, contents) in embedded_skill_set() {
            let key = path.to_string_lossy().into_owned();
            embedded.push(key.clone());
            if !contents.contains("Do NOT skip the workflow pipeline") {
                continue;
            }
            assert!(
                contents.contains(STANDING_RULE),
                "{key} states the pipeline rule in its own words; every copy is the one wording naming `edges`"
            );
            carried.push(key);
        }
        carried.sort();
        assert_eq!(carried, CARRIERS, "the set of skills carrying the standing rule changed; the rule must be present in each, not merely consistent");

        for out_of_reach in ["configure-type/SKILL.md", "create-audit/SKILL.md"] {
            assert!(
                !embedded.iter().any(|key| key == out_of_reach),
                "{out_of_reach} is embedded now, so this test's name no longer tells the truth"
            );
        }
    }

    /// ADR-033 withdrew status-conditioned create gating, so `require_parent_status`
    /// and `config add-gate` name a field and a subcommand the binary does not have. A
    /// skill mentioning either promises a refusal that will never come; what the agent
    /// can report is the `UnsatisfiedEdge` finding. `configure-type` is outside
    /// `EMBEDDED_SKILLS` (`src/engine/skills.rs`), so this assertion does not reach its
    /// copy of the same prose.
    #[test]
    fn embedded_skills_promise_no_status_conditioned_create_gate() {
        for (path, contents) in embedded_skill_set() {
            for withdrawn in ["require_parent_status", "add-gate", "gate facts"] {
                assert!(
                    !contents.contains(withdrawn),
                    "{} names `{withdrawn}`; an unsatisfied edge is a validation finding, not a gate on `create`",
                    path.display()
                );
            }
        }
    }

    /// `configure-type` ships from `skills/` on disk only, so `embedded_skill_set()`
    /// never reaches it -- yet it is where the user first meets `--parent-type`.
    const CONFIGURE_TYPE: &str = include_str!("../../skills/configure-type/SKILL.md");

    /// `parent_type` is containment. The authoring skills read it in preflight,
    /// and the one sentence they read it with is the only thing any embedded
    /// skill says about it -- nothing turns it into a link, a create or a
    /// constraint. `configure-type` interviews for the same field, so it is held
    /// to the same account even though it is not embedded.
    #[test]
    fn embedded_skills_describe_parent_type_only_as_containment() {
        const CONTAINMENT: &str = "`parent_type` decides containment only -- the directory this type's documents live under and the store backend they share -- and declares no link.";
        const BOUNDARY_DENIAL: &str = "a type's `parent_type` declares none";
        const READERS: [&str; 3] = [
            "co-write/SKILL.md",
            "generate/SKILL.md",
            "scaffold/SKILL.md",
        ];

        let mut readers = Vec::new();
        for (path, contents) in embedded_skill_set() {
            let key = path.to_string_lossy().into_owned();
            let mentions = contents.matches("`parent_type`").count();
            if mentions == 0 {
                continue;
            }
            let described =
                contents.matches(CONTAINMENT).count() + contents.matches(BOUNDARY_DENIAL).count();
            assert_eq!(
                described, mentions,
                "{key} says something about `parent_type` besides containment and the boundary denial"
            );
            if contents.contains(CONTAINMENT) {
                readers.push(key);
            }
        }
        readers.sort();
        assert_eq!(
            readers, READERS,
            "the set of skills describing `parent_type` changed; each must carry the containment sentence"
        );

        for containment in [
            "Containment, not linkage",
            "It creates no edge and constrains no link",
        ] {
            assert!(
                CONFIGURE_TYPE.contains(containment),
                "configure-type stopped saying `--parent-type` is {containment}"
            );
        }
        assert!(
            !CONFIGURE_TYPE.contains("parent relation"),
            "configure-type calls `--parent-type` a relation; a relation comes from an `[[edges]]` row's `via`"
        );
        assert!(
            CONFIGURE_TYPE.contains("from the `via` of an `[[edges]]` row"),
            "configure-type must say where a relation does come from"
        );
    }

    #[test]
    fn shipped_scaffold_takes_its_link_relation_from_an_edge_row() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, None).unwrap();

        let scaffold = fs::read_to_string(root.join(".claude/skills/scaffold/SKILL.md")).unwrap();
        assert!(
            !scaffold.contains("link <new-id> implements"),
            "scaffold bakes `implements` into its link call"
        );
        assert!(
            scaffold.contains("`via`"),
            "scaffold must name the edge-row key its relation comes from"
        );
    }

    /// A crossing whose far side admits several types offers a choice, and the
    /// report has to carry the whole set with a verb against each member.
    #[test]
    fn shipped_router_reports_every_type_a_crossing_admits() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, None).unwrap();

        let router = fs::read_to_string(root.join(".claude/skills/lazy/SKILL.md")).unwrap();
        assert!(
            !router.contains("its child type `<child-type>` is now eligible to create"),
            "router reports one arbitrary member where the crossing admits a set"
        );
        assert!(
            !router.contains("eligible to cross"),
            "router calls a crossing eligible; nothing gates it, so eligibility is not the fact to report"
        );
        assert!(
            router.contains("one line per type"),
            "router must say the report is a list, since the ceiling verb is per type"
        );
        assert!(
            router.contains("the severity the row's `required` value gives it"),
            "router must report an unsatisfied edge at the severity `required` gave it"
        );
        assert!(
            router.contains("it is not a crossing and gets no report"),
            "router must say a row whose `from` is a wildcard is not a crossing to report"
        );
        for source in ["`validate --json`", "`edges[].to`"] {
            assert!(
                router.contains(source),
                "router must name {source} among the commands the report is assembled from"
            );
        }
    }

    /// The boundary report and the `UnsatisfiedEdge` finding name the same target
    /// set, so they must name it the same way. A comment in `to_phrase` asking for
    /// that would not fail when the prose drifts, and this paragraph is rewritten
    /// once per slice.
    #[test]
    fn shipped_router_phrases_a_target_set_as_the_finding_does() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        run_install(root, None).unwrap();

        let named = to_phrase(&TypeSelector::Types(vec!["spike".into(), "story".into()]));
        let frame = named.split_once("spike").unwrap().0.trim_end();
        let wildcard = to_phrase(&TypeSelector::Any);

        let router = fs::read_to_string(root.join(".claude/skills/lazy/SKILL.md")).unwrap();
        let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        for (surface, prose) in [("router skill", &router), ("AGENTS.md", &agents)] {
            for phrase in [frame, wildcard.as_str()] {
                assert!(
                    prose.contains(phrase),
                    "{surface} phrases a target set its own way; the finding says `{phrase}`"
                );
            }
        }
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
