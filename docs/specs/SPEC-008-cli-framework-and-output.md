---
title: "CLI Framework and Output"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [cli, json, styling, completions]
related:
  - related-to: "docs/stories/STORY-002-cli-commands.md"
  - related-to: "docs/stories/STORY-021-json-everywhere.md"
  - related-to: "docs/stories/STORY-023-styled-cli-output.md"
  - related-to: "docs/stories/STORY-068-universal-id-resolution.md"
  - related-to: "docs/stories/STORY-070-shell-completions.md"
---

## Summary

The CLI is a Clap-derived command router that dispatches to per-command modules under `src/cli/`. Every command that produces output supports two modes: human-readable with ANSI styling, and machine-readable JSON via `--json`. Document arguments accept either full relative paths or shorthand IDs (e.g. `RFC-001`), resolved through a shared helper. Shell completions provide dynamic tab-completion for document IDs and relationship types.

## Command Routing

The top-level `Cli` struct contains an optional `Commands` enum. When no subcommand is provided, the TUI launches. When a subcommand is present, `main()` dispatches to the corresponding module.

@ref src/cli.rs#Cli

@ref src/cli.rs#Commands

Two commands skip `Store::load` entirely: `init` (no project exists yet) and `create` (only needs `Config` for type lookup and template rendering). All other commands load both `Config` and `Store` before dispatch.

## Output Modes

### JSON Serialization

Commands that accept `--json` serialize documents through `doc_to_json`, which produces a consistent schema: path, title, type, status, author, date, tags, related, and validate_ignore. The `doc_to_json_with_family` variant adds parent/children fields by querying the store's hierarchical relationships, and includes a `virtual_doc` flag when applicable.

@ref src/cli/json.rs#doc_to_json

@ref src/cli/json.rs#doc_to_json_with_family

### Human-Readable Styling

The `style` module provides primitives used across all human-mode output. The `console` crate handles ANSI detection -- when colors are disabled (piped output, dumb terminal), all styling functions fall back to plain text.

@ref src/cli/style.rs#doc_card

@ref src/cli/style.rs#styled_status

Status colors follow a fixed mapping: green for Accepted, yellow for Draft, blue for Review, red for Rejected, and color256(8) (gray) for Superseded. The `doc_card` function composes bold type prefix, bold title, colored status, and dimmed path into a single line used by list, status, and context output. Error and warning prefixes use red cross-mark and yellow exclamation respectively, falling back to `error:` and `warning:` text when colors are unavailable.

## ID Resolution

All commands that accept a document reference route through `resolve_shorthand_or_path` in the shared resolve module. This function first attempts a direct path lookup in the store. If that fails, it delegates to `Store::resolve_shorthand`, which matches the input against canonical document names.

@ref src/cli/resolve.rs#resolve_shorthand_or_path

Resolution produces two error variants. `ResolveError::NotFound` is returned when no document matches. `ResolveError::Ambiguous` is returned when multiple documents share a prefix, and the error message lists all matching paths so the user can disambiguate.

The resolve module also exposes `resolve_to_path`, a convenience wrapper that returns the owned `PathBuf` rather than a reference to the `DocMeta`. Commands like delete, update, link, unlink, ignore, and unignore use this variant since they need the path for filesystem operations.

## Shell Completions

The `completions` subcommand generates shell-specific completion scripts. For shells that support dynamic completions, `CompleteEnv` hooks into clap's runtime completion protocol. For other shells, static scripts are generated via `clap_complete::generate`.

@ref src/cli/completions.rs#complete_doc_id

Dynamic document ID completion loads `Config` and `Store` from the current directory at completion time, then filters all documents whose `id` field starts with the current input. Relationship type completion filters against `RelationType::ALL_STRS`. Both completers degrade gracefully -- if the store fails to load, they return an empty candidate list rather than erroring.

## Trivial Commands

### init

Creates `.lazyspec.toml` with default configuration, all document-type directories, and the template directory. Fails immediately if `.lazyspec.toml` already exists.

@ref src/cli/init.rs#run

### delete

Resolves the document via shorthand or path, then removes the file from disk. Returns an error if the resolved file does not exist.

### update

Resolves the document, reads its content, splits frontmatter from body using `split_frontmatter`, applies key-value updates to matching YAML lines, and writes the modified file back. Currently supports `--status` and `--title`.

@ref src/cli/update.rs#run

### link / unlink

Both commands resolve the `from` and `to` arguments via shorthand or path. `link` appends a new mapping entry to the source document's `related` YAML sequence, creating the sequence if absent. `unlink` retains all entries except the one matching the given relationship type and target path.

@ref src/cli/link.rs#link

### ignore / unignore

`ignore` sets `validate-ignore: true` in the document's frontmatter. `unignore` removes the `validate-ignore` key entirely from the YAML mapping. Both use `rewrite_frontmatter` for safe YAML mutation.
