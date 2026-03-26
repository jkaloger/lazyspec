---
title: Subfolder Document Discovery
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: STORY-031
---




## Changes

### Task 1: Extend Store::load() to discover subfolder documents

**ACs addressed:** AC-1, AC-2, AC-3

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**

In `Store::load()`, after the existing `fs::read_dir` loop body that filters for `.md` files, add handling for directory entries:

1. If the entry is a directory, check if it contains an `index.md` file
2. If `index.md` exists, read and parse it as a document
3. Set the document's relative path to `<dir>/<folder_name>/index.md` (e.g. `docs/rfcs/RFC-010-something/index.md`)
4. If `index.md` does not exist, skip the directory silently

The current code skips non-`.md` entries via the extension check. The change should check `entry.file_type()?.is_dir()` for directory entries and probe for `index.md` inside them.

**How to verify:**
`cargo test` -- covered by Task 3 tests.

---

### Task 2: Update shorthand resolution for folder-based documents

**ACs addressed:** AC-4

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**

`resolve_shorthand()` currently matches against `file_name()` (e.g. `RFC-001-event-sourcing.md`). For folder-based documents, `file_name()` returns `index.md`, which won't match any shorthand.

Change `resolve_shorthand()` to also check the parent directory name when the filename is `index.md`. Specifically: if `d.path.file_name() == "index.md"`, match against the parent directory's name instead. This way `RFC-010` matches `docs/rfcs/RFC-010-subfolder-document-support/index.md` via the folder name `RFC-010-subfolder-document-support`.

**How to verify:**
`cargo test` -- covered by Task 3 tests.

---

### Task 3: Add tests for subfolder document discovery

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Modify: `tests/store_test.rs`
- Modify: `tests/common/mod.rs` (add a helper for writing folder-based docs)

**What to implement:**

Add a `write_subfolder_doc` helper to `TestFixture` that creates `<dir>/<folder_name>/index.md` with the given content (ensuring parent dirs are created).

Add the following tests to `store_test.rs`:

1. `store_discovers_subfolder_index_md` -- write a folder-based RFC, load the store, assert it appears in `all_docs()` with path ending in `/index.md`
2. `store_discovers_both_flat_and_subfolder_docs` -- write one flat RFC and one folder-based RFC, assert `all_docs().len() == 2` (plus any other fixture docs)
3. `store_ignores_subfolder_without_index_md` -- create a subfolder with no `index.md`, assert it doesn't appear
4. `store_resolves_shorthand_for_subfolder_doc` -- write a folder-based RFC, resolve by prefix (e.g. `RFC-002`), assert it resolves
5. `store_subfolder_doc_relationships_resolve` -- write a folder-based RFC and a flat story that implements it via the full path, assert `related_to()` returns the link
6. `store_search_finds_subfolder_doc` -- write a folder-based RFC with a distinctive title, search for it, assert it appears

**How to verify:**
`cargo test`

## Test Plan

All tests are unit-level tests in `tests/store_test.rs` using the existing `TestFixture` infrastructure (temp directories). They are fast, isolated, deterministic, and behavioral (testing observable outcomes of store operations, not internal implementation).

| Test | AC | What it verifies |
|------|-----|-----------------|
| `store_discovers_subfolder_index_md` | AC-1 | Subfolder with index.md is parsed as a document |
| `store_discovers_both_flat_and_subfolder_docs` | AC-2 | Flat and folder-based docs coexist |
| `store_ignores_subfolder_without_index_md` | AC-3 | Missing index.md = silently ignored |
| `store_resolves_shorthand_for_subfolder_doc` | AC-4 | Shorthand matches folder name |
| `store_subfolder_doc_relationships_resolve` | AC-5 | Relationships using full path resolve |
| `store_search_finds_subfolder_doc` | AC-6 | Search returns folder-based docs |

## Notes

The change is concentrated in `store.rs`. Other commands (show, status, validate, search, list) operate on the `Store` abstraction, so they get subfolder support for free once discovery and shorthand resolution work correctly. No changes needed to CLI commands, validation, or TUI code.
