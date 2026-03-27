use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::{deterministic_color, type_label, GhCli, GhIssueWriter, GhError};
use crate::engine::github::infer_github_repo;
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
    let gh_types = github_issues_types(config);

    if gh_types.is_empty() {
        return;
    }

    let repo = resolve_repo(config, root);
    let repo = match repo {
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

fn resolve_repo(config: &Config, root: &Path) -> Option<String> {
    if let Some(ref gh) = config.documents.github {
        if let Some(ref repo) = gh.repo {
            return Some(repo.clone());
        }
    }
    infer_github_repo(root).ok()
}

const GITIGNORE_ENTRIES: &[&str] = &[".lazyspec/cache/", ".lazyspec/issue-map.json"];

fn ensure_gitignore(config: &Config, root: &Path) -> Result<()> {
    if github_issues_types(config).is_empty() {
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

fn github_issues_types(config: &Config) -> Vec<&str> {
    config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GithubIssues)
        .map(|t| t.name.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, TypeDef, NumberingStrategy};

    fn make_type(name: &str, store: StoreBackend) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}", name),
            prefix: name.to_uppercase(),
            icon: None,
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store,
        }
    }

    #[test]
    fn github_issues_types_filters_correctly() {
        let mut config = Config::default();
        config.documents.types = vec![
            make_type("rfc", StoreBackend::Filesystem),
            make_type("story", StoreBackend::GithubIssues),
            make_type("adr", StoreBackend::GithubIssues),
        ];
        let types = github_issues_types(&config);
        assert_eq!(types, vec!["story", "adr"]);
    }

    #[test]
    fn github_issues_types_empty_when_all_filesystem() {
        let config = Config::default();
        let types = github_issues_types(&config);
        assert!(types.is_empty());
    }

    #[test]
    fn resolve_repo_prefers_config() {
        let mut config = Config::default();
        config.documents.github = Some(GithubConfig {
            repo: Some("configured/repo".to_string()),
            cache_ttl: 60,
        });
        let repo = resolve_repo(&config, Path::new("/nonexistent"));
        assert_eq!(repo, Some("configured/repo".to_string()));
    }

    #[test]
    fn resolve_repo_none_without_config_or_git() {
        let config = Config::default();
        let repo = resolve_repo(&config, Path::new("/nonexistent"));
        assert!(repo.is_none());
    }

    fn gh_issues_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![
            make_type("rfc", StoreBackend::Filesystem),
            make_type("story", StoreBackend::GithubIssues),
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
