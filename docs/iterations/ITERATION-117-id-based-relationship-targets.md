---
title: ID-based relationship targets
type: iteration
status: accepted
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-092
---




## Changes

### Task 1: Change `link` command to write document IDs instead of paths

**ACs addressed:** id-based-storage, link-writes-id

**Files:**
- Modify: `src/cli/link.rs`
- Modify: `src/cli/resolve.rs` (add `resolve_to_id` helper)
- Test: `tests/cli_link_test.rs`

**What to implement:**

In `src/cli/link.rs`, the `link` function currently calls `resolve_to_path(store, to)` and writes the resulting path string into frontmatter. Change this to resolve the target to its document ID instead.

Add a `resolve_to_id(store: &Store, input: &str) -> Result<String>` function in `src/cli/resolve.rs` that resolves a shorthand or path to the document's ID (e.g. `RFC-001`). Use `store.resolve_shorthand()` for shorthand input, or look up the doc by path and return its `id` field.

Update `link()` to call `resolve_to_id` for the `to` argument and write the ID string into the YAML.

Similarly update `unlink()` in the same file — it matches against the target string to remove entries, so it needs to resolve and compare IDs.

**How to verify:**
```
cargo test cli_link
```

---

### Task 2: Resolve IDs to paths when building the link graph

**ACs addressed:** id-resolution-at-load

**Files:**
- Modify: `src/engine/store/links.rs`
- Modify: `src/engine/store.rs` (expose an id→path lookup if not already available)
- Test: `tests/store_test.rs` (or existing link graph tests)

**What to implement:**

In `build_links` (lines 67–91) and `rebuild_links` (lines 48–65), the target is currently converted via `PathBuf::from(&rel.target)`. This assumes the target is a path.

Change this to resolve the target string as a document ID first. Build a `HashMap<String, PathBuf>` mapping IDs to paths from `store.docs` (iterate docs, collect `(doc.id.clone(), doc.path.clone())`). Pass this map into `build_links`. For each `rel.target`, look up the ID in the map to get the `PathBuf`. If the ID doesn't resolve, skip it (validation will catch it separately).

The forward/reverse link maps remain keyed by `PathBuf` — only the resolution step changes.

**How to verify:**
```
cargo test store
cargo test links
```

---

### Task 3: Update `BrokenLinkRule` to validate IDs

**ACs addressed:** broken-link-validation

**Files:**
- Modify: `src/engine/validation.rs`
- Test: `tests/validation_test.rs`

**What to implement:**

In `BrokenLinkRule::check` (lines 231–313), the checker currently does `PathBuf::from(&rel.target)` and checks `store.docs.contains_key(&target)`.

Change this to build the same ID→path map used in Task 2 (or accept it as a parameter / use a shared helper). Look up `rel.target` as an ID. If not found, emit a `BrokenLink` issue. The `ValidationIssue::BrokenLink` variant currently stores `target: PathBuf` — change this to `target: String` to hold the unresolved ID, since that's what the user needs to see in the error message.

Also update `ParentLinkRule` (lines 315–388) which does the same `PathBuf::from(&r.target)` resolution pattern.

**How to verify:**
```
cargo test validation
```

---

### Task 4: Update JSON serialization to output IDs

**ACs addressed:** json-output-uses-ids

**Files:**
- Modify: `src/cli/json.rs`
- Test: `tests/cli_json_test.rs`

**What to implement:**

In `doc_to_json` (lines 5–22), the `related` field currently outputs `r.target` directly. Since `Relation.target` will now store IDs (from Task 1) and parse IDs (from existing documents after migration), this should already output IDs without additional changes.

However, verify that all JSON output paths (`status`, `show`, `list`, `search`, `context`) produce the correct format. Update test assertions in `tests/cli_json_test.rs` from path assertions (e.g. `"docs/rfcs/RFC-001-auth.md"`) to ID assertions (e.g. `"RFC-001"`).

**How to verify:**
```
cargo test cli_json
```

---

### Task 5: Add migration fix to rewrite path targets to IDs

**ACs addressed:** migration

**Files:**
- Modify: `src/cli/fix.rs` (add new fix action)
- Modify: `src/cli/fix/fields.rs` or create `src/cli/fix/relations.rs`
- Modify: `src/cli/fix/renumber.rs` (`cascade_references` also needs updating)
- Test: `tests/cli_fix_test.rs`

**What to implement:**

Add a new fix action that scans all documents' frontmatter `related` sequences. For each target value that looks like a path (contains `/` or ends in `.md`), resolve it to the corresponding document ID using the store's docs map, and rewrite the YAML value in-place.

Use `rewrite_frontmatter` from `src/engine/document.rs` to do the in-place update. Build a path→ID lookup from `store.docs`. For each relation entry in the YAML sequence, check if the value is a path, look up the ID, and replace.

Also update `cascade_references` in `src/cli/fix/renumber.rs` (lines 386–480) — when a document is renumbered, it should now cascade the new ID (not the new path) into referencing documents' frontmatter.

Add the fix to the `FixOutput` struct and wire it into `run`/`run_json`/`run_human`.

**How to verify:**
```
cargo test cli_fix
```

---

### Task 6: Migrate all existing documents

**ACs addressed:** migration

**Files:**
- Modify: all `docs/**/*.md` files with `related` entries

**What to implement:**

After Tasks 1–5 are implemented, run `cargo run -- fix` to migrate all existing documents. This should rewrite every path-based relationship target to its document ID.

Verify with `cargo run -- validate --json` that no broken links exist after migration.

This task is a one-time operation, not a code change.

**How to verify:**
```
cargo run -- validate --json
# Grep for any remaining path-like targets in frontmatter
grep -r "implements: docs/" docs/ && echo "FAIL: path targets remain" || echo "OK"
grep -r "related-to: docs/" docs/ && echo "FAIL: path targets remain" || echo "OK"
grep -r "supersedes: docs/" docs/ && echo "FAIL: path targets remain" || echo "OK"
grep -r "blocks: docs/" docs/ && echo "FAIL: path targets remain" || echo "OK"
```

## Test Plan

### Test 1: link writes document ID (AC: link-writes-id)
Create two documents, run `lazyspec link` between them, parse the source frontmatter, assert `related[0].target` is the target's document ID (e.g. `"RFC-001"`), not a path. **Isolated, fast, behavioral.**

### Test 2: unlink matches by document ID (AC: link-writes-id)
Create two documents, link them with IDs, run `lazyspec unlink`, assert the relationship is removed. **Isolated, fast, behavioral.**

### Test 3: parse_relation accepts ID strings (AC: id-based-storage)
Parse YAML `- implements: RFC-001` through `parse_relation`, assert `Relation { rel_type: Implements, target: "RFC-001" }`. **Isolated, fast, unit-level.**

### Test 4: build_links resolves IDs to paths (AC: id-resolution-at-load)
Load a store with documents where frontmatter has ID-based targets. Assert `forward_links` and `reverse_links` contain the correct `PathBuf` keys. **Isolated, fast, structure-insensitive.**

### Test 5: broken link validation reports unresolved IDs (AC: broken-link-validation)
Create a document with `- implements: RFC-999`. Run validation, assert a `BrokenLink` issue is emitted with the unresolved ID. **Isolated, fast, specific.**

### Test 6: JSON output shows IDs (AC: json-output-uses-ids)
Load a store with linked documents, serialize via `doc_to_json`, assert `related[0]["target"]` is an ID string. **Isolated, fast, behavioral.**

### Test 7: fix migrates path targets to IDs (AC: migration)
Create documents with path-based targets in frontmatter. Run the fix action. Re-parse frontmatter and assert targets are now IDs. **Isolated, fast, predictive.**

### Test 8: cascade_references writes IDs on renumber (AC: migration)
Renumber a document that other documents reference. Assert the cascaded update writes the new ID, not a new path. **Isolated, fast, behavioral.**

## Notes

- `Relation.target` semantics change from "relative path" to "document ID" — this is a breaking change to the on-disk format but the migration in Task 6 handles the transition.
- The internal link graph (`forward_links`, `reverse_links`) remains keyed by `PathBuf`. Only the frontmatter storage and resolution boundary changes.
- `cascade_references` in `fix/renumber.rs` is the only existing code that rewrites relation targets — it must be updated to cascade IDs instead of paths.
