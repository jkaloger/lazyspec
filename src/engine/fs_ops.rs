use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use chrono::Local;

use crate::engine::config::{Config, NumberingStrategy, ReservedFormat};
use crate::engine::document::{compose_frontmatter, split_frontmatter};
use crate::engine::reservation;
use crate::engine::store::Store;
use crate::engine::template;

/// Load a template from the configured templates directory, falling back to a built-in default.
fn load_template(root: &Path, config: &Config, doc_type: &str) -> String {
    let template_path = root
        .join(&config.filesystem.templates.dir)
        .join(format!("{}.md", doc_type.to_lowercase()));
    if template_path.exists() {
        fs::read_to_string(&template_path).unwrap_or_else(|_| default_template(doc_type))
    } else {
        default_template(doc_type)
    }
}

fn story_template(doc_type: &str) -> String {
    format!(
        r#"---
title: "{{title}}"
type: {}
status: draft
author: "{{author}}"
date: {{date}}
tags: []
related: []
---

## Acceptance Criteria

### AC: example-criterion

Given a precondition
When an action is taken
Then an expected outcome occurs
"#,
        doc_type.to_lowercase()
    )
}

fn default_template(doc_type: &str) -> String {
    match doc_type.to_lowercase().as_str() {
        "story" => r#"---
title: "{title}"
type: story
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Context

TODO: Describe the background and motivation.

## Acceptance Criteria

- **Given** a precondition
  **When** an action is taken
  **Then** an expected outcome occurs

## Scope

### In Scope

- TODO

### Out of Scope

- TODO
"#
        .to_string(),

        "iteration" => r#"---
title: "{title}"
type: iteration
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Changes

- TODO

## Test Plan

- TODO

## Notes

TODO
"#
        .to_string(),

        "spec" => r#"---
title: "{title}"
type: spec
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Summary

TODO
"#
        .to_string(),

        _ => format!(
            r#"---
title: "{{title}}"
type: {}
status: draft
author: "{{author}}"
date: {{date}}
tags: []
---

## Summary

TODO
"#,
            doc_type.to_lowercase()
        ),
    }
}

/// Create a document on the filesystem. Handles numbering, template resolution, and file writing.
/// Returns the absolute path to the created file.
#[allow(clippy::too_many_arguments)]
pub fn create_document(
    root: &Path,
    config: &Config,
    doc_type: &str,
    dir: &str,
    prefix: &str,
    title: &str,
    author: &str,
    numbering_strategy: &NumberingStrategy,
    subdirectory: bool,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<PathBuf> {
    let target_dir = root.join(dir);
    fs::create_dir_all(&target_dir)?;

    let (numbering, pre_computed_id) = match numbering_strategy {
        NumberingStrategy::Sqids => {
            let sqids_config = config.documents.sqids.as_ref().ok_or_else(|| {
                anyhow!(
                    "type '{}' uses sqids numbering but no [numbering.sqids] config found",
                    doc_type
                )
            })?;
            (Some((numbering_strategy, sqids_config)), None)
        }
        NumberingStrategy::Reserved => {
            let reserved_cfg = config.documents.reserved.as_ref().ok_or_else(|| {
                anyhow!(
                    "type '{}' uses reserved numbering but no [numbering.reserved] config found",
                    doc_type
                )
            })?;
            let num = reservation::reserve_next(
                root,
                &reserved_cfg.remote,
                &prefix.to_uppercase(),
                reserved_cfg.max_retries,
                &target_dir,
                &on_progress,
            )?;
            let id = match reserved_cfg.format {
                ReservedFormat::Incremental => format!("{:03}", num),
                ReservedFormat::Sqids => {
                    let sqids_config = config.documents.sqids.as_ref().ok_or_else(|| {
                        anyhow!("reserved format 'sqids' requires [numbering.sqids] config")
                    })?;
                    let alphabet = template::shuffle_alphabet(&sqids_config.salt);
                    let sqids = sqids::Sqids::builder()
                        .alphabet(alphabet)
                        .min_length(sqids_config.min_length)
                        .blocklist(std::collections::HashSet::new())
                        .build()?;
                    sqids.encode(&[num as u64])?.to_lowercase()
                }
            };
            (None, Some(id))
        }
        NumberingStrategy::Incremental => (None, None),
    };

    let filename = template::resolve_filename(
        &config.documents.naming.pattern,
        prefix,
        title,
        &target_dir,
        numbering,
        pre_computed_id.as_deref(),
    )
    .map_err(|e| anyhow!("{}", e))?;

    let date = Local::now().format("%Y-%m-%d").to_string();
    let vars = vec![
        ("title", title),
        ("author", author),
        ("date", date.as_str()),
        ("type", doc_type),
    ];

    if subdirectory {
        let dir_name = filename.trim_end_matches(".md");
        let spec_dir = target_dir.join(dir_name);
        fs::create_dir_all(&spec_dir)?;

        let index_template = load_template(root, config, doc_type);
        let index_content = template::render_template(&index_template, &vars);
        let index_path = spec_dir.join("index.md");
        fs::write(&index_path, index_content)?;

        let story_content = template::render_template(&story_template(doc_type), &vars);
        fs::write(spec_dir.join("story.md"), story_content)?;

        return Ok(index_path);
    }

    let target_path = target_dir.join(&filename);
    let template_content = load_template(root, config, doc_type);
    let content = template::render_template(&template_content, &vars);
    fs::write(&target_path, content)?;

    Ok(target_path)
}

/// Delete a filesystem document by ID or shorthand.
pub fn delete_document(root: &Path, store: &Store, doc_id: &str) -> Result<()> {
    let doc = store
        .get(Path::new(doc_id))
        .or_else(|| store.resolve_shorthand(doc_id).ok())
        .ok_or_else(|| anyhow!("could not resolve document: {}", doc_id))?;

    let full_path = root.join(&doc.path);
    if !full_path.exists() {
        return Err(anyhow!("file not found: {}", doc.path.display()));
    }
    fs::remove_file(&full_path)?;
    Ok(())
}

/// Update frontmatter fields of a filesystem document.
/// Resolves `doc_id` via the store, then performs in-place YAML key replacement.
pub fn update_document(
    root: &Path,
    store: &Store,
    doc_id: &str,
    updates: &[(&str, &str)],
) -> Result<()> {
    if updates.iter().any(|(k, _)| *k == "body") {
        bail!("--body and --body-file are not supported for filesystem documents; edit the file directly");
    }

    let doc = store
        .get(Path::new(doc_id))
        .or_else(|| store.resolve_shorthand(doc_id).ok())
        .ok_or_else(|| anyhow!("could not resolve document: {}", doc_id))?;

    let full_path = root.join(&doc.path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter(&content)?;

    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    for (key, value) in updates {
        let prefix = format!("{}:", key);
        if let Some(line) = lines
            .iter_mut()
            .find(|l| l.trim_start().starts_with(&prefix))
        {
            *line = format!("{}: {}", key, value);
        }
    }

    let new_yaml = lines.join("\n");
    let new_content = compose_frontmatter(&new_yaml, &body);
    fs::write(&full_path, new_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::DocMeta;
    use tempfile::TempDir;

    fn write_doc(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn fs_ops_round_trips_assignees_exact_order() {
        let tmp = TempDir::new().unwrap();
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: alice
date: 2026-05-12
tags: []
assignees:
  - alice
  - claude-bot
related: []
---

Body.
"#;
        let path = write_doc(tmp.path(), "RFC-001.md", content);
        let raw = fs::read_to_string(&path).unwrap();
        let meta = DocMeta::parse(&raw).unwrap();
        assert_eq!(
            meta.assignees,
            vec!["alice".to_string(), "claude-bot".to_string()],
            "assignees must round-trip in exact order"
        );
    }

    #[test]
    fn fs_ops_accepts_free_form_assignee_strings() {
        let tmp = TempDir::new().unwrap();
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: alice
date: 2026-05-12
tags: []
assignees:
  - "not-a-real-github-user"
  - "user@example.com"
  - "Some Free Form"
related: []
---

Body.
"#;
        let path = write_doc(tmp.path(), "RFC-002.md", content);
        let raw = fs::read_to_string(&path).unwrap();
        let meta = DocMeta::parse(&raw).unwrap();
        assert_eq!(
            meta.assignees,
            vec![
                "not-a-real-github-user".to_string(),
                "user@example.com".to_string(),
                "Some Free Form".to_string(),
            ],
            "free-form assignee strings must pass through verbatim"
        );
    }

    #[test]
    fn fs_ops_update_document_preserves_assignees_when_updating_other_fields() {
        use crate::engine::config::{
            Config, Directories, DocumentConfig, FilesystemConfig, Naming, NumberingStrategy,
            StoreBackend, Templates, TypeDef, UiConfig,
        };

        let tmp = TempDir::new().unwrap();
        let rfcs_dir = tmp.path().join("docs/rfcs");
        fs::create_dir_all(&rfcs_dir).unwrap();
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: alice
date: 2026-05-12
tags: []
assignees:
  - alice
  - claude-bot
related: []
---

Body.
"#;
        write_doc(&rfcs_dir, "RFC-001.md", content);

        let config = Config {
            documents: DocumentConfig {
                types: vec![TypeDef {
                    name: "rfc".to_string(),
                    plural: "rfcs".to_string(),
                    dir: "docs/rfcs".to_string(),
                    prefix: "RFC".to_string(),
                    icon: None,
                    numbering: NumberingStrategy::Incremental,
                    subdirectory: false,
                    store: StoreBackend::Filesystem,
                    singleton: false,
                    parent_type: None,
                }],
                naming: Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                },
                sqids: None,
                reserved: None,
                github: None,
            },
            filesystem: FilesystemConfig {
                directories: Directories {
                    rfcs: "docs/rfcs".to_string(),
                    adrs: "docs/adrs".to_string(),
                    stories: "docs/stories".to_string(),
                    iterations: "docs/iterations".to_string(),
                },
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            ui: UiConfig::default(),
            rules: vec![],
            ref_count_ceiling: 0,
            certification: Default::default(),
            coordination: None,
            orchestration: None,
        };

        let store = Store::load(tmp.path(), &config).unwrap();
        update_document(tmp.path(), &store, "RFC-001", &[("status", "accepted")]).unwrap();

        let updated = fs::read_to_string(rfcs_dir.join("RFC-001.md")).unwrap();
        let meta = DocMeta::parse(&updated).unwrap();
        assert_eq!(
            meta.assignees,
            vec!["alice".to_string(), "claude-bot".to_string()],
            "update_document must not strip assignees"
        );
        assert_eq!(meta.status.to_string(), "accepted");
    }
}
