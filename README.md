<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A little TUI for project documentation.
</p>

lazyspec gives your codebase a lightweight spec layer. Define decisions, track features from RFC through to iteration, and validate that everything links together. Works from the terminal as a dashboard or as a scriptable CLI with `--json` output.

<img width="1864" height="1193" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/18bdd9a7-16db-43f6-b6cc-a3dced1c9f66" />

## Install

```sh
cargo install --git https://github.com/jkaloger/lazyspec
```

### Local

```sh
cargo install --path .
```

## Quick Start

Initialise a new project, then launch the TUI:

```sh
lazyspec init
lazyspec
```

## CLI Reference

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

Most commands accept `--json` for machine-readable output.

### `show` flags

| Flag                    | Description                                  |
| ----------------------- | -------------------------------------------- |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`        | Max lines per expanded ref (default: 25)         |

### `@ref` syntax

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

## TUI

The dashboard provides fuzzy search, markdown preview, document creation, and live file watching. Documents update automatically when changed on disk.

## Development

```sh
cargo build
cargo test
```
