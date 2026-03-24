---
title: "Document Creation"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related:
  - related-to: docs/stories/STORY-007-document-creation-on-submit.md
  - related-to: docs/stories/STORY-043-cli-create-for-folder-based-documents.md
  - related-to: docs/stories/STORY-064-sqids-numbering-and-config.md
  - related-to: docs/stories/STORY-074-core-reservation-mechanism.md
  - related-to: docs/stories/STORY-076-reservation-management.md
---

## Summary

The `create` command produces a new document on disk from a template, handling type lookup, filename generation, numbering strategy dispatch, and optional subdirectory scaffolding. A companion `reservations` subcommand manages the remote reservation refs that accumulate when reserved numbering is in use.

## Create Command

The entry point is `run()`, which accepts a repo root, config, doc type name, title, author, and a progress callback. It returns the path of the created file.

@ref src/cli/create.rs#run

The command does not load the Store. It only needs the parsed `Config` to look up the type definition and resolve templates. This makes it one of the lightest commands in the CLI.

### Type Lookup

The doc type string is matched against `config.documents.types` by name. If no match is found, the error message lists all valid type names. The type definition carries the target directory, prefix, numbering strategy, and a `subdirectory` flag.

@ref src/engine/config.rs#TypeDef

### Numbering Strategy Dispatch

Each type definition declares a `NumberingStrategy`: `Incremental`, `Sqids`, or `Reserved`. The `run()` function dispatches on this enum before calling `resolve_filename`.

@ref src/engine/config.rs#NumberingStrategy

For `Incremental`, no pre-computation occurs. The `resolve_filename` function scans the target directory for existing files with the type prefix and picks `max + 1`.

@ref src/engine/template.rs#next_number

For `Sqids`, the sqids config (salt, min_length) is passed through to `resolve_filename`, which calls `next_sqids_id`. That function encodes the current Unix timestamp via the sqids crate, checks for filename collisions in the directory, and increments the input until no collision exists. All sqids IDs are lowercased.

@ref src/engine/template.rs#next_sqids_id

For `Reserved`, the command calls `reserve_next` before resolving the filename. This function queries the remote for existing reservation refs under `refs/reservations/{PREFIX}/*`, picks `max(remote_max, local_max) + 1`, creates a local ref, and atomically pushes it. If the push is rejected (another reservation claimed that number), the local ref is cleaned up, the candidate is incremented, and the push is retried up to `max_retries` times (default 5). Once reserved, the number is formatted according to `ReservedFormat` -- either zero-padded incremental (`{:03}`) or sqids-encoded -- and passed to `resolve_filename` as a pre-computed ID. The template layer never touches git.

@ref src/engine/reservation.rs#reserve_next

@ref src/engine/config.rs#ReservedFormat

### Filename Resolution

`resolve_filename` applies the naming pattern from config (e.g. `{type}-{n:03}-{title}.md`), substituting `{type}` (uppercased), `{title}` (slugified), `{date}`, and `{n}`/`{n:03}`. When a pre-computed ID is provided, it replaces both `{n}` and `{n:03}` directly. When no number placeholder exists in the pattern, numbering is skipped entirely.

@ref src/engine/template.rs#resolve_filename

### Template Rendering

`load_template` looks for a file at `{templates_dir}/{type}.md`. If the file exists, it is read; otherwise `default_template` provides a built-in fallback. Built-in defaults exist for `story`, `iteration`, `spec`, and a generic catch-all. Template variables (`{title}`, `{author}`, `{date}`, `{type}`) are replaced via simple string substitution in `render_template`.

@ref src/cli/create.rs#load_template

@ref src/cli/create.rs#default_template

### Subdirectory Mode

When `type_def.subdirectory` is true, the command creates a directory named after the filename stem (minus `.md`) inside the type directory. It writes `index.md` using the standard template and `story.md` using a dedicated story template that includes an Acceptance Criteria skeleton. The returned path points to `index.md`.

@ref src/cli/create.rs#story_template

### JSON Output

`run_json` wraps `run`, reads the created file back, parses its frontmatter into `DocMeta`, and serializes it as pretty-printed JSON. The path in the output is relative to the repo root.

@ref src/cli/create.rs#run_json

## Reservations CLI

The `reservations` subcommand provides `list` and `prune` for managing remote reservation refs.

### List

`run_list` calls `list_reservations`, which runs `git ls-remote --refs {remote} refs/reservations/*` and parses each line into a `Reservation` struct (prefix, number, ref_path). Human output is tab-separated; `--json` produces a JSON array.

@ref src/cli/reservations.rs#run_list

### Prune

`run_prune` fetches all reservations, then for each one formats the number (using `ReservedFormat` dispatch) and checks whether a matching document exists in the local store by looking for `{PREFIX}-{formatted_number}` in any document path. Reservations with a local match are pruned (the remote ref is deleted via `git push --delete`). Those without a match are reported as orphans. `--dry-run` previews the actions without deleting. JSON output groups results into `pruned`, `orphaned`, and `errors` arrays.

@ref src/cli/reservations.rs#run_prune
