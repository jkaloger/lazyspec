use crate::cli::json::doc_to_json;
use crate::engine::config::{Config, NumberingStrategy, ReservedFormat, StoreBackend};
use crate::engine::document::DocMeta;
use crate::engine::gh::GhCli;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::reservation;
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use crate::engine::template;
use anyhow::{anyhow, Result};
use chrono::Local;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(
    root: &Path,
    config: &Config,
    doc_type: &str,
    title: &str,
    author: &str,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<PathBuf> {
    let type_def = config.type_by_name(doc_type)
        .ok_or_else(|| anyhow!("unknown doc type: '{}'. valid types: {}", doc_type,
            config.documents.types.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")))?;

    if type_def.store == StoreBackend::GithubIssues {
        let gh_config = config.documents.github.as_ref()
            .ok_or_else(|| anyhow!("type '{}' uses github-issues store but no [github] config found", doc_type))?;
        let repo = gh_config.repo.as_ref()
            .ok_or_else(|| anyhow!("type '{}' uses github-issues store but no github.repo configured", doc_type))?;
        let store = GithubIssuesStore {
            client: GhCli::new(),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: RefCell::new(IssueMap::load(root)?),
            issue_cache: IssueCache::new(root),
        };
        let created = store.create(type_def, title, author, "")?;
        return Ok(root.join(&created.path));
    }

    let dir = &type_def.dir;

    let target_dir = root.join(dir);
    fs::create_dir_all(&target_dir)?;

    let (numbering, pre_computed_id) = match type_def.numbering {
        NumberingStrategy::Sqids => {
            let sqids_config = config.documents.sqids.as_ref()
                .ok_or_else(|| anyhow!("type '{}' uses sqids numbering but no [numbering.sqids] config found", doc_type))?;
            (Some((&type_def.numbering, sqids_config)), None)
        }
        NumberingStrategy::Reserved => {
            let reserved_cfg = config.documents.reserved.as_ref()
                .ok_or_else(|| anyhow!("type '{}' uses reserved numbering but no [numbering.reserved] config found", doc_type))?;
            let num = reservation::reserve_next(
                root,
                &reserved_cfg.remote,
                &type_def.prefix.to_uppercase(),
                reserved_cfg.max_retries,
                &target_dir,
                &on_progress,
            )?;
            let id = match reserved_cfg.format {
                ReservedFormat::Incremental => format!("{:03}", num),
                ReservedFormat::Sqids => {
                    let sqids_config = config.documents.sqids.as_ref()
                        .ok_or_else(|| anyhow!("reserved format 'sqids' requires [numbering.sqids] config"))?;
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
        &config.documents.naming.pattern, doc_type, title, &target_dir, numbering,
        pre_computed_id.as_deref(),
    ).map_err(|e| anyhow!("{}", e))?;
    let date = Local::now().format("%Y-%m-%d").to_string();
    let vars = vec![
        ("title", title),
        ("author", author),
        ("date", date.as_str()),
        ("type", doc_type),
    ];

    if type_def.subdirectory {
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

pub fn run_json(
    root: &Path,
    config: &Config,
    doc_type: &str,
    title: &str,
    author: &str,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<String> {
    let path = run(root, config, doc_type, title, author, on_progress)?;
    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

    let content = fs::read_to_string(&path)?;
    let mut meta = DocMeta::parse(&content)?;
    meta.path = relative;

    let json = doc_to_json(&meta);
    Ok(serde_json::to_string_pretty(&json)?)
}

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

        "spec" => format!(
            r#"---
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
        ),

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
