use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chrono::Local;

use crate::engine::config::{Config, NumberingStrategy, ReservedFormat};
use crate::engine::document::{compose_frontmatter, split_frontmatter};
use crate::engine::reservation;
use crate::engine::store::Store;
use crate::engine::template;

/// Load the template for `doc_type` from the configured templates directory.
/// Resolution order: a per-type `{type}.md` override, then the shared
/// `template.md`, then the embedded generic default. The tool is config-driven,
/// so it ships no per-type templates; `{type}` in the body is substituted at
/// creation time, letting one template serve every type.
fn load_template(root: &Path, config: &Config, doc_type: &str) -> String {
    let dir = root.join(&config.filesystem.templates.dir);

    let per_type = dir.join(format!("{}.md", doc_type.to_lowercase()));
    if per_type.exists() {
        return fs::read_to_string(&per_type).unwrap_or_else(|_| default_template().to_string());
    }

    let shared = dir.join("template.md");
    if shared.exists() {
        return fs::read_to_string(&shared).unwrap_or_else(|_| default_template().to_string());
    }

    default_template().to_string()
}

/// The single generic template `init` materializes to `template.md`. It carries
/// the authoring conventions (intent/guidance comments, `{key}` substitution)
/// rather than any type-specific sections; `{type}` is filled in per document.
pub(crate) fn default_template() -> &'static str {
    r#"---
title: "{title}"
type: {type}
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---
<!-- intent: state in one line what this document is for, then delete this comment -->

## Summary
<!-- guidance: what this document covers and why it exists. Rename or replace these
     sections to suit the type; the headings below are only a starting point. -->

## Detail
<!-- guidance: the substance of the document. Add as many sections as the type needs. -->

<!-- Authoring conventions this tool relies on:
       - an intent comment at the top of the body states the document's purpose; agents read it.
       - a guidance comment under a heading describes what belongs in that section.
       - {title}, {author}, {date}, and {type} are substituted when a document is created.
     Edit this file (template.md) to change the default for every new document, or add
     a {type}.md alongside it to override a single type. HTML comments render invisibly. -->
"#
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
    let doc = store
        .get(Path::new(doc_id))
        .or_else(|| store.resolve_shorthand(doc_id).ok())
        .ok_or_else(|| anyhow!("could not resolve document: {}", doc_id))?;

    let full_path = root.join(&doc.path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter(&content)?;

    let mut new_body = body;
    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    for (key, value) in updates {
        if *key == "body" {
            new_body = value.to_string();
            continue;
        }
        let prefix = format!("{}:", key);
        if let Some(line) = lines
            .iter_mut()
            .find(|l| l.trim_start().starts_with(&prefix))
        {
            *line = format!("{}: {}", key, value);
        }
    }

    let new_yaml = lines.join("\n");
    let new_content = compose_frontmatter(&new_yaml, &new_body);
    fs::write(&full_path, new_content)?;
    Ok(())
}
