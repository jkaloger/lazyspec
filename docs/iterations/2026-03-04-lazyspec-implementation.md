---
title: lazyspec Implementation
type: iteration
status: accepted
author: jkaloger
date: 2026-03-04
tags:
- implementation
related:
- implements: STORY-001
- implements: STORY-002
- implements: STORY-003
---


# lazyspec Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust TUI and CLI for managing project specs, RFCs, ADRs, and plans with frontmatter-based metadata, linking, and fuzzy search.

**Architecture:** Single binary with three modules: `core` (document model, store, queries, templates, linking), `cli` (clap subcommands), `tui` (ratatui dashboard). Core is the shared foundation. CLI and TUI are thin consumers.

**Tech Stack:** Rust, ratatui + crossterm, clap (derive), serde + serde_yaml, pulldown-cmark, tui-markdown, nucleo, notify, chrono, toml.

**Design doc:** `docs/plans/2026-03-04-lazyspec-design.md`

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/core/mod.rs`
- Create: `src/cli/mod.rs`
- Create: `src/tui/mod.rs`

**Step 1: Initialize cargo project**

Run: `cargo init --name lazyspec`

**Step 2: Add dependencies to Cargo.toml**

```toml
[package]
name = "lazyspec"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
pulldown-cmark = "0.12"
tui-markdown = "0.3"
nucleo = "0.5"
notify = "7"
chrono = { version = "0.4", features = ["serde"] }
toml = "0.8"
anyhow = "1"
```

**Step 3: Create module structure**

`src/main.rs`:
```rust
mod cli;
mod core;
mod tui;

fn main() {
    println!("lazyspec");
}
```

`src/core/mod.rs`:
```rust
pub mod config;
pub mod document;
pub mod store;
```

`src/core/config.rs`, `src/core/document.rs`, `src/core/store.rs`: empty files.

`src/cli/mod.rs`:
```rust
// CLI commands
```

`src/tui/mod.rs`:
```rust
// TUI application
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors (warnings ok at this stage).

**Step 5: Commit**

```bash
git add -A
git commit -m "chore: scaffold project with module structure and dependencies"
```

---

### Task 2: Core - Document Model & Frontmatter Parsing

**Files:**
- Create: `src/core/document.rs`
- Create: `tests/document_test.rs`

**Step 1: Write failing test for frontmatter parsing**

`tests/document_test.rs`:
```rust
use lazyspec::core::document::{DocMeta, DocType, Status, Relation, RelationType};
use chrono::NaiveDate;

#[test]
fn parse_frontmatter_from_markdown() {
    let content = r#"---
title: "Adopt Event Sourcing"
type: adr
status: draft
author: jkaloger
date: 2026-03-04
tags: [architecture, events]
related:
  - implements: rfcs/RFC-001-event-sourcing.md
---

## Context

Some body content here.
"#;

    let meta = DocMeta::parse(content).unwrap();

    assert_eq!(meta.title, "Adopt Event Sourcing");
    assert_eq!(meta.doc_type, DocType::Adr);
    assert_eq!(meta.status, Status::Draft);
    assert_eq!(meta.author, "jkaloger");
    assert_eq!(meta.date, NaiveDate::from_ymd_opt(2026, 3, 4).unwrap());
    assert_eq!(meta.tags, vec!["architecture", "events"]);
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].rel_type, RelationType::Implements);
    assert_eq!(meta.related[0].target, "rfcs/RFC-001-event-sourcing.md");
}

#[test]
fn parse_frontmatter_minimal() {
    let content = r#"---
title: "Simple Doc"
type: rfc
status: review
author: someone
date: 2026-01-01
tags: []
---

Body.
"#;

    let meta = DocMeta::parse(content).unwrap();
    assert_eq!(meta.title, "Simple Doc");
    assert_eq!(meta.doc_type, DocType::Rfc);
    assert!(meta.related.is_empty());
}

#[test]
fn parse_frontmatter_invalid_yaml() {
    let content = "no frontmatter here";
    assert!(DocMeta::parse(content).is_err());
}

#[test]
fn extract_body_skips_frontmatter() {
    let content = r#"---
title: "Test"
type: spec
status: draft
author: a
date: 2026-01-01
tags: []
---

## Body

Content here.
"#;

    let body = DocMeta::extract_body(content).unwrap();
    assert!(body.contains("## Body"));
    assert!(body.contains("Content here."));
    assert!(!body.contains("title:"));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test document_test`
Expected: FAIL - module not found.

**Step 3: Implement document model**

`src/core/document.rs`:
```rust
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocType {
    Rfc,
    Adr,
    Spec,
    Plan,
}

impl std::fmt::Display for DocType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocType::Rfc => write!(f, "RFC"),
            DocType::Adr => write!(f, "ADR"),
            DocType::Spec => write!(f, "SPEC"),
            DocType::Plan => write!(f, "PLAN"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Review,
    Accepted,
    Rejected,
    Superseded,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Draft => write!(f, "draft"),
            Status::Review => write!(f, "review"),
            Status::Accepted => write!(f, "accepted"),
            Status::Rejected => write!(f, "rejected"),
            Status::Superseded => write!(f, "superseded"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationType {
    Implements,
    Supersedes,
    Blocks,
    RelatedTo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub rel_type: RelationType,
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct DocMeta {
    pub path: PathBuf,
    pub title: String,
    pub doc_type: DocType,
    pub status: Status,
    pub author: String,
    pub date: NaiveDate,
    pub tags: Vec<String>,
    pub related: Vec<Relation>,
}

#[derive(Deserialize)]
struct RawFrontmatter {
    title: String,
    #[serde(rename = "type")]
    doc_type: DocType,
    status: Status,
    author: String,
    date: NaiveDate,
    tags: Vec<String>,
    #[serde(default)]
    related: Vec<serde_yaml::Value>,
}

impl DocMeta {
    pub fn parse(content: &str) -> Result<Self> {
        let (yaml, _) = Self::split_frontmatter(content)?;
        let raw: RawFrontmatter = serde_yaml::from_str(&yaml)?;

        let related = raw
            .related
            .into_iter()
            .filter_map(|v| {
                let map = v.as_mapping()?;
                let (key, val) = map.into_iter().next()?;
                let rel_type = match key.as_str()? {
                    "implements" => RelationType::Implements,
                    "supersedes" => RelationType::Supersedes,
                    "blocks" => RelationType::Blocks,
                    "related-to" => RelationType::RelatedTo,
                    _ => return None,
                };
                let target = val.as_str()?.to_string();
                Some(Relation { rel_type, target })
            })
            .collect();

        Ok(DocMeta {
            path: PathBuf::new(),
            title: raw.title,
            doc_type: raw.doc_type,
            status: raw.status,
            author: raw.author,
            date: raw.date,
            tags: raw.tags,
            related,
        })
    }

    pub fn extract_body(content: &str) -> Result<String> {
        let (_, body) = Self::split_frontmatter(content)?;
        Ok(body)
    }

    fn split_frontmatter(content: &str) -> Result<(String, String)> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(anyhow!("no frontmatter found"));
        }

        let after_first = &trimmed[3..];
        let end = after_first
            .find("\n---")
            .ok_or_else(|| anyhow!("unterminated frontmatter"))?;

        let yaml = after_first[..end].to_string();
        let body = after_first[end + 4..].to_string();

        Ok((yaml, body))
    }
}
```

Also make the core module public. In `src/main.rs`, change `mod core;` to `pub mod core;` (and similarly expose submodules in `src/core/mod.rs`). Add `lib.rs`:

`src/lib.rs`:
```rust
pub mod core;
```

Note: Rust's `core` is a reserved crate name. Rename the module to `spec_core` or use `self::core` carefully. Alternatively, name the module `engine` to avoid conflicts:
- Rename `src/core/` to `src/engine/`
- Update all references from `core` to `engine`

**Step 4: Run tests to verify they pass**

Run: `cargo test --test document_test`
Expected: all 4 tests PASS.

**Step 5: Commit**

```bash
git add src/engine/ src/lib.rs tests/document_test.rs
git commit -m "feat: document model with frontmatter parsing"
```

---

### Task 3: Core - Configuration

**Files:**
- Create: `src/engine/config.rs`
- Create: `tests/config_test.rs`

**Step 1: Write failing test for config parsing**

`tests/config_test.rs`:
```rust
use lazyspec::engine::config::Config;

#[test]
fn parse_config_from_toml() {
    let toml_str = r#"
[directories]
rfcs = "docs/rfcs"
adrs = "docs/adrs"
specs = "docs/specs"
plans = "docs/plans"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.directories.rfcs, "docs/rfcs");
    assert_eq!(config.naming.pattern, "{type}-{n:03}-{title}.md");
}

#[test]
fn default_config() {
    let config = Config::default();
    assert_eq!(config.directories.rfcs, "docs/rfcs");
    assert_eq!(config.directories.adrs, "docs/adrs");
    assert_eq!(config.directories.specs, "docs/specs");
    assert_eq!(config.directories.plans, "docs/plans");
    assert_eq!(config.templates.dir, ".lazyspec/templates");
    assert_eq!(config.naming.pattern, "{type}-{n:03}-{title}.md");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test`
Expected: FAIL.

**Step 3: Implement config**

`src/engine/config.rs`:
```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub directories: Directories,
    pub templates: Templates,
    pub naming: Naming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directories {
    pub rfcs: String,
    pub adrs: String,
    pub specs: String,
    pub plans: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Templates {
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Naming {
    pub pattern: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            directories: Directories {
                rfcs: "docs/rfcs".to_string(),
                adrs: "docs/adrs".to_string(),
                specs: "docs/specs".to_string(),
                plans: "docs/plans".to_string(),
            },
            templates: Templates {
                dir: ".lazyspec/templates".to_string(),
            },
            naming: Naming {
                pattern: "{type}-{n:03}-{title}.md".to_string(),
            },
        }
    }
}

impl Config {
    pub fn parse(toml_str: &str) -> Result<Self> {
        let config: Config = toml::from_str(toml_str)?;
        Ok(config)
    }

    pub fn load(project_root: &std::path::Path) -> Result<Self> {
        let path = project_root.join(".lazyspec.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Self::parse(&content)
        } else {
            Ok(Self::default())
        }
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test config_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/engine/config.rs tests/config_test.rs
git commit -m "feat: config parsing with defaults"
```

---

### Task 4: Core - Store (Loading & Querying)

**Files:**
- Create: `src/engine/store.rs`
- Create: `tests/store_test.rs`

**Step 1: Write failing tests for store**

`tests/store_test.rs`:
```rust
use lazyspec::engine::config::Config;
use lazyspec::engine::document::{DocType, Status};
use lazyspec::engine::store::{Filter, Store};
use std::fs;
use tempfile::TempDir;

fn setup_test_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::create_dir_all(root.join("docs/specs")).unwrap();
    fs::create_dir_all(root.join("docs/plans")).unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-001-event-sourcing.md"),
        r#"---
title: "Event Sourcing"
type: rfc
status: accepted
author: jkaloger
date: 2026-03-01
tags: [architecture]
---

## Summary
Event sourcing proposal.
"#,
    )
    .unwrap();

    fs::write(
        root.join("docs/adrs/ADR-001-adopt-es.md"),
        r#"---
title: "Adopt Event Sourcing"
type: adr
status: draft
author: jkaloger
date: 2026-03-04
tags: [architecture, events]
related:
  - implements: docs/rfcs/RFC-001-event-sourcing.md
---

## Decision
We adopt event sourcing.
"#,
    )
    .unwrap();

    dir
}

#[test]
fn store_loads_all_docs() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    assert_eq!(store.all_docs().len(), 2);
}

#[test]
fn store_filters_by_type() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let filter = Filter {
        doc_type: Some(DocType::Rfc),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Event Sourcing");
}

#[test]
fn store_filters_by_status() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let filter = Filter {
        status: Some(Status::Draft),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Adopt Event Sourcing");
}

#[test]
fn store_gets_body_lazily() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let docs = store.all_docs();
    let rfc = docs.iter().find(|d| d.doc_type == DocType::Rfc).unwrap();
    let body = store.get_body(&rfc.path).unwrap();
    assert!(body.contains("Event sourcing proposal."));
}

#[test]
fn store_resolves_related_docs() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let docs = store.all_docs();
    let adr = docs.iter().find(|d| d.doc_type == DocType::Adr).unwrap();
    let related = store.related_to(&adr.path);
    assert_eq!(related.len(), 1);
}
```

Add `tempfile` as a dev dependency in `Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test store_test`
Expected: FAIL.

**Step 3: Implement store**

`src/engine/store.rs`:
```rust
use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType, RelationType, Status};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Filter {
    pub doc_type: Option<DocType>,
    pub status: Option<Status>,
    pub tag: Option<String>,
}

pub struct Store {
    root: PathBuf,
    docs: HashMap<PathBuf, DocMeta>,
    forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
}

impl Store {
    pub fn load(root: &Path, config: &Config) -> Result<Self> {
        let mut docs = HashMap::new();

        let dirs = [
            (&config.directories.rfcs, DocType::Rfc),
            (&config.directories.adrs, DocType::Adr),
            (&config.directories.specs, DocType::Spec),
            (&config.directories.plans, DocType::Plan),
        ];

        for (dir, _expected_type) in &dirs {
            let full_path = root.join(dir);
            if !full_path.exists() {
                continue;
            }
            for entry in fs::read_dir(&full_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let content = fs::read_to_string(&path)?;
                if let Ok(mut meta) = DocMeta::parse(&content) {
                    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                    meta.path = relative;
                    docs.insert(meta.path.clone(), meta);
                }
            }
        }

        let mut forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();
        let mut reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();

        for (path, meta) in &docs {
            for rel in &meta.related {
                let target = PathBuf::from(&rel.target);
                forward_links
                    .entry(path.clone())
                    .or_default()
                    .push((rel.rel_type.clone(), target.clone()));
                reverse_links
                    .entry(target)
                    .or_default()
                    .push((rel.rel_type.clone(), path.clone()));
            }
        }

        Ok(Store {
            root: root.to_path_buf(),
            docs,
            forward_links,
            reverse_links,
        })
    }

    pub fn all_docs(&self) -> Vec<&DocMeta> {
        self.docs.values().collect()
    }

    pub fn list(&self, filter: &Filter) -> Vec<&DocMeta> {
        self.docs
            .values()
            .filter(|d| {
                if let Some(ref dt) = filter.doc_type {
                    if &d.doc_type != dt {
                        return false;
                    }
                }
                if let Some(ref s) = filter.status {
                    if &d.status != s {
                        return false;
                    }
                }
                if let Some(ref tag) = filter.tag {
                    if !d.tags.contains(tag) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn get(&self, path: &Path) -> Option<&DocMeta> {
        self.docs.get(path)
    }

    pub fn get_body(&self, path: &Path) -> Result<String> {
        let full_path = self.root.join(path);
        let content = fs::read_to_string(&full_path)?;
        DocMeta::extract_body(&content)
    }

    pub fn related_to(&self, path: &Path) -> Vec<(&RelationType, &PathBuf)> {
        let mut results = Vec::new();
        if let Some(fwd) = self.forward_links.get(path) {
            for (rel, target) in fwd {
                results.push((rel, target));
            }
        }
        if let Some(rev) = self.reverse_links.get(path) {
            for (rel, source) in rev {
                results.push((rel, source));
            }
        }
        results
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test store_test`
Expected: all 5 tests PASS.

**Step 5: Commit**

```bash
git add src/engine/store.rs tests/store_test.rs Cargo.toml
git commit -m "feat: store with loading, filtering, and link resolution"
```

---

### Task 5: CLI - init Command

**Files:**
- Create: `src/cli/init.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`
- Create: `tests/cli_init_test.rs`

**Step 1: Write failing test**

`tests/cli_init_test.rs`:
```rust
use std::fs;
use tempfile::TempDir;

#[test]
fn init_creates_config_and_directories() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    assert!(root.join(".lazyspec.toml").exists());
    assert!(root.join("docs/rfcs").is_dir());
    assert!(root.join("docs/adrs").is_dir());
    assert!(root.join("docs/specs").is_dir());
    assert!(root.join("docs/plans").is_dir());
    assert!(root.join(".lazyspec/templates").is_dir());

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert!(content.contains("[directories]"));
}

#[test]
fn init_does_not_overwrite_existing_config() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join(".lazyspec.toml"), "# custom config").unwrap();

    let result = lazyspec::cli::init::run(root);
    assert!(result.is_err());
}
```

**Step 2: Run to verify failure**

Run: `cargo test --test cli_init_test`
Expected: FAIL.

**Step 3: Implement init command**

`src/cli/init.rs`:
```rust
use crate::engine::config::Config;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

pub fn run(root: &Path) -> Result<()> {
    let config_path = root.join(".lazyspec.toml");
    if config_path.exists() {
        bail!(".lazyspec.toml already exists");
    }

    let config = Config::default();

    fs::create_dir_all(root.join(&config.directories.rfcs))?;
    fs::create_dir_all(root.join(&config.directories.adrs))?;
    fs::create_dir_all(root.join(&config.directories.specs))?;
    fs::create_dir_all(root.join(&config.directories.plans))?;
    fs::create_dir_all(root.join(&config.templates.dir))?;

    fs::write(&config_path, config.to_toml()?)?;

    println!("Initialized lazyspec in {}", root.display());
    Ok(())
}
```

Wire up clap in `src/cli/mod.rs`:
```rust
pub mod init;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lazyspec", about = "Manage project specs, RFCs, ADRs, and plans")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize lazyspec in the current project
    Init,
}
```

Update `src/main.rs`:
```rust
pub mod cli;
pub mod engine;
pub mod tui;

use clap::Parser;
use cli::{Cli, Commands};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            let cwd = std::env::current_dir()?;
            cli::init::run(&cwd)?;
        }
        None => {
            // TODO: launch TUI
            println!("TUI not implemented yet");
        }
    }

    Ok(())
}
```

**Step 4: Run tests**

Run: `cargo test --test cli_init_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli/ src/main.rs tests/cli_init_test.rs
git commit -m "feat: init command creates config and directories"
```

---

### Task 6: CLI - create Command (with Templates)

**Files:**
- Create: `src/cli/create.rs`
- Create: `src/engine/template.rs`
- Create: `tests/cli_create_test.rs`

**Step 1: Write failing test for template rendering**

`tests/cli_create_test.rs`:
```rust
use lazyspec::engine::config::Config;
use lazyspec::engine::template::render_template;
use std::fs;
use tempfile::TempDir;

#[test]
fn create_generates_doc_from_template() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Set up directories
    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();

    // Write a template
    fs::write(
        root.join(".lazyspec/templates/rfc.md"),
        r#"---
title: "{title}"
type: rfc
status: draft
author: "{author}"
date: {date}
tags: []
---

## Summary

TODO: Describe the proposal.
"#,
    )
    .unwrap();

    let config = Config::default();
    let path = lazyspec::cli::create::run(
        root,
        &config,
        "rfc",
        "Event Sourcing",
        "jkaloger",
    )
    .unwrap();

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("title: \"Event Sourcing\""));
    assert!(content.contains("type: rfc"));
    assert!(content.contains("author: \"jkaloger\""));
}

#[test]
fn create_auto_increments_number() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(root.join(".lazyspec/templates/rfc.md"), "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: {date}\ntags: []\n---\n").unwrap();

    // Create an existing file to test auto-increment
    fs::write(root.join("docs/rfcs/RFC-001-old.md"), "").unwrap();

    let config = Config::default();
    let path = lazyspec::cli::create::run(root, &config, "rfc", "New Feature", "a").unwrap();

    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("RFC-002"), "got: {}", filename);
}

#[test]
fn create_with_date_pattern() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(root.join(".lazyspec/templates/rfc.md"), "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: {date}\ntags: []\n---\n").unwrap();

    let mut config = Config::default();
    config.naming.pattern = "{date}-{title}.md".to_string();

    let path = lazyspec::cli::create::run(root, &config, "rfc", "My Feature", "a").unwrap();

    let filename = path.file_name().unwrap().to_str().unwrap();
    // Should contain today's date and slugified title
    assert!(filename.ends_with("-my-feature.md"), "got: {}", filename);
}
```

**Step 2: Run to verify failure**

Run: `cargo test --test cli_create_test`
Expected: FAIL.

**Step 3: Implement template engine and create command**

`src/engine/template.rs`:
```rust
use anyhow::{anyhow, Result};
use chrono::Local;
use std::fs;
use std::path::Path;

pub fn render_template(template_content: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template_content.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn next_number(dir: &Path, prefix: &str) -> u32 {
    let mut max = 0u32;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) {
                // Extract number after prefix and hyphen: e.g. "RFC-001-..." -> "001"
                if let Some(rest) = name.strip_prefix(prefix) {
                    let rest = rest.trim_start_matches('-');
                    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = num_str.parse::<u32>() {
                        max = max.max(n);
                    }
                }
            }
        }
    }
    max + 1
}

pub fn resolve_filename(pattern: &str, doc_type: &str, title: &str, dir: &Path) -> String {
    let slug = slugify(title);
    let date = Local::now().format("%Y-%m-%d").to_string();
    let type_upper = doc_type.to_uppercase();
    let n = next_number(dir, &type_upper);

    let mut filename = pattern.to_string();
    filename = filename.replace("{type}", &type_upper);
    filename = filename.replace("{title}", &slug);
    filename = filename.replace("{date}", &date);

    // Handle {n:03} style patterns
    if filename.contains("{n:03}") {
        filename = filename.replace("{n:03}", &format!("{:03}", n));
    } else if filename.contains("{n}") {
        filename = filename.replace("{n}", &n.to_string());
    }

    filename
}
```

`src/cli/create.rs`:
```rust
use crate::engine::config::Config;
use crate::engine::document::DocType;
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
        "spec" => &config.directories.specs,
        "plan" => &config.directories.plans,
        _ => return Err(anyhow!("unknown doc type: {}", doc_type)),
    };

    let target_dir = root.join(dir);
    fs::create_dir_all(&target_dir)?;

    let filename = template::resolve_filename(&config.naming.pattern, doc_type, title, &target_dir);
    let target_path = target_dir.join(&filename);

    let template_path = root.join(&config.templates.dir).join(format!("{}.md", doc_type.to_lowercase()));
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
    format!(
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
    )
}
```

Update `src/cli/mod.rs` to add the create subcommand and `src/engine/mod.rs` to export template.

**Step 4: Run tests**

Run: `cargo test --test cli_create_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli/create.rs src/engine/template.rs tests/cli_create_test.rs
git commit -m "feat: create command with templates and auto-increment naming"
```

---

### Task 7: CLI - list, show, query Commands

**Files:**
- Create: `src/cli/list.rs`
- Create: `src/cli/show.rs`
- Create: `src/cli/query.rs`
- Modify: `src/cli/mod.rs`
- Create: `tests/cli_query_test.rs`

**Step 1: Write failing tests**

`tests/cli_query_test.rs`:
```rust
use lazyspec::engine::config::Config;
use lazyspec::engine::store::{Filter, Store};
use lazyspec::engine::document::DocType;
use std::fs;
use tempfile::TempDir;

fn setup() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/adrs")).unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-001-auth.md"),
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: review\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security, auth]\n---\n\nAuth body.\n",
    ).unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-002-api.md"),
        "---\ntitle: \"API Versioning\"\ntype: rfc\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [api]\n---\n\nAPI body.\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    (dir, store)
}

#[test]
fn list_all_rfcs() {
    let (_dir, store) = setup();
    let filter = Filter {
        doc_type: Some(DocType::Rfc),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 2);
}

#[test]
fn filter_by_tag() {
    let (_dir, store) = setup();
    let filter = Filter {
        tag: Some("security".to_string()),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Auth Redesign");
}

#[test]
fn resolve_shorthand_id() {
    let (_dir, store) = setup();
    let doc = store.resolve_shorthand("RFC-001");
    assert!(doc.is_some());
    assert_eq!(doc.unwrap().title, "Auth Redesign");
}
```

**Step 2: Run to verify failure**

Run: `cargo test --test cli_query_test`
Expected: FAIL (resolve_shorthand doesn't exist yet).

**Step 3: Add resolve_shorthand to Store, implement CLI commands**

Add to `src/engine/store.rs`:
```rust
pub fn resolve_shorthand(&self, id: &str) -> Option<&DocMeta> {
    self.docs.values().find(|d| {
        d.path
            .file_name()
            .and_then(|f| f.to_str())
            .map(|f| f.starts_with(id))
            .unwrap_or(false)
    })
}
```

`src/cli/list.rs`:
```rust
use crate::engine::document::DocType;
use crate::engine::store::{Filter, Store};

pub fn run(store: &Store, doc_type: Option<&str>, status: Option<&str>, json: bool) {
    let filter = Filter {
        doc_type: doc_type.and_then(|t| match t {
            "rfc" => Some(DocType::Rfc),
            "adr" => Some(DocType::Adr),
            "spec" => Some(DocType::Spec),
            "plan" => Some(DocType::Plan),
            _ => None,
        }),
        status: status.and_then(|s| serde_yaml::from_str(s).ok()),
        ..Default::default()
    };

    let docs = store.list(&filter);

    if json {
        // JSON output for agents
        let items: Vec<_> = docs.iter().map(|d| {
            serde_json::json!({
                "path": d.path,
                "title": d.title,
                "type": format!("{}", d.doc_type),
                "status": format!("{}", d.status),
            })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        for doc in docs {
            println!("{:<40} {:<10} {}", doc.title, doc.status, doc.path.display());
        }
    }
}
```

`src/cli/show.rs`:
```rust
use crate::engine::store::Store;
use anyhow::Result;

pub fn run(store: &Store, id: &str) -> Result<()> {
    let doc = store
        .resolve_shorthand(id)
        .ok_or_else(|| anyhow::anyhow!("document not found: {}", id))?;

    println!("# {}", doc.title);
    println!("Type: {} | Status: {} | Author: {}", doc.doc_type, doc.status, doc.author);
    println!("Date: {} | Tags: {}", doc.date, doc.tags.join(", "));
    println!();

    let body = store.get_body(&doc.path)?;
    println!("{}", body);

    Ok(())
}
```

Add `serde_json` to `Cargo.toml` dependencies.

Wire up new subcommands in `src/cli/mod.rs`:
```rust
#[derive(Subcommand)]
pub enum Commands {
    Init,
    Create {
        #[arg()]
        doc_type: String,
        #[arg()]
        title: String,
        #[arg(long, default_value = "unknown")]
        author: String,
    },
    List {
        #[arg()]
        doc_type: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        #[arg()]
        id: String,
    },
    // ... more to come
}
```

**Step 4: Run tests**

Run: `cargo test --test cli_query_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli/ src/engine/store.rs tests/cli_query_test.rs Cargo.toml
git commit -m "feat: list, show commands with shorthand ID resolution"
```

---

### Task 8: CLI - update & delete Commands

**Files:**
- Create: `src/cli/update.rs`
- Create: `src/cli/delete.rs`
- Modify: `src/cli/mod.rs`
- Create: `tests/cli_mutate_test.rs`

**Step 1: Write failing tests**

`tests/cli_mutate_test.rs`:
```rust
use lazyspec::engine::config::Config;
use lazyspec::engine::document::DocMeta;
use std::fs;
use tempfile::TempDir;

fn write_doc(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("docs/rfcs")).unwrap();
    fs::write(
        dir.join("docs/rfcs/RFC-001-test.md"),
        "---\ntitle: \"Test\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n\nBody.\n",
    ).unwrap();
}

#[test]
fn update_status_in_frontmatter() {
    let dir = TempDir::new().unwrap();
    write_doc(dir.path());

    lazyspec::cli::update::run(dir.path(), "docs/rfcs/RFC-001-test.md", &[("status", "review")]).unwrap();

    let content = fs::read_to_string(dir.path().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(format!("{}", meta.status), "review");
}

#[test]
fn delete_removes_file() {
    let dir = TempDir::new().unwrap();
    write_doc(dir.path());

    let path = dir.path().join("docs/rfcs/RFC-001-test.md");
    assert!(path.exists());

    lazyspec::cli::delete::run(dir.path(), "docs/rfcs/RFC-001-test.md").unwrap();
    assert!(!path.exists());
}
```

**Step 2: Run to verify failure**

Run: `cargo test --test cli_mutate_test`
Expected: FAIL.

**Step 3: Implement update and delete**

`src/cli/update.rs` - reads the file, parses frontmatter, updates the specified fields, writes back:
```rust
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, doc_path: &str, updates: &[(&str, &str)]) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = split_frontmatter_raw(&content)?;

    let mut doc: serde_yaml::Value = serde_yaml::from_str(&yaml)?;

    for (key, value) in updates {
        doc[*key] = serde_yaml::Value::String(value.to_string());
    }

    let new_yaml = serde_yaml::to_string(&doc)?;
    let new_content = format!("---\n{}---\n{}", new_yaml, body);

    fs::write(&full_path, new_content)?;
    Ok(())
}

fn split_frontmatter_raw(content: &str) -> Result<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(anyhow::anyhow!("no frontmatter"));
    }
    let after = &trimmed[3..];
    let end = after.find("\n---").ok_or_else(|| anyhow::anyhow!("unterminated"))?;
    let yaml = after[..end].to_string();
    let body = after[end + 4..].to_string();
    Ok((yaml, body))
}
```

`src/cli/delete.rs`:
```rust
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    if !full_path.exists() {
        return Err(anyhow::anyhow!("file not found: {}", doc_path));
    }
    fs::remove_file(&full_path)?;
    Ok(())
}
```

**Step 4: Run tests**

Run: `cargo test --test cli_mutate_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli/update.rs src/cli/delete.rs tests/cli_mutate_test.rs
git commit -m "feat: update and delete commands"
```

---

### Task 9: CLI - link & unlink Commands

**Files:**
- Create: `src/cli/link.rs`
- Create: `tests/cli_link_test.rs`

**Step 1: Write failing tests**

`tests/cli_link_test.rs`:
```rust
use lazyspec::engine::document::DocMeta;
use std::fs;
use tempfile::TempDir;

fn setup_two_docs(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("docs/rfcs")).unwrap();
    fs::create_dir_all(dir.join("docs/adrs")).unwrap();
    fs::write(
        dir.join("docs/rfcs/RFC-001-auth.md"),
        "---\ntitle: \"Auth\"\ntype: rfc\nstatus: accepted\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();
    fs::write(
        dir.join("docs/adrs/ADR-001-adopt-auth.md"),
        "---\ntitle: \"Adopt Auth\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-02\ntags: []\n---\n",
    ).unwrap();
}

#[test]
fn link_adds_relationship_to_frontmatter() {
    let dir = TempDir::new().unwrap();
    setup_two_docs(dir.path());

    lazyspec::cli::link::link(
        dir.path(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    let content = fs::read_to_string(dir.path().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].target, "docs/rfcs/RFC-001-auth.md");
}

#[test]
fn unlink_removes_relationship() {
    let dir = TempDir::new().unwrap();
    setup_two_docs(dir.path());

    lazyspec::cli::link::link(
        dir.path(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    lazyspec::cli::link::unlink(
        dir.path(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    let content = fs::read_to_string(dir.path().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.related.is_empty());
}
```

**Step 2: Run to verify failure**

Run: `cargo test --test cli_link_test`
Expected: FAIL.

**Step 3: Implement link/unlink**

`src/cli/link.rs` - reads frontmatter, modifies the `related` array, writes back. Uses `serde_yaml::Value` manipulation to add/remove entries from the related list.

**Step 4: Run tests**

Run: `cargo test --test cli_link_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli/link.rs tests/cli_link_test.rs
git commit -m "feat: link and unlink commands"
```

---

### Task 10: CLI - validate Command

**Files:**
- Create: `src/cli/validate.rs`
- Create: `tests/cli_validate_test.rs`

**Step 1: Write failing tests**

`tests/cli_validate_test.rs`:
```rust
use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use std::fs;
use tempfile::TempDir;

#[test]
fn validate_catches_broken_link() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::write(
        root.join("docs/adrs/ADR-001.md"),
        "---\ntitle: \"Bad Link\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: docs/rfcs/DOES-NOT-EXIST.md\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(!errors.is_empty());
}

#[test]
fn validate_passes_clean_repo() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::write(
        root.join("docs/rfcs/RFC-001.md"),
        "---\ntitle: \"Good\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(errors.is_empty());
}
```

**Step 2: Run to verify failure**

Run: `cargo test --test cli_validate_test`
Expected: FAIL (validate method returns empty vec or doesn't exist).

**Step 3: Implement validate on Store**

Add `ValidationError` enum and `validate()` method to `src/engine/store.rs`. Checks: broken links (related targets that don't exist in the store), missing required frontmatter fields.

`src/cli/validate.rs`:
```rust
use crate::engine::store::Store;

pub fn run(store: &Store, json: bool) -> i32 {
    let errors = store.validate();
    if errors.is_empty() {
        if !json {
            println!("All documents valid.");
        }
        return 0;
    }

    if json {
        let items: Vec<_> = errors.iter().map(|e| format!("{}", e)).collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        for error in &errors {
            eprintln!("  {}", error);
        }
    }
    2 // exit code for validation errors
}
```

**Step 4: Run tests**

Run: `cargo test --test cli_validate_test`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli/validate.rs src/engine/store.rs tests/cli_validate_test.rs
git commit -m "feat: validate command checks broken links"
```

---

### Task 11: Wire Up All CLI Subcommands in main.rs

**Files:**
- Modify: `src/main.rs`
- Modify: `src/cli/mod.rs`

**Step 1: Update Cli enum with all commands**

Add all remaining subcommands to the `Commands` enum: `Update`, `Delete`, `Link`, `Unlink`, `Query`, `Validate`. Each with appropriate clap args.

**Step 2: Wire match arms in main.rs**

Each arm loads config, creates store (where needed), delegates to the module function.

**Step 3: Test the full CLI manually**

Run:
```bash
cargo run -- init
cargo run -- create rfc "Test Feature" --author jkaloger
cargo run -- list
cargo run -- show RFC-001
cargo run -- validate
```

Expected: all work end to end.

**Step 4: Commit**

```bash
git add src/main.rs src/cli/mod.rs
git commit -m "feat: wire all CLI subcommands"
```

---

### Task 12: TUI - Basic App Shell with Panels

**Files:**
- Create: `src/tui/app.rs`
- Create: `src/tui/ui.rs`
- Modify: `src/tui/mod.rs`
- Modify: `src/main.rs`

**Step 1: Implement minimal TUI that renders three panels**

`src/tui/app.rs`:
```rust
use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType};
use crate::engine::store::{Filter, Store};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Types,
    DocList,
}

pub struct App {
    pub store: Store,
    pub active_panel: Panel,
    pub selected_type: usize,
    pub selected_doc: usize,
    pub doc_types: Vec<DocType>,
    pub should_quit: bool,
    pub fullscreen_doc: bool,
    pub scroll_offset: u16,
}

impl App {
    pub fn new(store: Store) -> Self {
        App {
            store,
            active_panel: Panel::Types,
            selected_type: 0,
            selected_doc: 0,
            doc_types: vec![DocType::Rfc, DocType::Adr, DocType::Spec, DocType::Plan],
            should_quit: false,
            fullscreen_doc: false,
            scroll_offset: 0,
        }
    }

    pub fn current_type(&self) -> &DocType {
        &self.doc_types[self.selected_type]
    }

    pub fn docs_for_current_type(&self) -> Vec<&DocMeta> {
        self.store.list(&Filter {
            doc_type: Some(self.current_type().clone()),
            ..Default::default()
        })
    }

    pub fn selected_doc_meta(&self) -> Option<&DocMeta> {
        let docs = self.docs_for_current_type();
        docs.get(self.selected_doc).copied()
    }
}
```

`src/tui/ui.rs` - renders three panels using `ratatui::layout::Layout` with horizontal split (left 20%, right 80%) then vertical split on the right (top 40%, bottom 60%). Renders type list in left panel, doc list in top right, preview in bottom right. Highlights active panel border.

`src/tui/mod.rs` - event loop using crossterm. Handles:
- `h`/`l`: switch `active_panel`
- `j`/`k`: navigate within panel
- `Enter`: toggle `fullscreen_doc`
- `q`/`Esc`: quit (or exit fullscreen)
- `?`: toggle help overlay

**Step 2: Test manually**

Create some test docs with `cargo run -- create rfc "Test"`, then run `cargo run` (no subcommand) to launch TUI.

**Step 3: Commit**

```bash
git add src/tui/ src/main.rs
git commit -m "feat: TUI shell with three-panel dashboard layout"
```

---

### Task 13: TUI - Markdown Preview Rendering

**Files:**
- Modify: `src/tui/ui.rs`

**Step 1: Add tui-markdown rendering to preview panel**

When a doc is selected, call `store.get_body()` and render using `tui_markdown::from_str()` into the bottom-right panel. Cache the rendered body to avoid re-reading on every frame.

**Step 2: Test manually**

Select a doc with content, verify markdown renders with headings, bold, code blocks.

**Step 3: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: markdown preview rendering in TUI"
```

---

### Task 14: TUI - Full-Screen Document View

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/app.rs`

**Step 1: Implement full-screen mode**

When `app.fullscreen_doc == true`, render only the document body using the full terminal area. Support scrolling with `j`/`k` (adjusts `scroll_offset`). Show doc title and metadata in a header bar. `Esc` or `q` returns to dashboard.

**Step 2: Test manually**

Press Enter on a doc, verify full-screen view with scrolling.

**Step 3: Commit**

```bash
git add src/tui/
git commit -m "feat: full-screen document view with scrolling"
```

---

### Task 15: TUI - Fuzzy Search

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`

**Step 1: Add search mode to app state**

Add `search_mode: bool`, `search_query: String`, `search_results: Vec<PathBuf>` to App. When `/` is pressed, enter search mode. Render a text input at the top. On each keystroke, run `nucleo` fuzzy match against all doc titles and tags. Display results in the doc list panel. `Esc` exits search mode. `Enter` on a result selects that doc.

**Step 2: Test manually**

Press `/`, type a few characters, verify results filter in real time.

**Step 3: Commit**

```bash
git add src/tui/
git commit -m "feat: fuzzy search with nucleo"
```

---

### Task 16: TUI - Status Colors & Help Overlay

**Files:**
- Modify: `src/tui/ui.rs`

**Step 1: Add status color coding**

Map each Status variant to a ratatui `Color`:
- Draft: Yellow
- Review: Blue
- Accepted: Green
- Rejected: Red
- Superseded: DarkGray

Apply to status text in the doc list.

**Step 2: Add help overlay**

When `?` is pressed, render a centered popup listing all keybindings. Any key dismisses it.

**Step 3: Test manually**

Verify colors render correctly. Press `?`, verify help shows.

**Step 4: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: status colors and help overlay"
```

---

### Task 17: TUI - File Watching

**Files:**
- Modify: `src/tui/mod.rs`
- Modify: `src/engine/store.rs`

**Step 1: Add file watcher to TUI event loop**

Use `notify::recommended_watcher` to watch all configured doc directories. When a file change event fires, reload that file in the store. Use a channel to send reload events to the TUI event loop alongside crossterm events.

Add `reload_file(&mut self, path: &Path)` and `remove_file(&mut self, path: &Path)` methods to Store.

**Step 2: Test manually**

Launch TUI, edit a doc in another terminal, verify the TUI updates.

**Step 3: Commit**

```bash
git add src/tui/ src/engine/store.rs
git commit -m "feat: file watching for live TUI updates"
```

---

## Slice Summary

| # | Slice | Category |
|---|-------|----------|
| 1 | Project scaffold | Must-have |
| 2 | Document model & parsing | Must-have |
| 3 | Configuration | Must-have |
| 4 | Store (loading & querying) | Must-have |
| 5 | CLI: init | Must-have |
| 6 | CLI: create + templates | Must-have |
| 7 | CLI: list, show, query | Must-have |
| 8 | CLI: update, delete | Must-have |
| 9 | CLI: link, unlink | Must-have |
| 10 | CLI: validate | Must-have |
| 11 | Wire all CLI subcommands | Must-have |
| 12 | TUI: dashboard shell | Must-have |
| 13 | TUI: markdown preview | Must-have |
| 14 | TUI: full-screen view | Must-have |
| 15 | TUI: fuzzy search | Nice-to-have |
| 16 | TUI: colors & help | Nice-to-have |
| 17 | TUI: file watching | Nice-to-have |
