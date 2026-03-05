use crate::cli::json::doc_to_json;
use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType};
use crate::engine::template;
use anyhow::Result;
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
    let parsed: DocType = doc_type.parse()?;
    let dir = match parsed {
        DocType::Rfc => &config.directories.rfcs,
        DocType::Adr => &config.directories.adrs,
        DocType::Story => &config.directories.stories,
        DocType::Iteration => &config.directories.iterations,
    };

    let target_dir = root.join(dir);
    fs::create_dir_all(&target_dir)?;

    let filename =
        template::resolve_filename(&config.naming.pattern, doc_type, title, &target_dir);
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
