use crate::engine::config::Config;
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
    let dir = match doc_type.to_lowercase().as_str() {
        "rfc" => &config.directories.rfcs,
        "adr" => &config.directories.adrs,
        "story" => &config.directories.stories,
        "iteration" => &config.directories.iterations,
        _ => return Err(anyhow!("unknown doc type: {}", doc_type)),
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
