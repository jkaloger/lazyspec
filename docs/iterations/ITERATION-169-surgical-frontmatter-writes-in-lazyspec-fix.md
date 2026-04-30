---
title: "Surgical frontmatter writes in lazyspec fix"
type: iteration
status: accepted
author: "agent"
date: 2026-04-30
tags: []
related: []
validate_ignore: true
---


## Summary

Standalone iter. `lazyspec fix` currently rewrites the entire frontmatter via `serde_yaml::to_string`, producing two diff-noise classes:

1. **Extra blank line** inserted between `---` and body. Caused by `format!("---\n{yaml}---\n{body}")` where `yaml` already ends in `\n` and `body` retains its leading `\n` from `split_frontmatter`.
2. **String requoting**. `"foo"` → `foo`; `'bar'` → `bar`; titles w/ colons get reflowed. Pure serde_yaml round-trip artifact.

Iter 165 surfaced this when `fix` ran on 286 docs — diff was 80% noise, 20% intent. This iter switches to surgical text-level inserts that preserve every byte the user didn't ask to change.

No new RFC/Story. Engineering hygiene iter on existing CLI.

## Acceptance Criteria

- AC1: doc lacking required field (e.g. `tags`) → after `fix`, diff is exactly `+tags: []` line; no other byte changes (no extra blank line, no requoting).
- AC2: doc lacking `priority` on `requires_priority=true` type → after `fix`, diff is exactly `+priority: should` line; no other byte changes.
- AC3: doc w/ all required fields + valid priority → `fix` is byte-equal no-op.
- AC4: doc w/ existing `title: "Quoted Title"` → `fix` (any backfill) preserves the double quotes.
- AC5: doc w/ existing `author: 'single-quoted'` → `fix` preserves single quotes.
- AC6: doc w/ bare-scalar `tags: []` (already present) → `fix` doesn't requote it.

## Test Plan

Tests in `src/cli/fix/fields.rs` `#[cfg(test)] mod tests` for unit + `tests/cli_fix_test.rs` for integration. `tempfile::TempDir` + `FileSystem` trait. Deterministic. DICTUM-004 conformant.

### Unit (`src/cli/fix/fields.rs`)

- AC1 — fixture w/ frontmatter missing `tags:` and a multi-line body. Run `fix_file`. Read post-write file. Assert diff (string compare): only the new `tags: []` line was inserted, body identical, frontmatter ordering of other fields preserved.
- AC2 — fixture story w/ no `priority:`. Run `fix_priority_file`. Assert byte-level diff is one inserted line.
- AC3 — fixture w/ all required + priority. Run `fix_file` then `fix_priority_file`. Assert post-write content byte-equal to pre.
- AC4 — fixture w/ `title: "Quoted Title"` + missing `tags:`. Run `fix_file`. Assert post-write file still contains `title: "Quoted Title"` literally (no requote to bare).
- AC5 — same shape, `author: 'jkaloger'`. Assert single quotes preserved.
- AC6 — fixture w/ existing `tags: []` bare-scalar + missing `author:`. Run `fix_file`. Assert `tags: []` line literally preserved.

### Integration (`tests/cli_fix_test.rs`)

- AC1+AC2 combined — TempDir w/ a story missing both `tags` and `priority`. Run `lazyspec fix --json`. Assert `field_fixes[*].fields_added` covers both. Read file. Compare line-by-line w/ original: only the two named fields differ; everything else byte-equal.

## Changes

### 1. Surgical-insert helper

- ACs: 1-6 (foundation).
- File: sibling fn in `src/cli/fix/fields.rs` (Principle 6 — one file, no submodule yet).
- Intent:
  - Add `pub(super) fn insert_yaml_field(yaml_text: &str, key: &str, value_yaml: &str) -> String`.
  - Behaviour: append a new line `{key}: {value_yaml}` to existing `yaml_text`. Preserve trailing newline status (if input ends in `\n`, output ends in `\n`; if not, append `\n` before the new line).
  - Caller passes value pre-serialised (`"[]"` for empty list, `"draft"` for plain string, `"should"` for priority).
  - Skip if key already present (defensive — scan for `^{key}:` line prefix). Caller filtering still does the primary check.
  - Edge: yaml_text empty → return `{key}: {value}\n`.

### 2. Switch `fix_file` to surgical insert

- ACs: 1, 3, 4, 5, 6.
- File: `src/cli/fix/fields.rs`.
- Intent:
  - Replace the round-trip in `fix_file`:
    - Read file → `split_frontmatter` → `(yaml_text, body)`.
    - Detect missing fields: regex/line-scan `^{field}:` per `REQUIRED_FIELDS`. Avoid serde_yaml on read side too — keeps the file completely untouched if all fields present.
    - For each missing field: `yaml_text = insert_yaml_field(&yaml_text, field, &default_yaml(field))`.
    - Reassemble: write back as `format!("---\n{yaml_text}---\n{body}")`. Verify exact-1-newline rule:
      - `yaml_text` after inserts must end in exactly one `\n`.
      - `body` starts w/ whatever `split_frontmatter` returns. Test AC1 fixture confirms no extra blank line.
  - New `default_yaml(field) -> &'static str` (or owned String for date/author): `"title" → derived`, `"type" → derived`, `"status" → "draft"`, `"author" → git_author()`, `"date" → today`, `"tags" → "[]"`.
  - Drop the `serde_yaml::Mapping` mutation path entirely from `fix_file`.

### 3. Switch `fix_priority_file` to surgical insert

- ACs: 2, 3.
- File: `src/cli/fix/fields.rs`.
- Intent:
  - Replace round-trip in `fix_priority_file` w/ `insert_yaml_field(&yaml_text, "priority", "should")`.
  - Reassemble preserves byte-exact.
  - Existing skip-if-present logic stays.

### 4. Verify on real corpus

- ACs: cleanup; non-AC.
- Intent:
  - After tasks 1-3 land + green: handcraft a fixture file w/ quoted title + multi-line related list + missing `priority`. Run `cargo run -- fix --json`. Inspect diff: must be exactly one `+priority: should` line.
  - `cargo test` full suite green.
- Verify: manual diff inspection + test suite.

## Notes

### Decisions (locked)

- **Surgical text insert, not YAML round-trip.** Round-trip is the root cause of both drift classes.
- **Append at end of frontmatter block.** Field ordering in YAML frontmatter has no semantic meaning to lazyspec. Simplest, preserves diff cleanliness.
- **Drop serde_yaml on read side too.** Line-scan for `^{field}:` is sufficient and avoids any parse → reserialise risk.
- **Don't fix already-rewritten docs.** Iter 165's commits land w/ noise; this iter prevents future drift only. Cleanup of legacy noise is out of scope.

### Open / deferred

- **Insertion-point preferences.** Some may want priority near the top. Defer.
- **YAML escaping in `value_yaml`.** Helper trusts the caller. Document the contract; if a future caller passes a value w/ embedded `\n` or `:`, behaviour undefined. Add `debug_assert!` if it ever surfaces.

### Codebase anchors

- `fix_file`: `src/cli/fix/fields.rs:36`.
- `fix_priority_file`: `src/cli/fix/fields.rs` (added in iter 165).
- `split_frontmatter`: `src/engine/document.rs`.
- `default_for_field`: `src/cli/fix/fields.rs:90`.
