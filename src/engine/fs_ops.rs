use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Local;

use crate::engine::config::{Config, NumberingStrategy, ReservedFormat, TypeDef};
use crate::engine::document::{apply_attrs, compose_frontmatter, split_frontmatter, DocMeta};
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

/// Overwrite a rendered template's frontmatter `status:` line with the type's
/// first lifecycle state, so a document is born inside its lifecycle regardless
/// of what the template hardcodes (BUG-002). Default lifecycles start at
/// `draft`, so standard types are unaffected.
fn seed_lifecycle_status(content: &str, status: &str) -> Result<String> {
    let (yaml, body) = split_frontmatter(content)?;
    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    if let Some(line) = lines
        .iter_mut()
        .find(|l| l.trim_start().starts_with("status:"))
    {
        *line = format!("status: {}", status);
    }
    Ok(compose_frontmatter(&lines.join("\n"), &body))
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

    let seed_status = config
        .type_by_name(doc_type)
        .map(|t| t.lifecycle.seed_status())
        .unwrap_or("draft");

    if subdirectory {
        let dir_name = filename.trim_end_matches(".md");
        let spec_dir = target_dir.join(dir_name);
        fs::create_dir_all(&spec_dir)?;

        let index_template = load_template(root, config, doc_type);
        let index_content = template::render_template(&index_template, &vars);
        let index_content = seed_lifecycle_status(&index_content, seed_status)?;
        let index_path = spec_dir.join("index.md");
        fs::write(&index_path, index_content)?;

        return Ok(index_path);
    }

    let target_path = target_dir.join(&filename);
    let template_content = load_template(root, config, doc_type);
    let content = template::render_template(&template_content, &vars);
    let content = seed_lifecycle_status(&content, seed_status)?;
    fs::write(&target_path, content)?;

    Ok(target_path)
}

/// Author a single child document as a `.md` directly inside `target_dir`.
///
/// Unlike [`create_document`], which decides between a flat file and a
/// `<dir>/index.md` subdir from the type's `subdirectory` flag, this writes
/// exactly one `.md` into the explicit `target_dir`. Numbering scans
/// `target_dir`, so a subdir child's `{n:03}` is local to its parent's
/// subdirectory rather than the type's flat `dir`. Used by `create --parent`
/// to place a child alongside a promoted parent's `index.md`.
pub fn create_child_in_dir(
    root: &Path,
    config: &Config,
    child_type_def: &TypeDef,
    target_dir: &Path,
    title: &str,
    author: &str,
    body: Option<&str>,
) -> Result<PathBuf> {
    fs::create_dir_all(target_dir)?;

    let numbering = match &child_type_def.numbering {
        NumberingStrategy::Sqids => {
            let sqids_config = config.documents.sqids.as_ref().ok_or_else(|| {
                anyhow!(
                    "type '{}' uses sqids numbering but no [numbering.sqids] config found",
                    child_type_def.name
                )
            })?;
            Some((&child_type_def.numbering, sqids_config))
        }
        _ => None,
    };

    let filename = template::resolve_filename(
        &config.documents.naming.pattern,
        &child_type_def.prefix,
        title,
        target_dir,
        numbering,
        None,
    )
    .map_err(|e| anyhow!("{}", e))?;

    let date = Local::now().format("%Y-%m-%d").to_string();
    let vars = vec![
        ("title", title),
        ("author", author),
        ("date", date.as_str()),
        ("type", child_type_def.name.as_str()),
    ];
    let template_content = load_template(root, config, &child_type_def.name);
    let content = template::render_template(&template_content, &vars);
    let seed_status = child_type_def.lifecycle.seed_status();
    let content = seed_lifecycle_status(&content, seed_status)?;

    let target_path = target_dir.join(&filename);
    fs::write(&target_path, &content)?;

    if let Some(body_text) = body {
        let written = fs::read_to_string(&target_path)?;
        let (yaml, _) = split_frontmatter(&written)?;
        let new_content = format!("---\n{}\n---\n\n{}\n", yaml.trim(), body_text);
        fs::write(&target_path, new_content)?;
    }

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
    update_document_with_type(root, store, doc_id, updates, None)
}

const RESERVED_UPDATE_KEYS: &[&str] = &["status", "title", "body", "author", "assignee"];

/// Update a filesystem document's frontmatter. Reserved keys (status/title/body/
/// author) follow the in-place replace path; any other key is a declared custom
/// attribute, coerced and validated via [`apply_attrs`] against `type_def` and
/// then written back (replacing an existing line or appending a new one). When
/// `type_def` is absent, non-reserved keys are treated as plain replacements
/// (legacy behaviour for callers without type context).
pub fn update_document_with_type(
    root: &Path,
    store: &Store,
    doc_id: &str,
    updates: &[(&str, &str)],
    type_def: Option<&TypeDef>,
) -> Result<()> {
    let doc = store
        .get(Path::new(doc_id))
        .or_else(|| store.resolve_shorthand(doc_id).ok())
        .ok_or_else(|| anyhow!("could not resolve document: {}", doc_id))?;

    let full_path = root.join(&doc.path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter(&content)?;

    let attr_updates: Vec<(&str, &str)> = updates
        .iter()
        .filter(|(k, _)| !RESERVED_UPDATE_KEYS.contains(k))
        .copied()
        .collect();

    let coerced_attrs: Vec<(String, String)> = match type_def {
        Some(td) if !attr_updates.is_empty() => {
            let schema = &td.attributes;
            let mut meta = DocMeta::parse_with_schema(&content, schema)
                .with_context(|| format!("parsing {}", doc.path.display()))?;
            apply_attrs(td, &mut meta, &attr_updates)?;
            attr_updates
                .iter()
                .map(|(key, _)| {
                    let value = meta
                        .attributes
                        .get(*key)
                        .expect("attr inserted by apply_attrs");
                    let scalar = serde_yaml::to_string(value)
                        .map(|s| s.trim_end().to_string())
                        .unwrap_or_default();
                    ((*key).to_string(), scalar)
                })
                .collect()
        }
        _ => Vec::new(),
    };

    let mut new_body = body;
    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    for (key, value) in updates {
        if *key == "body" {
            new_body = value.to_string();
            continue;
        }
        // `assignee` is absent-when-unset, so unlike the other reserved keys it
        // must insert a line when missing and remove it when cleared with "".
        if *key == "assignee" {
            let pos = lines
                .iter()
                .position(|l| l.trim_start().starts_with("assignee:"));
            match (pos, value.is_empty()) {
                (Some(i), true) => {
                    lines.remove(i);
                }
                (Some(i), false) => lines[i] = format!("assignee: {}", value),
                (None, true) => {}
                (None, false) => lines.push(format!("assignee: {}", value)),
            }
            continue;
        }
        if RESERVED_UPDATE_KEYS.contains(key) {
            let prefix = format!("{}:", key);
            if let Some(line) = lines
                .iter_mut()
                .find(|l| l.trim_start().starts_with(&prefix))
            {
                *line = format!("{}: {}", key, value);
            }
        }
    }

    for (key, scalar) in &coerced_attrs {
        let prefix = format!("{}:", key);
        if let Some(line) = lines
            .iter_mut()
            .find(|l| l.trim_start().starts_with(&prefix))
        {
            *line = format!("{}: {}", key, scalar);
        } else {
            lines.push(format!("{}: {}", key, scalar));
        }
    }

    let new_yaml = lines.join("\n");
    let new_content = compose_frontmatter(&new_yaml, &new_body);
    fs::write(&full_path, new_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Lifecycle;
    use tempfile::TempDir;

    fn noop_progress(_: reservation::ReservationProgress) {}

    fn status_line(path: &Path) -> String {
        let content = fs::read_to_string(path).unwrap();
        content
            .lines()
            .find(|l| l.trim_start().starts_with("status:"))
            .expect("frontmatter has a status line")
            .to_string()
    }

    fn config_with_bug_type(subdirectory: bool) -> Config {
        let mut config = Config::default();
        let mut bug = config.documents.types[0].clone();
        bug.name = "bug".to_string();
        bug.plural = "bugs".to_string();
        bug.dir = "docs/bugs".to_string();
        bug.prefix = "BUG".to_string();
        bug.numbering = NumberingStrategy::Incremental;
        bug.subdirectory = subdirectory;
        bug.lifecycle = Lifecycle {
            states: vec!["reported".into(), "triaged".into(), "fixed".into()],
            edges: vec![],
        };
        config.documents.types.push(bug);
        config
    }

    #[test]
    fn create_seeds_first_lifecycle_state_for_non_draft_type() {
        let tmp = TempDir::new().unwrap();
        let config = config_with_bug_type(false);

        let path = create_document(
            tmp.path(),
            &config,
            "bug",
            "docs/bugs",
            "BUG",
            "Broken thing",
            "alice",
            &NumberingStrategy::Incremental,
            false,
            noop_progress,
        )
        .unwrap();

        assert_eq!(status_line(&path), "status: reported");
    }

    #[test]
    fn create_seeds_first_lifecycle_state_in_subdirectory_index() {
        let tmp = TempDir::new().unwrap();
        let config = config_with_bug_type(true);

        let path = create_document(
            tmp.path(),
            &config,
            "bug",
            "docs/bugs",
            "BUG",
            "Broken thing",
            "alice",
            &NumberingStrategy::Incremental,
            true,
            noop_progress,
        )
        .unwrap();

        assert_eq!(status_line(&path), "status: reported");
    }

    #[test]
    fn create_keeps_draft_for_default_lifecycle_type() {
        let tmp = TempDir::new().unwrap();
        let config = Config::default();

        let path = create_document(
            tmp.path(),
            &config,
            "iteration",
            "docs/iterations",
            "ITERATION",
            "Some Work",
            "bob",
            &NumberingStrategy::Incremental,
            false,
            noop_progress,
        )
        .unwrap();

        assert_eq!(status_line(&path), "status: draft");
    }

    #[test]
    fn create_child_seeds_first_lifecycle_state() {
        let tmp = TempDir::new().unwrap();
        let config = config_with_bug_type(false);
        let child_type = config.type_by_name("bug").unwrap().clone();
        let target_dir = tmp.path().join("docs/stories/STORY-001/bugs");

        let path = create_child_in_dir(
            tmp.path(),
            &config,
            &child_type,
            &target_dir,
            "Child Bug",
            "alice",
            None,
        )
        .unwrap();

        assert_eq!(status_line(&path), "status: reported");
    }

    #[test]
    fn create_child_seed_survives_body_recompose() {
        let tmp = TempDir::new().unwrap();
        let config = config_with_bug_type(false);
        let child_type = config.type_by_name("bug").unwrap().clone();
        let target_dir = tmp.path().join("docs/stories/STORY-001/bugs");

        let path = create_child_in_dir(
            tmp.path(),
            &config,
            &child_type,
            &target_dir,
            "Child Bug",
            "alice",
            Some("some custom body"),
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(status_line(&path), "status: reported");
        assert!(content.contains("some custom body"));
    }
}
