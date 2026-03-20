use crate::cli::json::doc_to_json;
use crate::engine::config::{Config, NumberingStrategy, ReservedFormat};
use crate::engine::document::DocMeta;
use crate::engine::reservation;
use crate::engine::template;
use anyhow::{anyhow, Result};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(
    root: &Path,
    config: &Config,
    doc_type: &str,
    title: &str,
    author: &str,
) -> Result<PathBuf> {
    let type_def = config.type_by_name(doc_type)
        .ok_or_else(|| anyhow!("unknown doc type: '{}'. valid types: {}", doc_type,
            config.types.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")))?;
    let dir = &type_def.dir;

    let target_dir = root.join(dir);
    fs::create_dir_all(&target_dir)?;

    let (numbering, pre_computed_id) = match type_def.numbering {
        NumberingStrategy::Sqids => {
            let sqids_config = config.sqids.as_ref()
                .ok_or_else(|| anyhow!("type '{}' uses sqids numbering but no [numbering.sqids] config found", doc_type))?;
            (Some((&type_def.numbering, sqids_config)), None)
        }
        NumberingStrategy::Reserved => {
            let reserved_cfg = config.reserved.as_ref()
                .ok_or_else(|| anyhow!("type '{}' uses reserved numbering but no [numbering.reserved] config found", doc_type))?;
            let num = reservation::reserve_next(
                root,
                &reserved_cfg.remote,
                &type_def.prefix.to_uppercase(),
                reserved_cfg.max_retries,
            )?;
            let id = match reserved_cfg.format {
                ReservedFormat::Incremental => format!("{:03}", num),
                ReservedFormat::Sqids => {
                    let sqids_config = config.sqids.as_ref()
                        .ok_or_else(|| anyhow!("reserved format 'sqids' requires [numbering.sqids] config"))?;
                    let alphabet = template::shuffle_alphabet(&sqids_config.salt);
                    let sqids = sqids::Sqids::builder()
                        .alphabet(alphabet)
                        .min_length(sqids_config.min_length)
                        .blocklist(std::collections::HashSet::new())
                        .build()
                        .expect("valid sqids config");
                    sqids.encode(&[num as u64]).expect("sqids encode").to_lowercase()
                }
            };
            (None, Some(id))
        }
        NumberingStrategy::Incremental => (None, None),
    };
    let filename = template::resolve_filename(
        &config.naming.pattern, doc_type, title, &target_dir, numbering,
        pre_computed_id.as_deref(),
    );
    let target_path = target_dir.join(&filename);

    let template_path = root
        .join(&config.templates.dir)
        .join(format!("{}.md", doc_type.to_lowercase()));
    let template_content = if template_path.exists() {
        fs::read_to_string(&template_path)?
    } else {
        default_template(doc_type)
    };

    let date = Local::now().format("%Y-%m-%d").to_string();
    let vars = vec![
        ("title", title),
        ("author", author),
        ("date", date.as_str()),
        ("type", doc_type),
    ];
    let content = template::render_template(&template_content, &vars);

    fs::write(&target_path, content)?;

    Ok(target_path)
}

pub fn run_json(
    root: &Path,
    config: &Config,
    doc_type: &str,
    title: &str,
    author: &str,
) -> Result<String> {
    let path = run(root, config, doc_type, title, author)?;
    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

    let content = fs::read_to_string(&path)?;
    let mut meta = DocMeta::parse(&content)?;
    meta.path = relative;

    let json = doc_to_json(&meta);
    Ok(serde_json::to_string_pretty(&json)?)
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
