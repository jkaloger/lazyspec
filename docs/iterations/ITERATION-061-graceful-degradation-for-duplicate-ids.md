---
title: Graceful degradation for duplicate IDs
type: iteration
status: draft
author: agent
date: 2026-03-13
tags: []
related:
- implements: docs/stories/STORY-061-graceful-degradation-for-duplicate-ids.md
---


## Changes

1. **Return ambiguity error from `resolve_shorthand`** — `src/engine/store.rs`
   - ACs: 1, 9
   - Change `resolve_shorthand` return type from `Option<&DocMeta>` to `Result<&DocMeta, ResolveError>` where `ResolveError` is a new enum with `NotFound(String)` and `Ambiguous { id: String, matches: Vec<PathBuf> }` variants.
   - The unqualified branch (line 337) currently uses `.find()` which returns the first match. Replace with `.filter()` + `.collect()` and check the count: 0 returns `NotFound`, 1 returns the match, 2+ returns `Ambiguous` with all matching paths.
   - Apply the same logic to the qualified branch (line 307).
   - Add a `resolve_shorthand_or_path` helper that first tries exact path lookup via `store.get()`, falling back to `resolve_shorthand`. This supports AC 4 (full-path still works).
   - Update all call sites: `src/cli/show.rs`, `src/cli/context.rs`, `src/tui/app.rs`, and any others found via grep for `resolve_shorthand`.
   - Verify: `cargo test` passes, `resolve_shorthand("RFC-020")` with two matches returns `Ambiguous`.

2. **CLI show: surface ambiguity error** — `src/cli/show.rs`
   - ACs: 2, 3, 4
   - In `run()`: match on the new `ResolveError::Ambiguous` variant. Print a message like "Ambiguous ID 'RFC-020' matches multiple documents:" followed by each path, then "Specify the full path to show a specific document."
   - In `run_json()`: on `Ambiguous`, return a JSON object `{ "error": "ambiguous_id", "id": "...", "ambiguous_matches": ["path1", "path2"] }`.
   - Both functions should use `resolve_shorthand_or_path` so full-path input bypasses ambiguity.
   - Verify: manually test with two `RFC-020-*.md` files. Confirm `lazyspec show RFC-020` prints the error, `--json` returns the structured error, and `lazyspec show docs/rfcs/RFC-020-foo.md` works.

3. **list and context: no changes needed (confirm)** — `src/cli/list.rs`, `src/cli/context.rs`
   - ACs: 5, 6, 7
   - `list` iterates `store.list()` which returns all docs from the HashMap; duplicates already appear. No code change needed, just a test confirming both show up.
   - `context` uses `resolve_shorthand` for its entry point. Update the call to handle the new error type. If ambiguous, print the same error as `show`.
   - Verify: `lazyspec list --json` with two RFC-020 docs returns both in the array.

4. **TUI: flag duplicate IDs with warning indicator** — `src/tui/app.rs`, `src/tui/ui.rs`
   - AC: 8
   - In `App::rebuild_doc_tree` (around line 440 of `app.rs`), after building the tree, compute a set of duplicate IDs by counting occurrences of each `doc.id` across all non-child docs. Store as `duplicate_ids: HashSet<String>` on `App`.
   - Add a `has_duplicate_id: bool` field to `DocListNode`.
   - In `doc_row_for_node` (`ui.rs` line 253), when `node.has_duplicate_id` is true, prepend a warning marker (e.g. `⚠` in yellow) to the ID cell.
   - Verify: create two docs with the same ID prefix, open TUI, confirm both appear with the warning marker.

## Test Plan

- `tests/store_test.rs` — new test `resolve_shorthand_ambiguous`: create two `RFC-020-*.md` docs via `TestFixture`, call `resolve_shorthand("RFC-020")`, assert it returns `Ambiguous` with both paths. (AC 1)
- `tests/store_test.rs` — new test `resolve_shorthand_unique_still_works`: single doc, assert `resolve_shorthand` returns it. (AC 9)
- `tests/cli_query_test.rs` — new test `show_ambiguous_human`: two RFC-020 docs, run `lazyspec show RFC-020`, assert stderr/stdout contains both paths. (AC 2)
- `tests/cli_query_test.rs` or `tests/cli_json_test.rs` — new test `show_ambiguous_json`: run `lazyspec show RFC-020 --json`, parse output, assert `ambiguous_matches` array contains both paths. (AC 3)
- `tests/cli_query_test.rs` — new test `show_full_path_when_ambiguous`: two RFC-020 docs, run `lazyspec show docs/rfcs/RFC-020-foo.md`, assert success. (AC 4)
- `tests/cli_query_test.rs` — new test `list_includes_duplicates`: two RFC-020 docs, run `lazyspec list --json`, parse output, assert both present. (ACs 5, 6)
- `tests/cli_context_test.rs` — new test `context_with_duplicates`: two RFC-020 docs, run `lazyspec context RFC-020`, assert ambiguity error rather than crash. (AC 7)
- `tests/tui_tree_test.rs` — new test `duplicate_id_warning_flag`: build an App with two RFC-020 docs, assert `has_duplicate_id` is true on both `DocListNode` entries. (AC 8)

## Notes

The `list` command requires no logic changes since `Store::list` iterates all values in the `docs` HashMap. Both duplicates are already separate entries keyed by their distinct file paths. The key change is in `resolve_shorthand` where `HashMap::values().find()` is nondeterministic with duplicates.
