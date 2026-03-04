---
title: "Core Document Management Tool"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-03-04
tags: [tui, cli, documentation, rust]
related: []
---

## Summary

A terminal-native tool for managing structured project documentation. Documents (RFCs, ADRs, Specs, Plans) are markdown files with YAML frontmatter, stored in configurable directories and linked through typed relationships.

The tool provides two interfaces: a CLI for scripting and agent integration, and a TUI dashboard for browsing and previewing documents.

## Problem

Project documentation lives in wikis, Google Docs, or scattered markdown files with no structure. There's no way to express relationships between documents (this ADR implements that RFC, this plan supersedes that one), no validation that links are correct, and no unified view across document types.

Existing tools either lock you into a specific platform or treat docs as flat files with no semantic structure.

## Design Intent

Documents are the unit of work. Each has a type, status lifecycle, author, tags, and typed relationships to other documents. The frontmatter schema is intentionally minimal but sufficient to support queries, filtering, and link validation.

The relationship model uses four types: `implements`, `supersedes`, `blocks`, and `related-to`. These are stored in the source document's frontmatter and resolved bidirectionally at query time.

### Architecture

Single Rust binary. No arguments launches the TUI. Subcommands run CLI operations.

```
lazyspec (binary)
  engine/    # Document model, store, queries, templates, linking
  cli/       # Clap commands that call into engine
  tui/       # Ratatui app that calls into engine
```

The engine is the shared core. CLI and TUI are thin consumers that compose engine primitives.

### Store Design

The Store loads all documents on startup by walking configured directories and parsing only frontmatter (stops at the second `---`). Body content is loaded lazily when a document is selected for preview. This keeps startup fast regardless of document count.

Mutations write to disk first, then update in-memory state. Disk is always the source of truth.

### TUI Layout

Three-panel dashboard:
- Left: document type selector with counts
- Top right: document list for selected type (title, status with color coding)
- Bottom right: rendered markdown preview of selected document

Navigation follows vim conventions (h/j/k/l), with `/` for fuzzy search and `?` for help.

### CLI Interface

```
lazyspec init                        # Create config and directories
lazyspec create <type> <title>       # Create from template
lazyspec list [type] [--status X]    # List docs, filterable
lazyspec show <path-or-id>           # Print doc to stdout
lazyspec update <path> --status X    # Update frontmatter fields
lazyspec delete <path>               # Remove doc
lazyspec link <from> <rel> <to>      # Add a relationship
lazyspec unlink <from> <rel> <to>    # Remove a relationship
lazyspec validate                    # Check frontmatter and links
```

All commands support `--json` for machine-readable output.

## Stories

1. Document model and store (parsing, indexing, link resolution, validation)
2. CLI commands (init, create, list, show, update, delete, link, unlink, validate)
3. TUI dashboard (layout, navigation, markdown preview, fuzzy search, file watching)
