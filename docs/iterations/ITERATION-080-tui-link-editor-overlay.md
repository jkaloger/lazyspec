---
title: TUI Link Editor Overlay
type: iteration
status: draft
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-069-tui-link-editor.md
---


Covers AC1-6, AC9-10 from STORY-069. AC7-8 (delete link with confirmation) deferred to a follow-up iteration.

## Changes

### Task 1: LinkEditor state, lifecycle, and basic rendering

**ACs addressed:** AC1, AC6, AC9

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`
- Test: `tests/tui_link_editor_test.rs`

**What to implement:**

Define a `LinkEditor` struct (modelled on `StatusPicker` at app.rs:205):

```rust
pub struct LinkEditor {
    pub active: bool,
    pub doc_path: PathBuf,       // source document
    pub rel_type_index: usize,   // index into REL_TYPES
    pub query: String,
    pub results: Vec<PathBuf>,
    pub selected: usize,
}
```

Add a `pub link_editor: LinkEditor` field to `App`.

Methods on `App`:
- `open_link_editor()` -- guard: return early if no document is selected (AC9). Set `active = true`, `doc_path` to the selected document, `rel_type_index = 0`, `query = ""`, populate `results` with all documents except self (AC10 prep), `selected = 0`.
- `close_link_editor()` -- reset all fields, `active = false`.
- `update_link_search()` -- called on every keystroke. Filters store documents by substring match on shorthand ID and title against `self.link_editor.query`. Excludes `self.link_editor.doc_path`. Stores results in `self.link_editor.results`, clamps `selected`.

Wire `r` in `handle_normal_key()` (near app.rs:1833) to call `open_link_editor()` when `self.preview_tab == PreviewTab::Relations`.

Add `handle_link_editor_key()` to the dispatch chain in `handle_key()` (app.rs:1382-1409), after `status_picker.active` check. Handle `Esc` -> `close_link_editor()`.

Add `draw_link_editor()` in ui.rs, called from `draw()` after `draw_status_picker` (ui.rs:208). Render a centered popup with a border, title "Add Relation", the relationship type field, the search input, and a results list. Follow the same `Clear` + `Paragraph` pattern as `draw_status_picker` (ui.rs:1067).

**How to verify:**
- `cargo test tui_link_editor` -- tests cover open/close lifecycle, AC9 guard, Esc cancels.

---

### Task 2: Document search, filtering, and display

**ACs addressed:** AC2, AC3, AC10

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`
- Test: `tests/tui_link_editor_test.rs`

**What to implement:**

In `update_link_search()` (added in Task 1), implement the filtering logic:

1. Build a candidate list from `store.docs` values, excluding the doc at `self.link_editor.doc_path` (AC10).
2. For each candidate, build a display string: `"{TYPE}-{NNN}: {title}"` using the document's shorthand ID and title (AC3).
3. If `query` is non-empty, filter candidates where the lowercased display string contains the lowercased query (AC2).
4. Sort results by display string for stable ordering.
5. Store filtered `Vec<PathBuf>` in `results`, clamp `selected` to bounds.

Call `update_link_search()` in `handle_link_editor_key()` on every `Char(_)` input and on `Backspace`.

In `handle_link_editor_key()`, add `j`/`Down` and `k`/`Up` to navigate `selected` within `results`.

In `draw_link_editor()`, render each result in `TYPE-NNN: Title` format. Highlight the `selected` entry with `Color::Cyan`.

**How to verify:**
- `cargo test tui_link_editor` -- tests cover: typing filters results, self-link excluded, display format matches `TYPE-NNN: Title`.

---

### Task 3: Relationship type cycling, link creation, and help text

**ACs addressed:** AC4, AC5

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`
- Test: `tests/tui_link_editor_test.rs`

**What to implement:**

Define a constant array of relationship types:
```rust
const REL_TYPES: [&str; 4] = ["implements", "supersedes", "blocks", "related-to"];
```

In `handle_link_editor_key()`:
- `Tab` increments `rel_type_index` modulo 4 (AC4).
- `Enter` (when `results` is non-empty): call `crate::cli::link::link(root, store, from, rel_type, to)` with the source doc path, `REL_TYPES[rel_type_index]`, and the selected result path. Then call `store.reload_file()`, rebuild caches (`filtered_docs_cache = None`, `rebuild_search_index()`, `build_doc_tree()`), and `close_link_editor()` (AC5). If `results` is empty, `Enter` does nothing.

In `draw_link_editor()`, display the current relationship type (`REL_TYPES[rel_type_index]`) in the overlay header or as a labelled field. Show `Tab` hint.

Update the help overlay text (ui.rs:906) to document `r` (add relation) in the Relations panel section.

**How to verify:**
- `cargo test tui_link_editor` -- tests cover: Tab cycles through all 4 types wrapping around, Enter with a selected doc writes the link (verify via store state), Enter with empty results is a no-op.

## Test Plan

All tests go in `tests/tui_link_editor_test.rs`, following the `TestFixture` + `App::new` pattern from `tests/tui_status_picker_test.rs`.

| # | AC | Test | Verifies |
|---|-----|------|----------|
| 1 | AC1 | `r` on Relations panel with doc selected opens overlay (`link_editor.active == true`) | Overlay opens |
| 2 | AC9 | `r` with no document selected does not open overlay (`link_editor.active == false`) | Guard works |
| 3 | AC6 | Open overlay then press `Esc` -> `link_editor.active == false`, no frontmatter changes | Cancel works |
| 4 | AC2 | Open overlay, type query, assert `results` shrinks to matching docs | Filtering works |
| 5 | AC3 | Open overlay, assert result display strings match `TYPE-NNN: Title` format | Display format |
| 6 | AC10 | Open overlay, assert source doc not in `results` | Self-link prevented |
| 7 | AC4 | Press `Tab` 5 times, assert `rel_type_index` cycles 0->1->2->3->0->1 | Type cycling |
| 8 | AC5 | Select a doc and type, press `Enter`, assert link appears in source doc's `related` frontmatter | Link creation |

Tests 1-7 are unit-level (fast, isolated, deterministic). Test 8 is integration-level since it writes to the filesystem via `cli::link::link()`, which trades some speed for being predictive of real behaviour.

## Notes

- No fuzzy matching library needed. Substring `contains` on lowercased display strings matches the existing search pattern and satisfies the ACs.
- The `r` binding only activates when `preview_tab == PreviewTab::Relations`. In other panels, `r` remains unbound.
- The existing `d` binding for document deletion in `handle_normal_key` is unchanged by this iteration. Delete-link (`d` on a selected relation) is deferred to the follow-up iteration (AC7-8).
