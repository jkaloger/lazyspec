---
title: "Fix command numbering conflict resolution"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-13
tags:
  - cli
  - fix
  - distributed
related:
  - related to: docs/rfcs/RFC-015-lenient-frontmatter-loading-with-warnings-and-fix-command.md
---

## Problem

When distributed teams work on the same lazyspec project across git branches, document numbering conflicts are inevitable. `next_number()` assigns the next available number by scanning the local filesystem:

@ref src/engine/template.rs#next_number

Two people on separate branches both run `lazyspec create rfc "My Feature"` and both get `RFC-020`. After merge, the project has two documents with the same numeric prefix. This causes silent data corruption:

- `resolve_shorthand("RFC-020")` returns whichever document the HashMap iterator yields first
- Relationships pointing at the old path silently break if the wrong file is returned
- `lazyspec context` and `lazyspec show` produce unpredictable results

There is no detection, warning, or resolution mechanism today.

## Design Intent

Extend the existing `fix` command to detect and resolve numbering conflicts. The `fix` command already repairs broken frontmatter -- numbering conflicts are another class of "broken state" that belongs in the same tool.

> [!NOTE]
> This deliberately avoids a new top-level command. `fix` is the established entry point for repairing project state, and conflict resolution fits that mental model.

### Detection

Build an ID-frequency map during `fix` by scanning all successfully-loaded documents (not just `store.parse_errors()`). Group documents by their extracted ID (e.g. `RFC-020`). Any ID with more than one document is a conflict.

@ref src/engine/store.rs#extract_id_from_name

### Resolution: Oldest Wins

When a conflict is found, the document with the earliest `date` frontmatter value keeps the original number. If dates are equal, fall back to filesystem mtime. The "losing" document gets renumbered to the next available number for its type.

### Cascade

Renumbering a document means updating every reference to it across the project. Four things must change:

1. **File or directory on disk** -- rename `RFC-020-caching-layer.md` to `RFC-021-caching-layer.md` (or `RFC-020-caching-layer/` directory for documents with children)
2. **Frontmatter title** -- if the title contains the old ID prefix, update it
3. **Relationship targets** -- every `related` entry across the entire store that references the old path must be rewritten to the new path
4. **Body references** -- any `@ref` directives pointing at the old document path must be updated

Relationship targets use full relative paths in frontmatter:

```yaml
related:
  - implements: docs/rfcs/RFC-020-caching-layer.md
```

So the cascade is a straightforward path string replacement across all document files.

### CLI Behaviour

The fix command interface does not change. Conflict resolution is additive:

```
lazyspec fix                    # fix fields + resolve conflicts
lazyspec fix --dry-run          # preview all fixes without writing
lazyspec fix --dry-run --json   # machine-readable preview
lazyspec fix --json             # apply and report as JSON
```

### JSON Output

The existing `FixResult` struct gains a sibling for conflict resolutions:

@ref src/cli/fix.rs#FixResult

```
@draft FixOutput {
  field_fixes: Vec<FieldFixResult>,    // existing FixResult, renamed for clarity
  conflict_fixes: Vec<ConflictFixResult>,
}

@draft ConflictFixResult {
  old_path: String,
  new_path: String,
  old_id: String,
  new_id: String,
  references_updated: Vec<ReferenceUpdate>,
  written: bool,
}

@draft ReferenceUpdate {
  file: String,
  field: String,       // "related" or "body"
  old_value: String,
  new_value: String,
}
```

### Validation Diagnostic

`engine/validation.rs` should also gain a duplicate-ID diagnostic so that `lazyspec validate` reports conflicts even when the user hasn't run `fix`. This gives visibility without requiring action.

### Graceful Degradation

Before conflicts are resolved, `show`, `list`, `context`, and the TUI must not crash or silently hide documents. When duplicate IDs exist:

- `resolve_shorthand()` should return all matches (or at least warn about ambiguity) rather than silently picking one
- The TUI document list must display all documents, including duplicates -- they should be visually flagged (e.g. with a warning indicator) but not hidden or merged
- `lazyspec show RFC-020` on an ambiguous ID should list the conflicting documents and ask the user to specify by full path, rather than returning an arbitrary result

This ensures that the project remains usable while conflicts exist, even if `fix` hasn't been run yet.

### Subfolder Documents

Documents with children live in directories (e.g. `docs/rfcs/RFC-020-caching-layer/index.md`). Numbering conflicts can occur at this level too -- two directories with the same prefix.

Renumbering a subfolder document means:
- Renaming the entire directory (`RFC-020-caching-layer/` to `RFC-021-caching-layer/`)
- The `index.md` and all child documents move with the directory, so their paths change
- References to the parent document AND to any child documents must be updated in the cascade
- `extract_id()` already derives the parent ID from the directory name, so renaming the directory is sufficient for ID resolution

## Scope Boundaries

**In scope:**
- Conflict detection across all document types (flat files and subfolders)
- Automated renumbering with cascading reference updates
- Dry-run support with detailed reporting
- Validation diagnostic for duplicate IDs
- Graceful degradation in `show`, TUI, and other read paths

**Out of scope:**
- Prevention at `create` time (e.g. locking, reserving numbers) -- this is a detection-and-repair approach, not a prevention approach
- Interactive conflict resolution (oldest wins is deterministic and sufficient)
- Git merge driver integration -- this operates post-merge on the working tree

## Stories

1. **Conflict detection and renumbering** -- detect duplicate IDs, renumber the newer document, rename on disk, update frontmatter title. Handles both flat files and subfolder documents. The core `fix` extension without cascade.

2. **Reference cascade** -- after renumbering, rewrite all `related` targets and `@ref` body directives that reference the old path. For subfolder documents, also cascade child document path updates. Depends on Story 1.

3. **Graceful degradation** -- update `resolve_shorthand`, `show`, and TUI to handle duplicate IDs without crashing or hiding documents. Warn on ambiguity rather than silently picking a winner.

4. **Validation diagnostic** -- add a duplicate-ID rule to `validate` so conflicts surface even without running `fix`.
