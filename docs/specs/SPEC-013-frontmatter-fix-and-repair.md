---
title: "Frontmatter Fix and Repair"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [cli, fix, frontmatter, conflicts]
related:
  - related-to: "docs/stories/STORY-045-frontmatter-fix-command.md"
  - related-to: "docs/stories/STORY-059-conflict-detection-and-renumbering.md"
  - related-to: "docs/stories/STORY-060-reference-cascade.md"
  - related-to: "docs/stories/STORY-061-graceful-degradation-for-duplicate-ids.md"
  - related-to: "docs/stories/STORY-062-validation-diagnostic-for-duplicate-ids.md"
---

## Summary

The `lazyspec fix` command repairs broken or incomplete frontmatter and resolves document numbering conflicts that arise from concurrent branch work. It operates across three layers: field repair (filling missing required frontmatter), conflict resolution (detecting and renumbering duplicate IDs), and reference cascade (rewriting stale paths in other documents after a rename). The command supports `--dry-run` for previewing changes and `--json` for machine-readable output.

## Entry Point and Output Shape

@ref src/cli/fix.rs#run

The top-level `run` function orchestrates both field fixes and conflict fixes in a single pass. It delegates to `plan_field_and_conflict_fixes`, which calls the field and conflict collectors sequentially, then returns a `FixOutput` struct.

@ref src/cli/fix.rs#FixOutput

`FixOutput` contains two vectors: `field_fixes` (one `FieldFixResult` per document that was repaired) and `conflict_fixes` (one `ConflictFixResult` per document that was renumbered). The human-readable formatter in `output.rs` iterates both vectors and prints one line per action, prefixed with "Would" in dry-run mode.

## Field Repair

@ref src/cli/fix.rs#FieldFixResult

When called with explicit paths, `collect_field_fixes` operates on those files. When called with no paths, it targets every document that has a parse error in the store. For each file, the function parses the YAML frontmatter (or creates an empty mapping if none exists) and inserts any missing fields from a fixed required-fields list.

@ref src/cli/fix/fields.rs#REQUIRED_FIELDS

The six required fields are `title`, `type`, `status`, `author`, `date`, and `tags`. Default values are derived contextually: `title` is extracted from the filename by stripping the type prefix and numeric segment, `type` is inferred by matching the file's parent directory against the configured type definitions, `status` defaults to `"draft"`, `author` comes from `git config user.name`, `date` is today's date, and `tags` is an empty sequence.

@ref src/cli/fix/fields.rs#default_for_field

The function `split_frontmatter` handles the parse boundary. If the file has no `---` delimiters at all, the entire content is treated as the body and a fresh frontmatter block is generated. Existing fields are never overwritten; only missing keys are inserted.

## Conflict Detection and Renumbering

@ref src/cli/fix/conflicts.rs#collect_conflict_fixes

Numbering conflicts occur when two contributors independently create documents of the same type on separate branches. After merge, both documents share an ID (e.g. two `RFC-020` files). The conflict detector groups all non-virtual documents by their extracted ID using `extract_id_from_name`, then processes any group with two or more members.

@ref src/cli/fix/conflicts.rs#renumber_doc

Within a conflict group, the oldest document wins. Documents are sorted first by their `date` frontmatter value, then by filesystem mtime as a tiebreaker. The first document in sort order keeps its number; all others are renumbered. The new ID is generated using the type's configured numbering strategy (incremental or sqids). Types using the `Reserved` strategy are skipped entirely.

Both flat files and subfolder documents (where the document is `index.md` inside a named directory) are handled. For subfolder documents, the entire directory is renamed rather than just the file. After renaming, the frontmatter title is updated if it contains the old ID prefix.

## Renumber Subcommand

@ref src/cli/fix.rs#run_renumber

A separate `fix renumber` subcommand converts all documents of a given type between numbering formats. It supports two formats: `incremental` (zero-padded three-digit numbers) and `sqids` (short hash-like IDs derived from the document's original number and a salt). The renumber operation also triggers reference cascade for every renamed document, and scans for external references that could not be auto-updated.

@ref src/cli/fix/renumber.rs#scan_external_references

External reference scanning walks the project tree outside managed document directories, checking `.md`, `.wiki`, and `README` files for occurrences of old document filenames. These are reported as warnings since they live outside the store and cannot be safely rewritten.

## Reference Cascade

@ref src/cli/fix/renumber.rs#cascade_references

When a document is renamed, all other documents in the store are scanned for stale references. The cascade targets two locations: `related` frontmatter entries (where the value is a path string) and `@ref` body directives (matched via `REF_PATTERN`). For each match containing the old path, the old path substring is replaced with the new path. In dry-run mode, the `ReferenceUpdate` entries are still collected and reported but no files are written.

@ref src/cli/fix.rs#ReferenceUpdate

Each `ReferenceUpdate` records the file that was updated, which field was changed (`"related"` or `"body"`), and the old and new values. These are nested inside the parent rename result's `references_updated` vector, giving callers a complete audit trail of every path rewrite.

## Duplicate ID Graceful Degradation

Duplicate IDs affect more than just the fix command. When `resolve_shorthand` encounters an ambiguous ID, it returns an error listing all matching document paths rather than silently picking one. The `show` command surfaces this as a user-facing error with instructions to use the full path. The `list` and `context` commands continue to display all documents, including duplicates, without filtering or crashing.

The validation engine also participates: `validate_full` groups documents by extracted ID and emits a `DuplicateId` error for any ID shared by two or more documents. Documents with `validate_ignore: true` are excluded from this grouping. This diagnostic appears in both human-readable and JSON validation output.

@ref src/engine/validation.rs#ValidationIssue
