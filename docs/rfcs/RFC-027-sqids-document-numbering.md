---
title: "Sqids Document Numbering"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-03-15
tags:
  - numbering
  - distributed
  - sqids
related:
  - related to: docs/rfcs/RFC-020-fix-command-numbering-conflict-resolution.md
  - related to: docs/rfcs/RFC-013-custom-document-types.md
---


## Problem

Document numbering uses sequential integers: `RFC-001`, `RFC-002`, `RFC-003`. The `next_number` function scans the directory, finds the highest existing number, and increments it:

@ref src/engine/template.rs#next_number

This is simple and produces readable IDs, but it breaks in distributed workflows. When two people on separate branches both create an RFC, they both get the same number. RFC-020 addresses this by detecting and renumbering conflicts after the fact, but that's a repair mechanism, not prevention.

Sequential numbering also leaks information. The number reveals how many documents of that type exist and roughly when they were created. For some teams this is fine. For others (open-source projects, shared specs with external parties) it's an unnecessary signal.

## Intent

Add an alternative numbering strategy using [sqids](https://sqids.org/) that generates short, unique, non-sequential IDs. Sqids produces URL-safe, collision-resistant identifiers from integer inputs, giving us IDs like `RFC-k3f` or `STORY-mQ7` instead of `RFC-022` or `STORY-064`.

This is complementary to the existing incremental numbering. Projects choose their strategy per-type in config. Incremental numbering remains the default. RFC-020's conflict resolution still applies to projects using incremental mode.

## Design

### Sqids Overview

Sqids (pronounced "squids") encodes integers into short, unique strings. Key properties:

- Deterministic: same input always produces same output (given same alphabet and salt)
- Reversible: the ID can be decoded back to the source integer
- Short: typically 3-5 characters for reasonable input ranges
- URL-safe: only uses alphanumeric characters
- Shuffleable: a project-specific salt changes the output alphabet, so `RFC-k3f` in one project maps to a different number than `RFC-k3f` in another

The Rust implementation is [`sqids`](https://crates.io/crates/sqids).

### Configuration

Numbering strategy is configured per-type in `.lazyspec.toml`:

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"        # "incremental" (default) or "sqids"

[numbering.sqids]
min_length = 3             # minimum ID length (default: 3)
alphabet = ""              # custom alphabet (default: sqids default)
salt = "my-project-salt"   # project-specific salt for unique sequences
```

The `[numbering.sqids]` section is global (not per-type). All types using `numbering = "sqids"` share the same sqids configuration. This keeps IDs consistent across the project.

When `numbering` is omitted or set to `"incremental"`, the existing `next_number` behavior is unchanged.

### ID Generation

With sqids numbering, `next_number` changes behavior:

1. Scan the directory for existing documents (same as today)
2. Use the current Unix timestamp (seconds precision) as the sqids input
3. Encode the timestamp through sqids to produce the ID
4. Verify the generated filename doesn't already exist (handle hash collisions)
5. If collision, increment the input and try again

```rust
@draft NumberingStrategy {
    Incremental,                    // existing behavior
    Sqids { config: SqidsConfig },  // new
}

@draft SqidsConfig {
    min_length: u8,     // default 3
    alphabet: String,   // default sqids alphabet
    salt: String,       // project-specific
}
```

@ref src/engine/template.rs#next_number

The `next_number` function signature changes to return a `String` instead of `u32`, or a new `next_id` function is introduced alongside it. The string is the formatted ID portion (e.g. `k3f` or `022`).

### ID Resolution

`resolve_shorthand` currently parses the numeric suffix to match documents:

@ref src/engine/store.rs#extract_id_from_name

With sqids IDs, the extractor changes: instead of looking for a numeric suffix after the prefix, it looks for an alphanumeric suffix. `RFC-k3f` extracts `k3f`, `RFC-022` extracts `022`. The resolution logic doesn't need to know which strategy produced the ID; it just matches the extracted ID string against available documents.

This means sqids and incremental documents can coexist in the same directory (though mixing is discouraged). A type configured as `sqids` might have legacy `RFC-001` documents alongside new `RFC-k3f` documents, and resolution handles both.

### Filename Format

Sqids IDs follow the same filename pattern: `{PREFIX}-{ID}-{slug}.md`

| Strategy | Example |
|----------|---------|
| Incremental | `RFC-022-tui-status-bar.md` |
| Sqids | `RFC-k3f-tui-status-bar.md` |

The ID portion is always lowercase for sqids (sqids default alphabet is lowercase alphanumeric). This avoids case-sensitivity issues on case-insensitive filesystems.

### Migration via `fix`

Switching a type's config from `incremental` to `sqids` (or vice versa) doesn't require migration. New documents get the configured strategy; existing documents keep their IDs. Both resolve correctly in the same directory.

But teams that want a clean cutover can use `lazyspec fix` to convert between formats. The `fix` command already handles frontmatter repair and (per RFC-020) numbering conflicts. Format conversion is another class of numbering fix.

**Incremental to sqids:**

```bash
lazyspec fix --renumber sqids
lazyspec fix --renumber sqids --type rfc    # scope to one type
```

For each document of the targeted type(s):
1. Decode the current numeric ID
2. Encode it through sqids using the project's config
3. Rename the file: `RFC-022-tui-status-bar.md` -> `RFC-k3f-tui-status-bar.md`
4. Update all `related` frontmatter references across the project that pointed at the old path

Step 4 is critical. Without it, renaming a file silently breaks every relationship that targets it. The `fix` command already walks all documents to repair frontmatter, so cascading path updates fits naturally into that pass.

**Sqids to incremental:**

```bash
lazyspec fix --renumber incremental
lazyspec fix --renumber incremental --type rfc
```

Same process in reverse. Decode the sqids ID back to its integer, format as zero-padded number, rename, cascade references. Documents are renumbered in filesystem order (alphabetical by filename), so the resulting sequence is deterministic but may not match the original creation order.

**Dry run:**

Both directions support `--dry-run` to preview what would change without touching the filesystem:

```bash
lazyspec fix --renumber sqids --dry-run
```

Output lists each rename and each reference update that would occur. This lets users audit the impact before committing to a potentially disruptive operation.

**What gets updated during cascade:**

| Location | Update |
|----------|--------|
| `related` frontmatter paths in all documents | Old path -> new path |
| `@ref` directives pointing at renamed docs | Not affected (refs point at source code, not docs) |
| External links (READMEs, wikis, browser bookmarks) | Not updated (out of scope, warned about) |

The command prints a summary of external references it found but couldn't update (e.g. markdown links in non-lazyspec files), so users know what to fix manually.

### Trade-offs

| Aspect             | Incremental                    | Sqids                                    |
|--------------------|--------------------------------|------------------------------------------|
| Readability        | High (`RFC-022` is obvious)    | Medium (`RFC-k3f` is opaque)             |
| Conflict risk      | High in distributed workflows  | Negligible (requires same-second create) |
| Ordering           | Implicit (higher number = newer)| None (IDs don't sort chronologically)   |
| Information leakage| Reveals document count         | Obscured                                 |
| Reversibility      | Trivial                        | Requires sqids config                    |

Teams that value readable, ordered IDs should stick with incremental. Teams that value conflict-free distributed creation should use sqids.

## Stories

1. **Sqids numbering strategy** -- Add `sqids` crate dependency. `NumberingStrategy` enum, `SqidsConfig` parsing from `.lazyspec.toml`. New `next_id` function that dispatches to incremental or sqids. Update `create` command to use it.

2. **ID resolution for mixed formats** -- Update `extract_id_from_name` and `resolve_shorthand` to handle alphanumeric IDs alongside numeric ones. Ensure sqids and incremental documents coexist.

3. **Config and validation** -- `numbering` field on `[[types]]`, `[numbering.sqids]` global section. Validate salt is present when sqids is used. Validate `min_length` is reasonable (1-10).

4. **Fix: numbering format conversion** -- `lazyspec fix --renumber sqids|incremental` with optional `--type` scoping. File renames, reference cascade across all document frontmatter, `--dry-run` preview. Summary of external references that couldn't be auto-updated.
