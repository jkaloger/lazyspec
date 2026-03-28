use crate::engine::config::Config;
use crate::engine::gh::{deterministic_color, type_label, GhCli, GhIssueWriter, GhError};
use crate::engine::github::resolve_repo;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

pub fn run(root: &Path) -> Result<()> {
    let config_path = root.join(".lazyspec.toml");
    if config_path.exists() {
        bail!(".lazyspec.toml already exists");
    }

    let config = Config::default();

    for type_def in &config.documents.types {
        fs::create_dir_all(root.join(&type_def.dir))?;
    }
    fs::create_dir_all(root.join(&config.filesystem.templates.dir))?;

    fs::write(&config_path, config.to_toml()?)?;

    ensure_github_labels(&config, root);
    ensure_gitignore(&config, root)?;

    println!("Initialized lazyspec in {}", root.display());
    Ok(())
}

fn ensure_github_labels(config: &Config, root: &Path) {
    let gh_types = config.documents.github_issues_types();

    if gh_types.is_empty() {
        return;
    }

    let repo = match resolve_repo(config, root).ok() {
        Some(r) => r,
        None => {
            eprintln!("warning: could not resolve GitHub repo; skipping label creation");
            return;
        }
    };

    let client = GhCli::new();
    for type_name in &gh_types {
        let label = type_label(type_name);
        let color = deterministic_color(type_name);
        let description = format!("lazyspec document type: {}", type_name);
        match client.label_ensure(&repo, &label, &description, &color) {
            Ok(()) => println!("  created label: {}", label),
            Err(e) => {
                if let Some(gh_err) = e.downcast_ref::<GhError>() {
                    if matches!(gh_err, GhError::NotInstalled) {
                        eprintln!(
                            "warning: gh CLI not found; skipping label creation for github-issues types"
                        );
                        return;
                    }
                }
                eprintln!("warning: failed to create label {}: {}", label, e);
            }
        }
    }
}

const GITIGNORE_ENTRIES: &[&str] = &[".lazyspec/cache/", ".lazyspec/issue-map.json"];

fn ensure_gitignore(config: &Config, root: &Path) -> Result<()> {
    if !config.documents.has_github_issues_types() {
        return Ok(());
    }

    let gitignore_path = root.join(".gitignore");
    let existing = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    let existing_lines: Vec<&str> = existing.lines().collect();
    let mut to_append: Vec<&str> = GITIGNORE_ENTRIES
        .iter()
        .filter(|entry| !existing_lines.contains(entry))
        .copied()
        .collect();

    if to_append.is_empty() {
        return Ok(());
    }

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    to_append.push(""); // trailing newline
    content.push_str(&to_append.join("\n"));

    fs::write(&gitignore_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{StoreBackend, TypeDef};

    fn gh_issues_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![
            TypeDef::test_fixture("rfc", StoreBackend::Filesystem),
            TypeDef::test_fixture("story", StoreBackend::GithubIssues),
        ];
        config
    }

    #[test]
    fn gitignore_created_when_github_issues_type_exists() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();

        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(contents.contains(".lazyspec/cache/"));
        assert!(contents.contains(".lazyspec/issue-map.json"));
    }

    #[test]
    fn gitignore_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();

        ensure_gitignore(&config, dir.path()).unwrap();
        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            contents.matches(".lazyspec/cache/").count(),
            1,
            "cache entry duplicated"
        );
        assert_eq!(
            contents.matches(".lazyspec/issue-map.json").count(),
            1,
            "issue-map entry duplicated"
        );
    }

    #[test]
    fn gitignore_appends_to_existing() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        fs::write(&gitignore, "node_modules/\n").unwrap();

        let config = gh_issues_config();
        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(&gitignore).unwrap();
        assert!(contents.starts_with("node_modules/\n"));
        assert!(contents.contains(".lazyspec/cache/"));
        assert!(contents.contains(".lazyspec/issue-map.json"));
    }

    #[test]
    fn gitignore_skips_already_present_entries() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        fs::write(&gitignore, ".lazyspec/cache/\n").unwrap();

        let config = gh_issues_config();
        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(&gitignore).unwrap();
        assert_eq!(contents.matches(".lazyspec/cache/").count(), 1);
        assert!(contents.contains(".lazyspec/issue-map.json"));
    }

    #[test]
    fn gitignore_not_created_for_filesystem_only() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();

        ensure_gitignore(&config, dir.path()).unwrap();

        assert!(!dir.path().join(".gitignore").exists());
    }
}
