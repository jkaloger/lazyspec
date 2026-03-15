---
title: "Commands"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, cli]
related:
  - related-to: "docs/stories/STORY-002-cli-commands.md"
  - related-to: "docs/stories/STORY-019-context-command.md"
  - related-to: "docs/stories/STORY-020-status-command.md"
  - related-to: "docs/stories/STORY-041-cli-child-document-support.md"
  - related-to: "docs/stories/STORY-045-frontmatter-fix-command.md"
  - related-to: "docs/stories/STORY-054-forward-and-backward-context-with-related-records.md"
---

# Commands

Commands that don't need the Store (and skip `Store::load`):
- `init` -- creates config and directories
- `create` -- only needs config for type lookup and template rendering

## init

Creates `.lazyspec.toml` with defaults, type directories, and template directory.

## create

Creates a new document from a template.

1. Look up type definition in config
2. Load template from `.lazyspec/templates/{type}.md` (or use built-in default)
3. Generate filename via `resolve_filename(pattern, type, title, dir)`
4. Render template with variable substitution
5. Write file to disk

## list

Lists documents with optional filters.

**Filters:** `--type`, `--status`
**Output:** Grouped cards (human) or JSON array

## show

Displays a single document resolved by shorthand ID or path.

**Flags:**
- `-e` / `--expand-references` -- expand @ref directives inline
- `--max-ref-lines N` -- truncation limit per ref (default 25)

## update

Modifies frontmatter fields in-place using `rewrite_frontmatter()`. Currently
supports `--status` and `--title`. The YAML is parsed, mutated, and re-serialized
while preserving the body unchanged.

@ref src/engine/document.rs#rewrite_frontmatter

## delete

Removes a document file from disk.

## link / unlink

Adds or removes a relation entry in the source document's `related` array.
Operates on the YAML directly via `rewrite_frontmatter()`.

## search

Case-insensitive substring search across title, tags, and body content.
Returns results with match field and context snippet.

## status

Project overview showing all documents grouped by type, with counts per status.
JSON mode includes validation summary.
See [STORY-020: Status Command](../../stories/STORY-020-status-command.md).

## context

Traverses the document chain by following `implements` relations upward to
find the root, then displays the full chain with forward implementations
and related documents. See [STORY-019: Context Command](../../stories/STORY-019-context-command.md)
and [STORY-054: Forward and Backward Context](../../stories/STORY-054-forward-and-backward-context-with-related-records.md).

@ref src/cli/context.rs#ResolvedContext

@ref src/cli/context.rs#resolve_chain

```d2
direction: down

target: "Start: ITERATION-001"

chain: "Chain traversal" {
  rfc: "RFC-001"
  story: "STORY-001"
  iter: "ITERATION-001 <- you are here"

  rfc -> story: "implements"
  story -> iter: "implements"
}

forward: "Forward context" {
  desc: "Docs that implement the target"
}

related: "Related context" {
  desc: "RelatedTo links from any chain member"
}

target -> chain
chain -> forward: "reverse_links[target]"
chain -> related: "forward+reverse RelatedTo"
```

## validate

Runs `validate_full()` against the store and config. Outputs errors and
optionally warnings (`--warnings` flag). Exits with code 2 on errors.

## fix

Auto-repairs documents with broken or incomplete frontmatter.
See [STORY-045: Frontmatter fix command](../../stories/STORY-045-frontmatter-fix-command.md)
and [RFC-020: Fix command numbering conflict resolution](../../rfcs/RFC-020-fix-command-numbering-conflict-resolution.md).

Capabilities:
- Adds missing required fields with sensible defaults
- Resolves command numbering conflicts (duplicate `{n}` values)
- Updates references in other documents when paths change

Supports `--dry-run` to preview changes without writing.

## ignore / unignore

Sets or removes the `validate-ignore: true` flag in a document's frontmatter.
See [STORY-030: Validate-Ignore Flag](../../stories/STORY-030-validate-ignore-flag.md).

## CLI Interface Reference

```
lazyspec init
lazyspec create <type> <title> [--author NAME] [--json]
lazyspec list [TYPE] [--status STATUS] [--json]
lazyspec show <ID> [-e] [--max-ref-lines N] [--json]
lazyspec update <PATH> [--status STATUS] [--title TITLE]
lazyspec delete <PATH>
lazyspec link <FROM> <REL_TYPE> <TO>
lazyspec unlink <FROM> <REL_TYPE> <TO>
lazyspec search <QUERY> [--type TYPE] [--json]
lazyspec status [--json]
lazyspec context <ID> [--json]
lazyspec validate [--json] [--warnings]
lazyspec fix [PATHS...] [--dry-run] [--json]
lazyspec ignore <PATH>
lazyspec unignore <PATH>
lazyspec                          # launches TUI
```
