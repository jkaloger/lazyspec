---
title: Child document discovery
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-040-child-document-discovery.md
---



## Changes

### Task 1: Add children/parent_of indexes to Store

**ACs addressed:** AC2

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**

Add two new fields to the `Store` struct:

```rust
pub(crate) children: HashMap<PathBuf, Vec<PathBuf>>,   // parent path -> child paths
pub(crate) parent_of: HashMap<PathBuf, PathBuf>,        // child path -> parent path
```

Initialize both as empty in `Store::load()` return value (they get populated in Task 2). Add accessor methods:

- `pub fn children_of(&self, path: &Path) -> &[PathBuf]` -- returns children for a parent, or empty slice
- `pub fn parent_of(&self, path: &Path) -> Option<&PathBuf>` -- returns parent for a child

**How to verify:**
`cargo build` compiles. Existing tests still pass with `cargo test`.

---

### Task 2: Discover child .md files during Store::load()

**ACs addressed:** AC1, AC2, AC8

**Files:**
- Modify: `src/engine/store.rs` (Store::load)

**What to implement:**

In the existing directory entry loop within `Store::load()`, when a subdirectory with `index.md` is found (the existing subfolder path, around line 38), add a second scan of the subdirectory. For each entry in the subdirectory:

1. Skip `index.md` (already handled as the parent)
2. Skip entries that are directories (AC8: no recursive nesting)
3. For each remaining `.md` file: read and parse with `DocMeta::parse()`, set its `path` to the relative path (e.g. `docs/rfcs/RFC-014-nested/threat-model.md`), insert into `docs`
4. Populate `children` map: key = parent's path, value = vec of child paths
5. Populate `parent_of` map: key = child path, value = parent path

Build forward/reverse links for child documents using their `related` fields (existing link-building loop already iterates `self.docs`, so children are included automatically).

**How to verify:**
`cargo test` -- new tests in Task 5 verify discovery.

---

### Task 3: Synthesise virtual parents

**ACs addressed:** AC3, AC4

**Files:**
- Modify: `src/engine/store.rs` (Store::load)
- Modify: `src/engine/document.rs` (DocMeta)

**What to implement:**

In `Store::load()`, when a subdirectory does NOT contain `index.md` but DOES contain at least one `.md` file:

1. Parse all `.md` files as child documents (same as Task 2)
2. Synthesise a virtual `DocMeta` for the parent:
   - `path`: the folder path with a synthetic suffix, e.g. `docs/rfcs/RFC-014-nested/.virtual` (must not collide with real files)
   - `title`: derived from folder name. Strip the type prefix and number, replace hyphens with spaces, title-case. E.g. `RFC-014-nested-child-support` becomes "Nested child support"
   - `doc_type`: inferred from folder name prefix using existing `DocType::new()`
   - `status`: `Status::Draft` initially. After all children are parsed, check if every child has `Status::Accepted` -- if so, set to `Status::Accepted`
   - `author`: empty string
   - `date`: today's date (or earliest child date)
   - `tags`, `related`: empty
   - `validate_ignore`: false
3. Add a `pub virtual_doc: bool` field to `DocMeta` (default `false`). Set to `true` for synthesised parents. This lets other code distinguish virtual from real docs.
4. Insert virtual parent into `docs`, populate `children`/`parent_of` indexes

The virtual parent is never written to disk (AC4) since it only exists in memory.

**How to verify:**
`cargo test` -- new tests in Task 5.

---

### Task 4: Qualified shorthand resolution

**ACs addressed:** AC5, AC6

**Files:**
- Modify: `src/engine/store.rs` (Store::resolve_shorthand)

**What to implement:**

Update `resolve_shorthand()` to handle the `PARENT/CHILD` pattern:

1. Check if `id` contains a `/` separator
2. If yes: split into `(parent_prefix, child_stem)`. Resolve `parent_prefix` against parent documents (existing logic). Then search children of that parent for a file whose stem starts with `child_stem`.
3. If no `/`: use existing logic but skip documents that have a parent (i.e. skip entries present in `parent_of`). This implements AC6 -- unqualified shorthand never resolves to children.

```rust
pub fn resolve_shorthand(&self, id: &str) -> Option<&DocMeta> {
    if let Some((parent_id, child_stem)) = id.split_once('/') {
        // Qualified: find parent, then find child within it
        let parent = self.resolve_shorthand(parent_id)?;
        let child_paths = self.children.get(&parent.path)?;
        child_paths.iter().find_map(|cp| {
            let doc = self.docs.get(cp)?;
            let stem = cp.file_stem()?.to_str()?;
            stem.starts_with(child_stem).then_some(doc)
        })
    } else {
        // Unqualified: existing logic, but exclude children
        self.docs.values().find(|d| {
            if self.parent_of.contains_key(&d.path) { return false; }
            // ... existing name matching logic ...
        })
    }
}
```

**How to verify:**
`cargo test` -- new tests in Task 5.

---

### Task 5: Tests

**ACs addressed:** AC1-AC8 (all)

**Files:**
- Modify: `tests/store_test.rs`
- Modify: `tests/common/mod.rs`

**What to implement:**

Add a helper to `tests/common/mod.rs`:

- `write_child_doc(folder_rel_path, filename, content)` -- writes a `.md` file inside a subfolder document

Add tests to `tests/store_test.rs`:

1. **`store_discovers_child_md_files()`** (AC1): Create `RFC-003-multi/index.md` and `RFC-003-multi/appendix.md`. Load store. Assert both are in `docs`. Assert `appendix.md` has its own parsed frontmatter.

2. **`store_tracks_parent_child_relationship()`** (AC2): Same fixture. Assert `store.children_of(parent_path)` contains the child path. Assert `store.parent_of(child_path)` equals the parent path.

3. **`store_synthesises_virtual_parent()`** (AC3): Create folder `RFC-004-virtual/` with `notes.md` and `design.md` but no `index.md`. Load store. Assert a virtual parent exists with title derived from folder name, `virtual_doc == true`, and correct type/status.

4. **`store_virtual_parent_accepted_when_all_children_accepted()`** (AC3): Same as above but both children have `status: accepted`. Assert virtual parent status is `Accepted`.

5. **`store_virtual_parent_not_on_disk()`** (AC4): After loading, assert no `index.md` file exists in the virtual parent's folder on disk.

6. **`store_qualified_shorthand_resolves_child()`** (AC5): Assert `resolve_shorthand("RFC-003/appendix")` returns the child doc.

7. **`store_unqualified_shorthand_skips_children()`** (AC6): Create two folders each with `notes.md`. Assert `resolve_shorthand("notes")` returns `None`.

8. **`store_child_relationships_resolve()`** (AC7): Create a child doc with `related: [implements: "docs/stories/STORY-001.md"]`. Assert forward/reverse links include the child.

9. **`store_ignores_nested_subdirectories()`** (AC8): Create `RFC-003-multi/deep/hidden.md`. Assert `hidden.md` is not in the store.

**How to verify:**
`cargo test` -- all new tests pass. All existing tests still pass.

## Test Plan

All tests are unit/integration tests in `tests/store_test.rs` using the existing `TestFixture` pattern. Each test creates a temporary directory, writes fixture files, loads a Store, and asserts expectations. Tests are isolated (own temp dir), deterministic, and fast.

| Test | AC | What it verifies |
|------|----|-----------------|
| `store_discovers_child_md_files` | AC1 | Child .md files parsed with own frontmatter |
| `store_tracks_parent_child_relationship` | AC2 | children_of/parent_of indexes populated |
| `store_synthesises_virtual_parent` | AC3 | Virtual parent created with derived metadata |
| `store_virtual_parent_accepted_when_all_children_accepted` | AC3 | Status promotion to accepted |
| `store_virtual_parent_not_on_disk` | AC4 | No index.md written for virtual parents |
| `store_qualified_shorthand_resolves_child` | AC5 | `RFC-003/appendix` resolves |
| `store_unqualified_shorthand_skips_children` | AC6 | bare `notes` returns None |
| `store_child_relationships_resolve` | AC7 | Child's related links in forward/reverse maps |
| `store_ignores_nested_subdirectories` | AC8 | Subdirs within doc folders skipped |

## Notes

- The `virtual_doc` field on DocMeta is a simple boolean. If virtual parents grow more complex later (e.g. editable status), this could become an enum, but a bool is sufficient for now.
- The `.virtual` path suffix for virtual parents is a convention to avoid colliding with real files. An alternative would be to not store virtual parents in the `docs` HashMap at all and use a separate map, but having them in `docs` means all existing code (list, search, filter) works without changes.
- Existing tests for subfolder `index.md` discovery remain unchanged. The new child discovery is additive.
