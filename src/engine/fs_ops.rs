use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use chrono::Local;

use crate::engine::config::{Config, NumberingStrategy, ReservedFormat};
use crate::engine::document::split_frontmatter;
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
title: "{{title}}"
type: spec
status: draft
author: "{{author}}"
date: {{date}}
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
        doc_type,
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
    let new_content = format!("---\n{}\n---\n{}", new_yaml, body);
    fs::write(&full_path, new_content)?;
    Ok(())
}
