<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A little TUI for project documentation.
</p>

lazyspec gives your codebase a lightweight spec layer. Define decisions, track features from RFC through to iteration, and validate that everything links together. Works from the terminal as a dashboard or as a scriptable CLI with `--json` output.

<img width="1292" height="1193" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/6a1447f9-7397-4db5-9ee5-8720bd500269" />

## Install

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
| `show <id>`                          | Display a document by path or shorthand ID (e.g. `RFC-001`)           |
| `update <path> --status X --title X` | Update document frontmatter                                           |
| `delete <path>`                      | Delete a document                                                     |
| `link <from> <rel> <to>`             | Add a typed relationship (implements, supersedes, blocks, related-to) |
| `unlink <from> <rel> <to>`           | Remove a relationship between documents                               |
| `search <query> [--doc-type X]`      | Full-text search across all documents                                 |
| `context <id>`                       | Show the full document chain (RFC -> Story -> Iteration)              |
| `status`                             | Show full project status with all documents and validation            |
| `validate [--warnings]`              | Check document integrity and link consistency                         |

Most commands accept `--json` for machine-readable output.

## TUI

The dashboard provides fuzzy search, markdown preview, document creation, and live file watching. Documents update automatically when changed on disk.

## Development

```sh
cargo build
cargo test
```
