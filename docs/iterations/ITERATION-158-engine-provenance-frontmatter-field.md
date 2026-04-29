---
title: Engine provenance frontmatter field
type: iteration
status: accepted
author: agent
date: 2026-04-29
tags: []
related:
- implements: STORY-110
---



## Changes

1. **`DocMeta` field** — `src/engine/document.rs:188`. Add `pub provenance: Vec<String>` after `tags`. ACs: round-trip, default-empty, all-types.

2. **`RawFrontmatter` field** — `src/engine/document.rs:202`. Add `#[serde(default)] provenance: Vec<String>` after `tags`. Map in `parse` (line 284). Missing field → empty via `#[serde(default)]`. ACs: missing-defaults, empty-list-loads.

3. **Empty-string validator in `parse`** — `src/engine/document.rs:273` `DocMeta::parse`. After serde decode, iterate `raw.provenance`, bail on any empty string. Error: `"provenance entry must not be empty"` with title or path context if available. ACs: empty-rejects.

4. **Update all `DocMeta {}` literals** — add `provenance: vec![]`. Sites:
   - `src/engine/document.rs:319` (test helper `make_doc`)
   - `src/engine/store/loader.rs:141`
   - `src/engine/store_dispatch.rs:187,226,304,1341`
   - `src/engine/issue_cache.rs:172,259,328`
   - `src/engine/issue_body.rs:75,179`
   - `src/engine/git_ref_store.rs:118`
   - `src/tui/state/app.rs:1744,1757,1904`
   Verify: `cargo check` clean.

5. **JSON output** — `src/cli/json.rs:5` `doc_to_json`. Add `"provenance": doc.provenance` after `"tags"`. AC: round-trip surfaced via existing `--json` paths.

6. **Frontmatter writer** — no change. `rewrite_frontmatter` (`src/engine/document.rs:218`) reads → `serde_yaml::Value` → mutate → write. `provenance` is preserved opaquely. Verify with round-trip test.

7. **Templates** — no change. KISS: don't emit `provenance: []` in default templates; absent field loads as empty list anyway.

## Test Plan

Unit tests in `src/engine/document.rs` `mod tests`:

- `provenance_loads_in_order` — fixture with 3 entries → parse → assert `Vec` matches order. AC1.
- `provenance_missing_defaults_empty` — fixture without field → parse → empty vec. AC3.
- `provenance_empty_list_loads` — `provenance: []` → parse → empty vec. AC5.
- `provenance_empty_string_rejected` — `provenance: ["", "ok"]` → parse → `Err`, message contains "empty". AC4.

Integration test new file `tests/provenance_roundtrip.rs` (or extend existing frontmatter test):

- `provenance_round_trips_via_rewriter` — load doc with provenance, run `rewrite_frontmatter` no-op closure, reload, assert identical. AC2.
- `provenance_empty_round_trips` — load doc without field, rewrite, reload, still empty. AC6.
- `provenance_works_for_each_doc_type` — parametrized over `rfc/story/iteration/audit/adr/spec`, fixture per type, parse → assert. AC7.

All tests use `tempfile::TempDir` or in-memory fixture strings. No mocks. Real `DocMeta::parse` and `rewrite_frontmatter`.

## Notes

- `rewrite_frontmatter` uses `serde_yaml::Value` so unknown fields survive automatically — no writer code change needed.
- `RawFrontmatter` does not include `provenance` in its `Serialize` path because it isn't `Serialize` — only deserialised; mutations use `serde_yaml::Value`. Confirms writer-preservation.
- 13 `DocMeta { ... }` literals exist. Adding default field is mechanical; `Default::default()`-style init not used here.
- Validation message format mirrors existing `parse` errors (anyhow `bail!`).
- No engine module split required (KISS, dictum: don't invent module for one struct field).

