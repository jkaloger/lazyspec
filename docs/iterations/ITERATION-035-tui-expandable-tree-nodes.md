---
title: TUI expandable tree nodes
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-042
---




## Changes

### Task 1: Add expanded state tracking to App

**ACs addressed:** AC1, AC2, AC3

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a `HashSet<PathBuf>` field `expanded_parents` to the `App` struct to track which parent documents are currently expanded. Initialize it as empty (all parents collapsed by default).

Add methods:
- `toggle_expanded(&mut self, path: &Path)` — inserts or removes the path from the set
- `is_expanded(&self, path: &Path) -> bool` — checks membership

### Task 2: Build flattened tree list for document view

**ACs addressed:** AC1, AC2, AC3, AC4

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Replace or wrap the flat `docs_for_current_type()` result with a tree-aware list. Add a new struct `DocListNode` (in `app.rs` or a shared location) with fields: `path: PathBuf`, `title: String`, `doc_type: DocType`, `status: Status`, `depth: usize`, `is_parent: bool`, `is_virtual: bool`.

Add a method `build_doc_tree(&self) -> Vec<DocListNode>` that:
1. Gets the flat doc list for the current type from `store.list()`
2. Separates documents into parents (those with children via `store.children_of()`) and standalone docs
3. For each parent: emits a `DocListNode` at depth 0 with `is_parent: true`
4. If the parent is in `expanded_parents`: emits each child as a `DocListNode` at depth 1
5. Standalone documents (not a parent, not a child) emit at depth 0 as before
6. Children that are not under an expanded parent are hidden (not emitted)

Documents that are children should only appear nested under their parent, not as top-level items. Use `store.parent_of()` to detect children and skip them in the top-level pass.

Cache the result in a field `doc_tree: Vec<DocListNode>` and rebuild when the type changes or expand/collapse is toggled.

### Task 3: Render tree nodes with connectors in the document list

**ACs addressed:** AC1, AC2, AC5

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Update `draw_doc_list()` to use `app.doc_tree` instead of the flat list. For each `DocListNode`:

- If `depth == 0` and `is_parent`: render an expand/collapse indicator before the title. Use `▶` for collapsed, `▼` for expanded (checking `app.is_expanded()`).
- If `depth > 0`: render with indentation and ASCII connectors, reusing the pattern from `draw_graph()` (lines 893-904). Use `"   ".repeat(depth - 1)` for leading space and `├─` / `└─` connectors based on whether it's the last sibling.
- If `is_virtual`: append a visual marker like `(virtual)` in a dimmed style to distinguish virtual parents from real documents.
- Preserve the existing status coloring and tag rendering for all nodes.

### Task 4: Keybindings for expand/collapse

**ACs addressed:** AC2, AC3, AC4

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

In the key event handler for the document list view, add expand/collapse keybindings:

- Right arrow or `l`: if the selected node is a collapsed parent, expand it (add to `expanded_parents`, rebuild `doc_tree`)
- Left arrow or `h`: if the selected node is an expanded parent, collapse it (remove from `expanded_parents`, rebuild `doc_tree`). If the selected node is a child, jump selection to its parent and collapse.
- Enter on a parent should still show preview (existing behavior), not toggle expand.

Up/down navigation (`j`/`k` or arrows) continues to work on the flat `doc_tree` list as normal. When collapsing, if the current selection index points to a now-hidden child, move selection to the parent.

### Task 5: Preview pane for child documents

**ACs addressed:** AC4

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/app.rs`

**What to implement:**

The selected doc path for preview should come from `doc_tree[selected_doc].path` rather than the flat list. Verify that the preview pane (`draw_preview()`) works correctly when a child document is selected — it should show the child's content, not the parent's. The path lookup into `store.docs` should already work since children are full documents in the store.

## Test Plan

### T1: Tree building with expanded/collapsed state (unit)

Create a `Store` with a parent document that has two children. Call `build_doc_tree()` with the parent collapsed — assert only the parent appears (depth 0, `is_parent: true`). Toggle expanded, rebuild — assert parent at depth 0 followed by two children at depth 1. Toggle collapsed again — assert children disappear.

Tradeoffs: Behavioral over structure-insensitive. Tests the tree-building logic directly since rendering is hard to unit test in a TUI.

### T2: Standalone documents unaffected (unit)

Create a `Store` with a mix of standalone documents and one parent with children. Build the tree — assert standalone docs appear at depth 0 with `is_parent: false`, and children only appear when the parent is expanded. Ensures the tree logic doesn't break existing flat list behavior.

### T3: Virtual parent rendering (unit)

Create a `Store` with a virtual parent (no `index.md`, synthesised). Build the tree — assert the virtual parent node has `is_virtual: true`. Verifies AC5 data is available for rendering.

### T4: Collapse moves selection to parent (unit)

Set up an expanded parent with children. Set `selected_doc` to a child index. Trigger collapse. Assert `selected_doc` now points to the parent node.

### T5: Child documents not duplicated at top level (unit)

Create a `Store` with parent + children. Build the tree with parent expanded. Assert children appear only once (under the parent), not also as top-level entries.

## Notes

Reuses the ASCII connector pattern from graph mode (`src/tui/ui.rs:893-904`) for consistent visual style. The `DocListNode` struct mirrors `GraphNode` but adds `is_parent` and `is_virtual` flags specific to the folder-based parent-child relationship.
