---
title: TUI Interaction Enhancements
type: rfc
status: draft
author: "@jkaloger"
date: 2026-03-08
tags:
  - tui
related:
  - related-to: docs/rfcs/RFC-006-tui-progressive-disclosure.md
  - related-to: docs/rfcs/RFC-011-tui-ux-refinements.md
---

## Problem

The TUI has a handful of interaction gaps that make it feel clunky for day-to-day use:

The document list uses a `List` widget with hand-rolled column spacing via format strings. This works, but columns don't align when content varies in length, and the rendering logic differs between Types mode and Filters mode. Ratatui ships a `Table` widget that handles column alignment natively.

Scrolling is minimal. The document list relies on `ListState`'s built-in viewport, which snaps the selection to the edge of the visible area with no padding. There's no way to page through long lists quickly. The fullscreen preview has line-by-line scrolling but no half-page jump. Neither view shows a scrollbar, so there's no indication of position within the content.

Tags can only be set at document creation time. Editing tags on an existing document requires opening it in `$EDITOR` and manually editing frontmatter. A quick inline editor would keep users in the TUI for this common operation.

> [!NOTE]
> The `s` key status picker is covered by STORY-016 under RFC-006 and is not part of this RFC.

## Intent

Five targeted improvements to tighten up the TUI's interaction model. Each is independently shippable.

## 1. Table widget for document list

Replace the `List` widget in `draw_doc_list` with ratatui's `Table` widget. This gives us proper column alignment without manual format-width hacks.

**Columns:**

| Column | Width | Content |
|--------|-------|---------|
| Tree | 4 chars fixed | Expand/collapse indicator or tree connector |
| ID | 14 chars fixed | Document ID (e.g. `RFC-018`) |
| Title | Flexible (fill) | Document title, truncated to fit |
| Status | 12 chars fixed | Colored status label |
| Tags | 20 chars min | First 3 tags as `[tag]`, overflow as `+N` |

The tree column preserves the current parent/child hierarchy and expand/collapse indicators. Virtual documents still show `(virtual)` appended to the title.

Highlight style stays as `Modifier::REVERSED` for the selected row. The table uses the same dim styling when the Relations tab is focused.

This table layout should be shared between Types mode and Filters mode so the document list looks identical regardless of how you navigated to it.

```
@ref src/tui/ui.rs#draw_doc_list
@ref src/tui/ui.rs#doc_list_node_spans
```

## 2. Scroll padding and sticky viewport

Two changes to how the document list viewport behaves:

**Padding:** Keep 2 documents visible above and below the selection at all times (where possible). When the user moves down with `j`, the viewport scrolls once the selection is within 2 rows of the bottom edge. Same logic applies upward with `k`. This prevents the selection from sitting at the very edge of the visible area.

This is the `scrolloff` concept from vim. Ratatui's `List`/`Table` widgets don't support this natively, so we need to manage the viewport offset manually rather than relying on `ListState`'s auto-scroll.

**Sticky viewport on scroll-up:** When the user scrolls down past the visible area, the viewport follows (current behavior). When scrolling back up, the viewport stays put until the selection reaches the top padding boundary. This avoids the jarring snap that happens when the viewport tracks the selection on every keystroke.

Combined, these create a scrolling feel similar to neovim with `scrolloff=2`.

```
@ref src/tui/app.rs#scroll_down
@ref src/tui/app.rs#scroll_up
```

## 3. Half-page scrolling with `Ctrl-D` / `Ctrl-U`

Add `Ctrl-D` (half page down) and `Ctrl-U` (half page up) keybindings that move the selection and viewport together by half the visible height.

**In document lists (Types, Filters):** Move the selection by `visible_height / 2` rows. The viewport adjusts to maintain the scroll padding from section 2. If the jump would overshoot the list boundaries, clamp to the first or last item.

**In fullscreen preview:** Adjust `scroll_offset` by `visible_height / 2`. The scroll padding rules don't apply here since there's no selection cursor, just a viewport position.

These bindings are handled in the key dispatch alongside existing `j`/`k` handlers. They should be active in all modes where `j`/`k` work (normal navigation, fullscreen, and the filtered document list) but not in modal states (create form, delete confirm, search overlay).

```
@ref src/tui/app.rs#handle_fullscreen_key
@ref src/tui/app.rs#handle_key_event
```

## 4. Scrollbar in focused scrollable views

Add a `Scrollbar` widget (ratatui provides one) to any scrollable view that currently has focus. The scrollbar renders on the right edge of the content area, inside the border.

**Views that get a scrollbar:**

- Document list (Types mode, Filters mode) when the list is longer than the visible area
- Fullscreen preview
- Relations list when it overflows

The scrollbar only renders when the view is focused (not dimmed). It uses `ScrollbarState` seeded with the total content length and current offset. Styling: thin track in `DarkGray`, thumb in `Cyan` to match the focused border color.

```
@ref src/tui/ui.rs#draw_doc_list
@ref src/tui/ui.rs#draw_fullscreen
```

## 5. Tag editor with `t` key

Pressing `t` on a selected document opens a tag editor overlay, similar to how STORY-016 handles the status picker.

**Behavior:**

1. `t` opens the overlay. The current document's tags are shown as removable chips.
2. A text input sits below the tag list. Typing filters existing tags from the project (autocomplete).
3. `Enter` adds the typed or selected tag. If the tag doesn't exist yet, it creates a new one.
4. `Backspace` on an empty input removes the last tag (like a token input).
5. `Esc` closes the overlay and writes the updated tags to frontmatter.
6. `d` on a highlighted tag removes it.

**Autocomplete source:** `app.store` already tracks all tags across documents. The overlay collects unique tags, excludes ones already on the current document, and filters by the current input prefix.

**Frontmatter write-back:** Uses the existing `update_tags()` function in `app.rs` which rewrites the `tags` field via `rewrite_frontmatter`. The store reloads automatically via the file watcher.

```
@ref src/tui/app.rs#update_tags
@ref src/tui/app.rs#handle_key_event
```

## Stories

1. **Table widget for document list** - Replace `List` with `Table` in `draw_doc_list`. Shared layout between Types and Filters modes. Tree hierarchy preserved.

2. **Scroll padding and half-page navigation** - Manual viewport management with `scrolloff=2` behavior. `Ctrl-D`/`Ctrl-U` for half-page jumps in lists and fullscreen preview. Sticky viewport on scroll-up.

3. **Scrollbar** - Add `Scrollbar` widget to document list, fullscreen preview, and relations list. Only shown when focused and content overflows.

4. **Tag editor** - `t` key opens tag overlay with autocomplete. Add/remove tags inline. Writes back to frontmatter on close.
