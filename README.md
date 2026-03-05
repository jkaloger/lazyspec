<h1>
<p align="center">
  🤖
  <br>lazyspec
</h1>
  <p align="center">
    A little TUI for project documentation.
  </p>
</p>


<img width="1292" height="1193" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/6a1447f9-7397-4db5-9ee5-8720bd500269" />


## Install

```sh
cargo install --path .
```

## Quick Start

```sh
lazyspec --help
```

Launch the TUI dashboard:

```sh
lazyspec
```

## CLI

| Command                    | Description                                    |
| -------------------------- | ---------------------------------------------- |
| `init`                     | Scaffold a new project                         |
| `create <type> <title>`    | Create a document (rfc, adr, story, iteration) |
| `list [type] [--status X]` | List documents with optional filters           |
| `show <id>`                | Display a document                             |
| `update <path> --status X` | Update document metadata                       |
| `delete <path>`            | Delete a document                              |
| `link <from> <rel> <to>`   | Add a typed relationship                       |
| `search <query>`           | Full-text search across all documents          |
| `validate`                 | Check document integrity and link consistency  |

## TUI

The dashboard provides fuzzy search, markdown preview, document creation, and live file watching. Documents update automatically when changed on disk.

## Development

```sh
cargo build
cargo test
```
