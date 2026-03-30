use crate::engine::config::Config;
use crate::engine::gh::{deterministic_color, type_label, GhCli, GhError, GhIssueWriter};
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

    scaffold_skeleton_files(root, &config)?;

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

fn scaffold_skeleton_files(root: &Path, config: &Config) -> Result<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    for type_def in &config.documents.types {
        if type_def.singleton && type_def.name == "convention" {
            let conv_dir = root.join(&type_def.dir).join("convention");
            fs::create_dir_all(&conv_dir)?;
            write_if_absent(&conv_dir.join("index.md"), &convention_skeleton(&today))?;
        }

        if type_def.parent_type.as_deref() == Some("convention") && type_def.name == "dictum" {
            let parent = config
                .documents
                .types
                .iter()
                .find(|t| t.name == "convention");
            if let Some(parent) = parent {
                let conv_dir = root.join(&parent.dir).join("convention");
                write_if_absent(&conv_dir.join("example.md"), &dictum_skeleton(&today))?;
            }
        }
    }

    Ok(())
}

fn write_if_absent(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, content)?;
    Ok(())
}

fn convention_skeleton(date: &str) -> String {
    format!(
        r#"---
title: "Convention"
type: convention
status: draft
author: "unknown"
date: {date}
tags: []
---

This is your project's convention. It captures the values, constraints, and
principles that should inform all work in this repository.

Edit this document to describe your project's constitution. Keep it short.
Dictum (child documents in this folder) capture specific principles.
"#
    )
}

fn dictum_skeleton(date: &str) -> String {
    format!(
        r#"---
title: "Example Dictum"
type: dictum
status: draft
author: "unknown"
date: {date}
tags: [example]
---

This is an example dictum. Replace it with a principle that matters to your project.

Each dictum should cover a single topic and be tagged for selective retrieval
by agent skills. For example, a dictum about testing philosophy would have
`tags: [testing]`.
"#
    )
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
