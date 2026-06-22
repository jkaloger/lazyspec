<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A little TUI & CLI for project documentation.
</p>

<p align="center">
  <a href="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml"><img src="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white" alt="Rust 2021">
  <img src="https://img.shields.io/badge/status-experimental-blueviolet" alt="Status: Experimental">
  <img src="https://img.shields.io/github/v/tag/jkaloger/lazyspec?label=version&color=blue" alt="Version">
  <a href="https://github.com/jkaloger/lazyspec/commits/main"><img src="https://img.shields.io/github/last-commit/jkaloger/lazyspec?logo=git&logoColor=white" alt="Last commit"></a>
  <a href="https://github.com/jkaloger/lazyspec/blob/main/flake.nix"><img src="https://img.shields.io/badge/nix-flake-5277C3?logo=nixos&logoColor=white" alt="Nix Flake"></a>
</p>

<img width="1864" height="1147" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/91f308d1-8d03-4815-b2ec-fa445159c563" />

> [!WARNING]
> Lazyspec is experimental. APIs and CLI interfaces will change frequently and without notice.

## Features

Lazyspec manages project documentation as version-controlled markdown files with YAML frontmatter. Documents live in your repo, so agents and humans read from the same source of truth.

- Create, update, link, and validate documents. Config-driven relationships (the starter set is `implements`, `supersedes`, `blocks`, `related-to`) keep the chain explicit.
- Catch broken links, orphaned documents, and incomplete frontmatter before they rot. `lazyspec validate` exits non-zero on errors, so it slots into CI.
- Embed `@ref` directives in your specs to point at source code. Lazyspec expands them inline using `git show`, with symbol-level extraction for Rust and TypeScript.
- Fuzzy search, markdown preview, live file watching, and document creation without leaving the terminal.
- Every command supports `--json` output for automation and agent integration.
- Define your own types, templates, and directory layout in `.lazyspec.toml`.

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

Lazyspec includes a set of agent skills that enforce its workflow:

| Skill              | Purpose                                                                |
| ------------------ | ---------------------------------------------------------------------- |
| `plan-work`        | Detect existing artifacts and determine the right entry point          |
| `write-rfc`        | Propose a design with intent, interface sketches, and identify stories |
| `create-story`     | Create stories with acceptance criteria linked to an RFC               |
| `resolve-context`  | Gather full document chain (RFC -> Story -> Iteration) before work     |
| `create-iteration` | Plan an iteration with task breakdown and test plan                    |
| `build`            | Implement tasks from an iteration with subagent dispatch               |
| `review-iteration` | Two-stage review -- AC compliance first, then code quality             |
| `create-audit`     | Criteria-based review (health check, security, accessibility, etc.)    |

## Usage

### Quick Start

Initialise a new project, then launch the TUI:

```sh
lazyspec init
lazyspec
```

> [!TIP]
> Check the `examples/` directory for a complete project setup including config, templates, and agent skill definitions you can use as a starting point.
> This repo dogfoods lazyspec, so you can also check out the `docs/` directory or run `lazyspec` from this repo.

### TUI

Running `lazyspec` with no subcommand opens the interactive dashboard. It provides fuzzy search, markdown preview, document creation, and live file watching -- documents update automatically when changed on disk. An external edit of `.lazyspec.toml` (e.g. a `git pull`) reloads the running session automatically; press `R` to reload it manually. Press `?` for the full keybindings overlay.

| Key       | Action                                  |
| --------- | --------------------------------------- |
| `j` / `k` | Navigate up/down                        |
| `h` / `l` | Switch document type                    |
| `Enter`   | Open document fullscreen                |
| `/`       | Fuzzy search                            |
| `n`       | Create new document                     |
| `e`       | Edit document in `$EDITOR`              |
| `d`       | Delete document                         |
| `r`       | Add relation                            |
| `R`       | Reload config from `.lazyspec.toml`     |
| `w`       | Warnings / validation panel             |
| `5`       | Open the Settings view                   |
| `` ` ``   | Cycle view (documents / filters / graph / settings) |
| `q`       | Quit                                    |
| `?`       | Toggle keybindings help                 |

#### Settings View

Press `5` (or cycle to it with `` ` ``) to open the Settings view, which edits `.lazyspec.toml` in place. Categories are listed on the left; the right panel shows the fields (or entries) of the selected category. Saving rewrites `.lazyspec.toml`, preserving its comments and formatting, after validating the whole config; an invalid config is reported and not written.

| Key                | Action                                                        |
| ------------------ | ------------------------------------------------------------- |
| `h` / `l`          | Switch category (also `Left` / `Right`)                       |
| `j` / `k`          | Move between fields / entries (also `Down` / `Up`)            |
| `Enter`            | Drill into a collection entry, or start editing a field       |
| `n`                | Add a new entry to a collection (Document Types / Relationships / Validation Rules seed a default and drill in; Certification prompts for a spec-path key) |
| `d`                | Delete the selected collection entry, behind a confirm (refuses the last relationship) |
| `Space`            | Toggle a boolean / cycle an enum field                        |
| `g`                | When a dependency section is auto-scaffolded (e.g. cycling numbering to `sqids`), jump to the required field it needs filled |
| type + `Enter`     | Confirm a text / number / duration / list edit                |
| `Esc`              | Cancel an in-progress edit, or undrill from an entry          |
| `w` / `Ctrl-S`     | Save changes to `.lazyspec.toml` (validates the whole config) |
| `q` / `Esc`        | Quit; with unsaved changes, prompts `(s)ave / (d)iscard / (Esc) cancel` |

<details>
<summary><h3>CLI</h3></summary>

All document management is available as subcommands. Most accept `--json` for machine-readable output.

| Command                              | Description                                                           |
| ------------------------------------ | --------------------------------------------------------------------- |
| `init`                               | Initialise lazyspec in the current project                            |
| `create <type> <title> [--author X]` | Create a document (rfc, adr, story, iteration)                        |
| `list [type] [--status X]`           | List documents with optional filters                                  |
| `show <id> [-e]`                     | Display a document by path or shorthand ID (e.g. `RFC-001`)           |
| `update <path> --status X --title X` | Update document frontmatter                                           |
| `delete <path>`                      | Delete a document                                                     |
| `link <from> <rel> <to>`             | Add a typed relationship (canonical or inverse keyword)               |
| `unlink <from> <rel> <to>`           | Remove a relationship (canonical or inverse keyword)                  |
| `search <query> [--doc-type X]`      | Full-text search across all documents                                 |
| `context <id> [--depth N]`           | Show the full document chain (RFC -> Story -> Iteration)              |
| `status`                             | Show full project status with all documents and validation            |
| `ignore <path>`                      | Mark a document to skip validation                                    |
| `unignore <path>`                    | Remove validation skip from a document                                |
| `validate [--warnings]`              | Check document integrity and link consistency                         |
| `fix [paths] [--dry-run]`            | Fix documents with broken or incomplete frontmatter                   |
| `fix --config [--dry-run]`           | Repair `.lazyspec.toml` (inject missing standard relationships/rules) |
| `pin <id>`                           | Pin blob hashes onto `@ref` directives in a document                  |
| `provenance add <id> <citation>`     | Append a citation to a document's provenance list                     |
| `provenance remove <id> <citation>`  | Remove an exact-match citation from a document's provenance list      |
| `provenance list [id]`               | List citations for a document, or for all documents grouped by id     |
| `reservations list`                  | Show all reservation refs on the remote                               |
| `reservations prune [--dry-run]`     | Remove refs for documents that already exist locally                  |

#### Relationship Keywords

`link` and `unlink` resolve relationship names against the `[[relationships]]` block in your `.lazyspec.toml` (see [Configuration](#configuration)). The starter config declares the canonical set (`implements`, `supersedes`, `blocks`, `related-to`) and, for each directional relationship, an inverse keyword (`implemented-by`, `superseded-by`, `blocked-by`) -- but the vocabulary is yours to change. An inverse keyword is a write-time alias: it flips the direction and stores the canonical relation on the target document. Nothing new is persisted; the reverse direction is still computed by the link graph.

```sh
lazyspec link STORY-9 blocked-by RFC-2
# writes `blocks: STORY-9` onto RFC-2, prints:
# Linked docs/rfcs/RFC-002-....md --blocks--> STORY-9
```

A relationship declared without an `inverse` is symmetric (like `related-to`) and has no separate inverse keyword. A keyword that matches no declared `name` or `inverse` is rejected before anything is written, and `validate` flags any document carrying a relationship name absent from `[[relationships]]`.

#### `show` Flags

| Flag                        | Description                                      |
| --------------------------- | ------------------------------------------------ |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`         | Max lines per expanded ref (default: 25)         |

#### `context` Flags

| Flag        | Description                                                          |
| ----------- | ------------------------------------------------------------------- |
| `--depth N` | Max hops to follow `related-to` links when collecting related records (default: 1) |

#### `provenance` Subcommands

Cite the sources of truth that informed a document. Citations are free-form strings stored as a YAML list in frontmatter.

```sh
lazyspec provenance add RFC-001 "Workshop 2026-04-12"
lazyspec provenance add RFC-001 "Privacy Act 1988"
lazyspec provenance list RFC-001
# Workshop 2026-04-12
# Privacy Act 1988

lazyspec provenance remove RFC-001 "Privacy Act 1988"
lazyspec provenance list
# RFC-001	Workshop 2026-04-12
```

All three subcommands accept `--json`. Shapes:

- `add` / `remove`: `{ "doc": "...", "added"|"removed": "...", "provenance": [...] }`
- `list <id>`: `{ "doc": "...", "provenance": [...] }`
- `list` (no id): `{ "documents": [{ "id": "...", "path": "...", "provenance": [...] }, ...] }`

`add` rejects empty citations. `remove` is exact-match and errors when the citation is absent.

## Coordination

### Claude Code Hooks

Lazyspec ships hook snippets that claim, heartbeat, and release a lease on `$ASSIGNED_TASK` across a Claude Code session. The orchestrator (daemon, manual `export`, etc.) sets the env var; hooks no-op silently when it is unset, so the snippet is safe to install unconditionally.

Drop into `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec claim \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec heartbeat \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --min-interval 15m --json || true"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec release \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ]
  }
}
```

The standalone file lives at [`hooks/claude-code-settings.json`](hooks/claude-code-settings.json).

**`$ASSIGNED_TASK` contract.** Orchestrator sets it to a doc id (e.g. `ITERATION-170`). If unset, the `[ -n "$ASSIGNED_TASK" ]` guard short-circuits and no `lazyspec` invocation happens.

**Throttle.** `--min-interval 15m` matches the default `lease_duration / 4` (lease defaults to 60m). If you tune `lease_duration` in `.lazyspec.toml`, tune this to roughly a quarter of it.

**Error tolerance.** `|| true` swallows non-zero exits from `lazyspec` (e.g. lease already released, network blip), so a session never fails to end because of a coordination error.

See [RFC-035](docs/rfcs/RFC-035-git-ref-document-storage-with-lease-based-claiming.md) for the design rationale.

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

`lazyspec init` creates a `.lazyspec.toml` in your project root with a starter set
of document types, relationships, and validation rules. The engine carries no
built-in document types or relationship vocabulary: the `[[types]]`,
`[[relationships]]`, and `[[rules]]` declared in `.lazyspec.toml` are the sole
source of truth. A missing `.lazyspec.toml`, or a config with no `[[types]]`, is a
hard error that points you at `lazyspec init`; a config missing the
`[[relationships]]` block is a hard error that points you at `lazyspec fix --config`.

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
```

### Migrating an Existing Config

Projects created before relationships and rules became config-driven have a
`.lazyspec.toml` with no `[[relationships]]` or `[[rules]]` blocks. Strict load
now rejects such a config on every command, pointing you at the migration:

```sh
lazyspec fix --config            # inject missing standard relationships/rules and default lifecycles
lazyspec fix --config --dry-run  # preview the additions without writing
```

`fix --config` reads the config leniently (the one place strict load is
bypassed), then appends only the standard `[[relationships]]` / `[[rules]]` that
are missing -- comparing by name, so user-added relationships and rules are kept
and nothing is duplicated. It also injects the default `lifecycle` into any
`[[types]]` entry that lacks one (a type that already declares a lifecycle is
left untouched); migrated types are reported under `lifecycles_added`. Every
existing section (`[github]`, `[coordination]`, comments, ordering) is preserved,
and it is idempotent -- running it on an up-to-date config makes no change. The
flag is config-only: no documents are touched (use plain `lazyspec fix` for
frontmatter).

### Custom Types

Each document type is declared with a `[[types]]` block. This lets you rename the
defaults, add new types, or set custom prefixes and icons used in the TUI.
Directories derive entirely from each type's own `dir`; there is no separate
`[directories]` table.

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

### Lifecycle

Each type declares a `lifecycle`: the set of valid statuses (`states`) and the
permitted status transitions (`edges`). `update --status` is gated by this
lifecycle -- a move is allowed only when an edge from the current status to the
target is declared. An edge with a `*` source matches any current status (e.g.
`* -> superseded` lets any document be superseded). Setting a status to its
current value is always a no-op (idempotent, never rejected). When a move has no
matching edge, `update` exits non-zero and the frontmatter is left unchanged.

```toml
[[types]]
name = "rfc"
prefix = "RFC"
lifecycle = { states = ["draft", "review", "accepted", "in-progress", "complete", "rejected", "superseded"], edges = [{ from = "draft", to = "review" }, { from = "review", to = "accepted" }, { from = "accepted", to = "in-progress" }, { from = "in-progress", to = "complete" }, { from = "*", to = "rejected" }, { from = "*", to = "superseded" }] }
```

Projects whose `[[types]]` predate the lifecycle axis can backfill the default
lifecycle with `lazyspec fix --config` (see *Migrating an Existing Config*).

### Relationships

The relationship vocabulary is config-driven, just like document types. Each
`[[relationships]]` block declares a relationship `name` and an optional
`inverse` keyword. A directional relationship declares its inverse (e.g.
`implements` / `implemented-by`); a relationship with no `inverse` is symmetric
(e.g. `related-to`). `link`/`unlink` resolve the keyword you type against this
registry -- a canonical `name` links in the stated direction, while a declared
`inverse` flips it and stores the canonical relation on the target. `validate`
flags any document carrying a relationship name not declared here.

```toml
[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "tracks"
inverse = "tracked-by"

[[relationships]]
name = "related-to"
```

### Validation Rules

Validation rules define structural constraints between document types. Two shapes are supported:

- `parent-child` -- the child type must link to a parent type via a given relationship.
- `relation-existence` -- documents of a given type must have at least one relationship.

A `parent-child` rule may also carry `require_parent_status`: when set, `create`
of the child type is refused unless at least one parent document of the rule's
`parent` type has reached that status. The required status must be a valid state
of the parent type's lifecycle. Rules without `require_parent_status` impose no
creation gate.

```toml
[[rules]]
shape = "parent-child"
name = "stories-need-rfcs"
child = "story"
parent = "rfc"
link = "implements"
severity = "warning"
require_parent_status = "accepted"  # optional: a story cannot be created until an rfc is accepted

[[rules]]
shape = "relation-existence"
name = "adrs-need-relations"
type = "adr"
require = "any-relation"
severity = "error"
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

### Agents

The global `[agents]` block configures interactive agent run mode. When an interactive-mode template is selected in the agent dialog, the configured shell command runs via `bash -lc` with the rendered template body exported as `$LAZYSPEC_PROMPT` and the document path as `$LAZYSPEC_DOC_PATH`.

```toml
[agents]
interactive = 'claude "$LAZYSPEC_PROMPT"'
# or 'opencode -p "$LAZYSPEC_PROMPT"', 'pi', 'tmux new-window claude "$LAZYSPEC_PROMPT"'
```

Zero-defaults: when `[agents] interactive` is unset, interactive-mode templates (`mode: interactive`) are not offered. Headless-mode templates (`mode: headless`) continue to work using the standard `claude -p` command.

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
