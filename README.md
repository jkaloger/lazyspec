<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A TUI+CLI for keeping track of project specs.
</p>

<p align="center">
  <a href="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml"><img src="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white" alt="Rust 2021">
  <img src="https://img.shields.io/badge/status-experimental-blueviolet" alt="Status: Experimental">
  <img src="https://img.shields.io/github/v/tag/jkaloger/lazyspec?label=version&color=blue" alt="Version">
  <a href="https://github.com/jkaloger/lazyspec/commits/main"><img src="https://img.shields.io/github/last-commit/jkaloger/lazyspec?logo=git&logoColor=white" alt="Last commit"></a>
  <a href="https://github.com/jkaloger/lazyspec/blob/main/flake.nix"><img src="https://img.shields.io/badge/nix-flake-5277C3?logo=nixos&logoColor=white" alt="Nix Flake"></a>
</p>

<!-- TODO: replace with VHS gif of TUI + workflow -->
<img width="1864" height="1147" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/91f308d1-8d03-4815-b2ec-fa445159c563" />

> [!WARNING]
> Lazyspec is experimental. APIs and CLI interfaces will change frequently and without notice.

## Features

- **Pluggable storage backends.** Filesystem, git refs, or GitHub Issues per document type. Same commands, same validation, mix-and-match.
- **Distributed numbering via git refs.** Lease-based reservation prevents collisions across branches and agents.
- **Typed links + cross-backend validation.** `implements`, `blocks`, `supersedes`, `related-to`. `lazyspec validate` exits non-zero in CI.
- **`@ref` source expansion.** Embed `@ref src/foo.rs#Bar` in specs; `show -e` inlines code from git history. Symbol extraction for Rust and TypeScript. Pin hashes to lock to commits.
- **DAG-driven work sequencing.** Derives a work graph from `blocks` relationships. `lazyspec next` surfaces unblocked work; the TUI has a planning view.
- **TUI dashboard.** Fuzzy search, markdown preview, live file watching, git status gutters. No config to start.
- **Customisable!** Config based setup allows you to customise types, relationships, validation, backends, and more.

Every command supports `--json`. Document content and metadata are retrievable programmatically, so agents and humans share one interface.

## Install

### Nix

```sh
nix profile install github:jkaloger/lazyspec
```

Or run without installing:

```sh
nix run github:jkaloger/lazyspec
```

### Cargo

```sh
cargo install --git https://github.com/jkaloger/lazyspec
```

### From Source

```sh
git clone https://github.com/jkaloger/lazyspec
cd lazyspec
cargo install --path .
```

### Shell Completions

Generate and source a completion script for your shell:

```sh
# zsh
source <(lazyspec completions zsh)

# bash
source <(lazyspec completions bash)

# fish
lazyspec completions fish | source
```

Add the appropriate line to your shell profile (`~/.zshrc`, `~/.bashrc`, etc.) to load completions on startup. Completions include subcommands, flags, document IDs, and relationship types.

## Skills

Lazyspec ships agent skills that drive its workflow (RFC -> Story -> Iteration -> build -> review). See [`skills/README.md`](skills/README.md).

## Usage

### Quick Start

```sh
lazyspec init
lazyspec create rfc "auth redesign"
lazyspec create story "login form"
lazyspec link STORY-001 implements RFC-001
lazyspec validate
lazyspec                      # launch TUI
```

> [!TIP]
> Check the `examples/` directory for a complete project setup including config, templates, and agent skill definitions you can use as a starting point.
> This repo dogfoods lazyspec, so you can also check out the `docs/` directory or run `lazyspec` from this repo.

### TUI

Running `lazyspec` with no subcommand opens the interactive dashboard. Fuzzy search, markdown preview, document creation, and live file watching -- documents update automatically when changed on disk.

<img width="1864" height="1147" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/91f308d1-8d03-4815-b2ec-fa445159c563" />

<details>
<summary><h3>CLI</h3></summary>

All document management is available as subcommands. Most accept `--json` for machine-readable output. See `lazyspec help <command>` for full flag reference.

| Command                              | Description                                                           |
| ------------------------------------ | --------------------------------------------------------------------- |
| `init`                               | Initialise lazyspec in the current project                            |
| `create <type> <title> [--author X]` | Create a document (rfc, adr, story, iteration)                        |
| `list [type] [--status X]`           | List documents with optional filters                                  |
| `show <id> [-e]`                     | Display a document by path or shorthand ID (e.g. `RFC-001`)           |
| `update <path> --status X --title X` | Update document frontmatter                                           |
| `delete <path>`                      | Delete a document                                                     |
| `link <from> <rel> <to>`             | Add a typed relationship (implements, supersedes, blocks, related-to) |
| `unlink <from> <rel> <to>`           | Remove a relationship between documents                               |
| `search <query> [--doc-type X]`      | Full-text search across all documents                                 |
| `context <id>`                       | Show the full document chain (RFC -> Story -> Iteration)              |
| `status`                             | Show full project status with all documents and validation            |
| `ignore <path>`                      | Mark a document to skip validation                                    |
| `unignore <path>`                    | Remove validation skip from a document                                |
| `validate [--warnings]`              | Check document integrity and link consistency                         |
| `fix [paths] [--dry-run]`            | Fix documents with broken or incomplete frontmatter                   |
| `next`                               | Show the next ready work items based on the dependency graph          |
| `graph`                              | Render the document dependency graph as d2, dot, or JSON              |
| `critical-path`                      | Show the longest weighted path through the dependency graph           |
| `pin <id>`                           | Pin blob hashes onto `@ref` directives in a document                  |
| `provenance add <id> <citation>`     | Append a citation to a document's provenance list                     |
| `provenance remove <id> <citation>`  | Remove an exact-match citation from a document's provenance list      |
| `provenance list [id]`               | List citations for a document, or for all documents grouped by id     |
| `reservations list`                  | Show all reservation refs on the remote                               |
| `reservations prune [--dry-run]`     | Remove refs for documents that already exist locally                  |

#### `show` Flags

| Flag                        | Description                                      |
| --------------------------- | ------------------------------------------------ |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`         | Max lines per expanded ref (default: 25)         |

#### `next` Flags

| Flag                | Description                                                                                              |
| ------------------- | -------------------------------------------------------------------------------------------------------- |
| `--scope <SCOPE>`   | Restrict the ready set to a scope anchor (RFC or Story id). Mutually exclusive with `--after`            |
| `--after <AFTER>`   | Restrict the ready set to documents downstream of an anchor (transitive blocks). Mutually exclusive with `--scope` |
| `--type <type>`     | Filter ready[] by document type (e.g. story, iteration, rfc)                                             |
| `--include-leased`  | Include candidates that are currently leased (default: hide them)                                        |
| `--json`            | Output as JSON                                                                                           |

#### `graph` Flags

| Flag                | Description                                                                                                          |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `--scope <SCOPE>`   | Restrict the graph to the implements-subtree of an anchor (RFC or Story id). Mutually exclusive with `--after`       |
| `--after <AFTER>`   | Restrict the graph to documents downstream of an anchor (transitive blocks). Mutually exclusive with `--scope`       |
| `--format <FORMAT>` | Output format. One of `d2`, `json`, `dot` (default: `d2`)                                                            |

#### `critical-path` Flags

| Flag              | Description                                                                                                              |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `--scope <SCOPE>` | Restrict the path search to the implements-subtree of an anchor (RFC or Story id). Mutually exclusive with `--after`     |
| `--after <AFTER>` | Restrict the path search to documents downstream of an anchor (transitive blocks). Mutually exclusive with `--scope`     |
| `--json`          | Output as JSON                                                                                                           |

#### Provenance

Cite the sources of truth that informed a document. Citations are free-form strings stored as a YAML list in frontmatter.

```sh
lazyspec provenance add RFC-001 "Workshop 2026-04-12"
lazyspec provenance list RFC-001
# Workshop 2026-04-12
```

</details>

<details>
<summary><h3><code>@ref</code> Syntax</h3></summary>

Documents can embed references to source code using `@ref` directives. By default, `lazyspec show` renders them as-is. Pass `-e` to expand them inline.

```
@ref <path>                    # entire file
@ref <path>#<symbol>           # specific type or struct
@ref <path>#<symbol>@<sha>     # symbol at a specific git commit
@ref <path>#123                # line 123
@ref <path>#123@<sha>          # line 123 at a specific git commit
```

Expansion resolves content via `git show` (committed state, not working tree). Supported languages for symbol extraction are TypeScript (`.ts`/`.tsx`) and Rust (`.rs`).

Each expanded ref includes a caption line showing the file path, short git SHA, and symbol or line info. Expanded blocks are truncated to 25 lines by default; when truncated, a trailing comment shows how many lines were omitted. Use `--max-ref-lines` to adjust the limit.

**Example**

A document containing:

```
@ref src/engine/store.rs#Store
```

Renders as:

````
```rust
pub struct Store { ... }
```
````

Unresolvable refs render as:

```
> [unresolved: src/engine/store.rs#Store]
```

</details>

<details>
<summary><h2>Configuration</h2></summary>

`lazyspec init` creates a `.lazyspec.toml` in your project root with four built-in document types:

```toml
[directories]
rfcs = "docs/rfcs"
adrs = "docs/adrs"
stories = "docs/stories"
iterations = "docs/iterations"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
```

### Custom Types

Instead of `[directories]`, you can define types explicitly with `[[types]]`. This lets you rename the defaults, add new types, or set custom prefixes and icons used in the TUI.

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[types]]
name = "spec"
plural = "specs"
dir = "docs/specs"
prefix = "SPEC"
icon = "◆"
```

### Storage Backends

Each type can be stored on a different backend via `store`. The same commands and validation work across all of them.

| Backend         | When to use                                                             |
| --------------- | ----------------------------------------------------------------------- |
| `filesystem`    | Default. Markdown files committed to the repo.                          |
| `git-ref`       | Documents stored as git refs. Useful for transient or high-churn types. |
| `github-issues` | Documents stored as GitHub Issues. Surfaces state in the GitHub UI.     |

```toml
[[types]]
name = "rfc"
store = "filesystem"

[[types]]
name = "iteration"
store = "github-issues"

[github]
owner = "jkaloger"
repo = "lazyspec"
```

### Validation Rules

Validation rules define structural constraints between document types. Two shapes are supported:

- `parent-child` -- the child type must link to a parent type via a given relationship.
- `relation-existence` -- documents of a given type must have at least one relationship.

```toml
[[rules]]
shape = "parent-child"
name = "stories-need-rfcs"
child = "story"
parent = "rfc"
link = "implements"
severity = "warning"

[[rules]]
shape = "relation-existence"
name = "adrs-need-relations"
type = "adr"
require = "any-relation"
severity = "error"
```

In addition to user-defined rules, `validate` ships built-in diagnostics derived from the work graph:

- **Cycle in `blocks` graph** (error) -- fires when `blocks` relationships form a directed cycle. The diagnostic message names the cycle members.
- **RFC accepted but all implementing stories complete** (warning) -- fires when an RFC is `accepted` and every implementing story is in a terminal state. Suggests promoting the RFC to `complete`.
- **Rejected upstream blocker** (warning) -- fires when a document declares `blocks: <X>` and `X` is `rejected`. Indicates the downstream may be stale.

### Priorities

Documents may carry a `priority` frontmatter key. The vocabulary defaults to MoSCoW (`must=4`, `should=3`, `could=2`, `wont=1`) and feeds into work-graph weighting. Define `[priorities.<key>]` blocks to replace the defaults with your own scheme; defining any block disables the MoSCoW fallback.

```toml
[priorities.must]
weight = 4
[priorities.should]
weight = 3
```

Each `[[types]]` accepts two related fields:

- `requires_priority` -- when `true`, validation rejects documents of that type with no `priority`. Defaults to `true` for `story` and `iteration`, `false` otherwise.
- `terminal_statuses` -- the status values that mark a document as done for sequencing purposes. Falls back to per-type defaults (RFC-041): `complete`/`superseded`/`rejected` for RFCs and stories, `complete` for iterations and audits, `accepted`/`superseded` for ADRs and conventions. An override replaces the defaults rather than merging.

```toml
[[types]]
name = "rfc"
requires_priority = false
terminal_statuses = ["complete", "superseded", "rejected"]
```

### Numbering

Document numbers are assigned automatically during `create`. Three strategies are available per type:

| Strategy      | Behaviour                                                                                 |
| ------------- | ----------------------------------------------------------------------------------------- |
| `incremental` | Next sequential integer from existing files (default)                                     |
| `sqids`       | Short hash-like IDs derived from a timestamp, configured via `[numbering.sqids]`          |
| `reserved`    | Reserves numbers on a git remote before creating files, preventing distributed collisions |

Reserved numbering uses git custom refs (`refs/reservations/*`) to coordinate across branches. It wraps either incremental or sqids formatting with an atomic push-based lock, so two people never get the same number.

```toml
[[types]]
name = "rfc"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
remote = "origin"        # default
format = "incremental"   # or "sqids"
max_retries = 5          # push retry attempts before failing
```

If the remote is unreachable, `create` fails rather than silently falling back. Use `lazyspec reservations prune` to clean up refs for documents that have been created.

### Templates

Place markdown templates in the templates directory (`.lazyspec/templates/` by default). When creating a document, lazyspec uses the template matching the document type name (e.g. `rfc.md`, `story.md`).

</details>

## Development

### Nix (recommended)

The repo includes a Nix flake that provides the full toolchain. With [direnv](https://direnv.net/) installed:

```sh
direnv allow
```

Or enter the dev shell manually:

```sh
nix develop
```

This gives you cargo, clippy, rustfmt, and rust-analyzer at pinned versions.

To run all checks (clippy, tests, formatting):

```sh
nix flake check
```

### Without Nix

```sh
cargo build
cargo test
```
