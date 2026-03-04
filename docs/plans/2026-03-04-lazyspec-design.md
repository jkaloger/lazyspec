# lazyspec Design

A TUI and CLI for managing project specifications, RFCs, ADRs, and plans.

## Stack

- **Language**: Rust
- **TUI**: ratatui + crossterm
- **CLI**: clap (derive)
- **Frontmatter**: serde + serde_yaml
- **Markdown parsing**: pulldown-cmark
- **Markdown rendering (TUI)**: tui-markdown
- **Fuzzy search**: nucleo
- **File watching**: notify

## Architecture

Single binary, unified architecture. No arguments launches the TUI. Subcommands run CLI operations.

```
lazyspec (binary)
├── core/       # Document model, store, queries, templates, linking
├── cli/        # Clap commands that call into core
└── tui/        # Ratatui app that calls into core
```

## Document Model

Each document is a markdown file with YAML frontmatter, stored in type-specific directories.

### Frontmatter Schema

```yaml
---
title: "Adopt Event Sourcing for Audit Log"
type: adr          # rfc | adr | spec | plan
status: draft      # draft | review | accepted | rejected | superseded
author: jkaloger
date: 2026-03-04
tags: [architecture, events]
related:
  - implements: rfcs/RFC-001-event-sourcing.md
  - supersedes: adrs/ADR-002-simple-logging.md
---
```

### Directory Structure

```
docs/
├── rfcs/
├── adrs/
├── specs/
└── plans/
```

Directories are configurable via `.lazyspec.toml`.

### Relationships

The `related` field supports typed relationships: `implements`, `supersedes`, `blocks`, `related-to`. These are bidirectional in the query layer.

## CLI Interface

```
lazyspec init                          # Create .lazyspec.toml and doc directories
lazyspec create <type> <title>         # Create from template, prints path
lazyspec list [type] [--status X]      # List docs, filterable
lazyspec show <path-or-id>             # Print doc to stdout
lazyspec update <path> --status X      # Update frontmatter fields
lazyspec delete <path>                 # Remove doc
lazyspec link <from> <rel> <to>        # Add a relationship
lazyspec unlink <from> <rel> <to>      # Remove a relationship
lazyspec query --tag X --status Y      # Search across all docs
lazyspec validate                      # Check frontmatter and links
```

- `--json` flag for machine-readable output (agent use).
- Exit codes: 0 success, 1 not found, 2 validation error.
- Shorthand IDs: `RFC-001` resolves by prefix match against filenames.

## TUI Interface

Dashboard layout with three zones:

```
┌──────────────────────────────────────────────────────┐
│  lazyspec                              [/] search    │
├────────────┬─────────────────────────────────────────┤
│            │                                         │
│  RFCs  (3) │  RFC-001  Event Sourcing     accepted   │
│  ADRs  (5) │  RFC-002  API Versioning     review     │
│  Specs (2) │  RFC-003  Auth Redesign      draft      │
│  Plans (4) │                                         │
│            │                                         │
│            ├─────────────────────────────────────────┤
│            │  ## Context                             │
│            │  We need a reliable audit trail for...  │
│            │                                         │
│            │  ## Decision                            │
│            │  Adopt event sourcing for the audit...  │
│            │                                         │
│            │  Related: implements RFC-001             │
│            │  Tags: architecture, events             │
└────────────┴─────────────────────────────────────────┘
```

- **Left panel**: Document type selector with counts.
- **Top right**: Doc list for selected type. Title, status (color-coded).
- **Bottom right**: Rendered markdown preview of selected doc.

### Navigation

- `h`/`l`: Switch focus between left panel (types) and right panel (doc list)
- `j`/`k`: Navigate within focused panel
- `Enter`: Full-screen doc view with scrolling
- `Esc`/`q`: Back / quit
- `/`: Fuzzy search across all docs (title, tags)
- `?`: Help overlay
- `g`/`G`: Jump to top/bottom

### Status Colors

- draft: yellow
- review: blue
- accepted: green
- rejected: red
- superseded: grey

## Configuration

`.lazyspec.toml` at project root:

```toml
[directories]
rfcs = "docs/rfcs"
adrs = "docs/adrs"
specs = "docs/specs"
plans = "docs/plans"

[templates]
dir = ".lazyspec/templates"

[naming]
# Available tokens: {type}, {n:03} (auto-increment), {date} (YYYY-MM-DD), {title} (slugified)
pattern = "{type}-{n:03}-{title}.md"
# or: pattern = "{date}-{title}.md"
# or: pattern = "{type}-{date}-{title}.md"
```

`lazyspec init` creates this file with sensible defaults.

## Core: Store

The store is the central data layer shared by CLI and TUI.

### In-Memory Model

```rust
struct Store {
    docs: HashMap<PathBuf, DocMeta>,
    graph: LinkGraph,
    config: Config,
}

struct DocMeta {
    path: PathBuf,
    title: String,
    doc_type: DocType,
    status: Status,
    author: String,
    date: NaiveDate,
    tags: Vec<String>,
    related: Vec<Relation>,
}

struct LinkGraph {
    edges: Vec<(PathBuf, RelationType, PathBuf)>,
    forward: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    reverse: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
}
```

### Loading Strategy

On startup, the store walks configured directories and reads only frontmatter (stops at the second `---`). Body content is loaded lazily when a doc is selected for preview. This keeps startup fast.

### File Watching (TUI only)

Uses `notify` to watch doc directories. On change events, re-parses the affected file and updates the index. CLI mode skips file watching.

### Mutations

Writes to disk first, then updates in-memory state. Disk is the source of truth.

### Query Interface

```rust
impl Store {
    fn list(&self, filter: &Filter) -> Vec<&DocMeta>;
    fn get(&self, path: &Path) -> Option<&DocMeta>;
    fn get_body(&self, path: &Path) -> io::Result<String>;
    fn related_to(&self, path: &Path) -> Vec<(RelationType, &DocMeta)>;
    fn search(&self, query: &str) -> Vec<&DocMeta>;
    fn validate(&self) -> Vec<ValidationError>;
}
```

Fuzzy search covers frontmatter only (title, tags, author). Body content is not indexed.

## Key Crates

| Crate | Purpose |
|-------|---------|
| `ratatui` + `crossterm` | TUI framework + terminal backend |
| `clap` (derive) | CLI argument parsing |
| `serde` + `serde_yaml` | Frontmatter serialization |
| `pulldown-cmark` | Markdown parsing |
| `tui-markdown` | Markdown rendering in TUI |
| `nucleo` | Fuzzy matching |
| `notify` | Filesystem watching |
| `chrono` | Date handling |
| `toml` | Config file parsing |
