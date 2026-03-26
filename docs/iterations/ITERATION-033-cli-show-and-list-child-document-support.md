---
title: CLI show and list child document support
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-041
---




## Changes

### Task 1: Add parent-child fields to JSON serialization

**ACs addressed:** AC7

**Files:**
- Modify: `src/cli/json.rs`

**What to implement:**

Add a new function `doc_to_json_with_family` that extends `doc_to_json` with parent-child metadata. It takes the `Store` reference alongside the `DocMeta`:

- If the document has children (`store.children_of(&doc.path)` is non-empty), add a `"children"` array to the JSON. Each entry should be an object with `"path"` and `"title"` (look up each child path in the store via `store.get()`).
- If the document has a parent (`store.parent_of(&doc.path)` returns `Some`), add a `"parent"` object with `"path"` and `"title"`.
- Include `"virtual_doc": true` when `doc.virtual_doc` is true. Omit or set false otherwise.

Keep the existing `doc_to_json` unchanged for callers that don't need family info. The new function builds on it:

```rust
pub fn doc_to_json_with_family(doc: &DocMeta, store: &Store) -> Value {
    let mut json = doc_to_json(doc);
    // add children array if non-empty
    // add parent object if present
    // add virtual_doc flag
    json
}
```

**How to verify:**
`cargo test` -- new unit tests in Task 4 will cover JSON output.

---

### Task 2: Update `show` command to display children and parent

**ACs addressed:** AC1, AC2

**Files:**
- Modify: `src/cli/show.rs`

**What to implement:**

Update `run()` (human output):
- After printing the body, check `store.children_of(&doc.path)`. If non-empty, print a "Children" section: a blank line, then `dim("Children:")`, then for each child path, look up the doc via `store.get()` and print `  - {title}  ({qualified_shorthand})`. The qualified shorthand is the parent's folder name prefix + "/" + child file stem. Derive it from the child's path: take the parent directory name and the file stem.
- Check `store.parent_of(&doc.path)`. If `Some`, print a "Parent" line after the metadata header: `dim("Parent:") bold(parent_title) dim(parent_path)`.

Update `run_json()`:
- Replace `doc_to_json(doc)` with `doc_to_json_with_family(doc, store)` from Task 1. The body field is still added after.

**How to verify:**
`cargo test` -- integration tests in Task 4. Manual: `cargo run -- show RFC-003` on a fixture with children should display the Children section.

---

### Task 3: Update `list` command to include parent-child metadata in JSON

**ACs addressed:** AC4, AC7

**Files:**
- Modify: `src/cli/list.rs`

**What to implement:**

The `list` command already returns all documents (parents and children) from `store.list()`. No filtering changes needed for AC4.

For human output: after each `doc_card` line, if the document is a child, append a dim indicator like `  (child of {parent_title})`. Check `store.parent_of(&doc.path)` for each doc.

For JSON output: replace `doc_to_json(d)` with `doc_to_json_with_family(d, store)` so that each document in the array includes its parent/children metadata.

Update `run()` and `run_json()` signatures to accept `&Store` (they currently receive `&Store` already via the `store` parameter -- verify the call in `main.rs` passes it correctly for JSON).

**How to verify:**
`cargo test` -- integration tests in Task 4. Manual: `cargo run -- list --json` should include `children`/`parent` fields.

---

### Task 4: Integration tests for CLI child document output

**ACs addressed:** AC1, AC2, AC4, AC7

**Files:**
- Create: `tests/cli_child_test.rs`
- Modify: `tests/common/mod.rs` (if a new helper is needed)

**What to implement:**

Write integration tests that use `TestFixture` to set up a parent with children, then call the CLI functions directly (same pattern as existing tests call `store` methods). Since the CLI modules are public, import and call them.

Planned tests:

1. **`show_parent_lists_children_human`** (AC1): Create a parent RFC with two children. Call `show::run()` capturing stdout. Assert output contains "Children" section with both child titles.

2. **`show_child_indicates_parent_human`** (AC2): Create a parent with a child. Call `show::run()` for the child (via qualified shorthand). Assert output contains "Parent" with the parent's title.

3. **`show_parent_json_includes_children`** (AC1, AC7): Call `show::run_json()` for the parent. Parse the JSON. Assert `children` array has correct entries with path and title.

4. **`show_child_json_includes_parent`** (AC2, AC7): Call `show::run_json()` for a child. Parse the JSON. Assert `parent` object has correct path and title.

5. **`list_includes_child_documents`** (AC4): Create parent + children. Call `list::run_json()`. Parse the JSON array. Assert children appear in the list.

6. **`list_json_includes_family_metadata`** (AC4, AC7): Same setup. Assert each parent in the JSON has `children` and each child has `parent`.

> [!NOTE]
> Tests 1 and 2 require capturing stdout. Use a test helper that redirects output, or refactor `show::run` to return a String instead of printing directly. If refactoring is needed, keep it minimal -- add a `format()` function that returns String, and have `run()` call `println!("{}", format(...))`.

Tradeoffs: these are integration-level tests (not unit tests) which sacrifices Fast for Predictive. The CLI output format is the contract that agents and users depend on, so testing at this level is worth the cost.

## Test Plan

| Test | AC | Property focus | Approach |
|------|-----|---------------|----------|
| show_parent_lists_children_human | AC1 | Behavioral, Predictive | Assert "Children" section present with child titles |
| show_child_indicates_parent_human | AC2 | Behavioral, Predictive | Assert "Parent" line present with parent title |
| show_parent_json_includes_children | AC1, AC7 | Specific, Deterministic | Parse JSON, check `children` array structure |
| show_child_json_includes_parent | AC2, AC7 | Specific, Deterministic | Parse JSON, check `parent` object structure |
| list_includes_child_documents | AC4 | Behavioral | Assert children appear in list output |
| list_json_includes_family_metadata | AC4, AC7 | Specific, Deterministic | Parse JSON, check family fields on each doc |

## Notes

- The `show::run()` function currently prints directly to stdout. Task 2 may need a small refactor to make it testable (extract formatting to a function that returns String). This is a minimal change scoped to testability.
- `list` already returns children since `store.list()` iterates all docs. AC4 is partially satisfied by existing behavior; the iteration adds the metadata and indicators.
- The `doc_to_json_with_family` function in Task 1 is the foundation that Tasks 2 and 3 depend on. Build order: Task 1 -> Task 2 -> Task 3 -> Task 4.
