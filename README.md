<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A little TUI & CLI for project documentation.
</p>

<img width="1864" height="1193" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/18bdd9a7-16db-43f6-b6cc-a3dced1c9f66" />

> [!WARNING]
> Lazyspec is experimental. APIs and CLI interfaces will change frequently and without notice.

## Features

Lazyspec manages project documentation as version-controlled markdown files with YAML frontmatter. Documents live in your repo, so agents and humans read from the same source of truth.

- Create, update, link, and validate documents. Typed relationships (`implements`, `supersedes`, `blocks`, `related-to`) keep the chain explicit.
- Catch broken links, orphaned documents, and incomplete frontmatter before they rot. `lazyspec validate` exits non-zero on errors, so it slots into CI.
- Embed `@ref` directives in your specs to point at source code. Lazyspec expands them inline using `git show`, with symbol-level extraction for Rust and TypeScript.
- Fuzzy search, markdown preview, live file watching, and document creation without leaving the terminal.
- Every command supports `--json` output for automation and agent integration.
- Define your own types, templates, and directory layout in `.lazyspec.toml`.

## Install

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

| Skill              | Purpose                                                              |
| ------------------ | -------------------------------------------------------------------- |
| `plan-work`        | Detect existing artifacts and determine the right entry point       |
| `write-rfc`        | Propose a design with intent, interface sketches, and identify stories |
| `create-story`     | Create stories with acceptance criteria linked to an RFC             |
| `resolve-context`  | Gather full document chain (RFC -> Story -> Iteration) before work  |
| `create-iteration` | Plan an iteration with task breakdown and test plan                 |
| `build`            | Implement tasks from an iteration with subagent dispatch             |
| `review-iteration` | Two-stage review -- AC compliance first, then code quality           |
| `create-audit`     | Criteria-based review (health check, security, accessibility, etc.)  |


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

Running `lazyspec` with no subcommand opens the interactive dashboard. It provides fuzzy search, markdown preview, document creation, and live file watching -- documents update automatically when changed on disk.

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
| `link <from> <rel> <to>`             | Add a typed relationship (implements, supersedes, blocks, related-to) |
| `unlink <from> <rel> <to>`           | Remove a relationship between documents                               |
| `search <query> [--doc-type X]`      | Full-text search across all documents                                 |
| `context <id>`                       | Show the full document chain (RFC -> Story -> Iteration)              |
| `status`                             | Show full project status with all documents and validation            |
| `ignore <path>`                      | Mark a document to skip validation                                    |
| `unignore <path>`                    | Remove validation skip from a document                                |
| `validate [--warnings]`              | Check document integrity and link consistency                         |
| `fix [paths] [--dry-run]`            | Fix documents with broken or incomplete frontmatter                   |
| `reservations list`                  | Show all reservation refs on the remote                               |
| `reservations prune [--dry-run]`     | Remove refs for documents that already exist locally                  |

#### `show` Flags

| Flag                        | Description                                      |
| --------------------------- | ------------------------------------------------ |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`         | Max lines per expanded ref (default: 25)         |

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

### Numbering

Document numbers are assigned automatically during `create`. Three strategies are available per type:

| Strategy      | Behaviour |
|---------------|-----------|
| `incremental` | Next sequential integer from existing files (default) |
| `sqids`       | Short hash-like IDs derived from a timestamp, configured via `[numbering.sqids]` |
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

```sh
cargo build
cargo test
```
