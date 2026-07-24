---
title: "related-to relations missing from TUI Relations tab"
type: bug
status: fixed
author: "unknown"
date: 2026-07-21
tags: []
related: []
---

## Context

`related-to` relations declared in a document's frontmatter do not appear in the TUI Relations tab. Other relations (the traversal-marked ones, e.g. `implements`) show fine. The same relations DO render in the web view, so the data is present and parsed correctly -- the TUI just drops them.

## Root Cause

The TUI Relations tab renders only what the engine's `resolve_chain` returns, and `resolve_chain`'s related-neighbourhood BFS is gated on a config traversal marker. Relations without that marker are silently dropped.

Causal chain:

1. Relations are config-driven, not a closed enum: `RelationshipDef { name, inverse, github_native, traversal }` (`src/engine/config.rs:392-408`), `enum Traversal { Chain, Related }` (`src/engine/config.rs:21-25`).
2. Parsing is correct and lossless -- `related-to` is stored in both directions (`src/engine/document.rs:401-422`, `src/engine/store/links.rs:89-102`). The data is there.
3. TUI gather: `relation_sections()` (`src/tui/state/app.rs:2322-2350`) calls `resolve_chain(store, doc.id, 1)` and takes only `resolved.chain` / `.forward` / `.related`. It never reads `doc.related` directly.
4. TUI render: `render_relationship_sections` (`src/tui/views/panels.rs:1224-1334`) faithfully draws chain/children/related -- no filtering here. The render is not the culprit.
5. **The gate:** related BFS keeps a candidate only if its rel_type is in `store.related_relationships` -- forward `src/engine/context.rs:126-129`, reverse `:137-140`. `store.related_relationships` is populated (`src/engine/store.rs:120-125`) ONLY from relationship names whose `traversal == Some(Traversal::Related)`.

So: a `related-to` `[[relationships]]` entry WITHOUT `traversal = \"related\"` is parsed and stored, but rejected by the BFS filter, so `relation_sections().related` is empty and the tab shows nothing. Same silent drop hits every non-traversal relation (`supersedes`, `blocks`, `member-of`).

Note: the current repo `.lazyspec.toml` DOES carry `traversal = \"related\"` on `related-to`, so it resolves there. The defect manifests against any config missing the marker -- one predating the traversal field, hand-edited, or generated without it.

## Expected vs Actual

- **Expected:** every declared relation on a document, including `related-to`, appears in the TUI Relations tab.
- **Actual:** only traversal-marked relations appear; `related-to` (and other unmarked relations) silently vanish from the tab, while still showing in the web view.

## Divergence across surfaces

- **Web:** builds relations directly from `doc.related` (all types, ungated) -- `src/web/render.rs:192-199` -- plus a separate traversal-gated context section (`:159-165`). So related-to always renders. This is why the bug is TUI-visible but web-invisible.
- **CLI `context`:** shares the TUI gate -- `src/cli/context.rs:16,329` call `resolve_chain`, print `resolved.related` (`:363-373`). Drops related-to under the same condition. Fixing the TUI should close the CLI gap too.

## Repro

1. Use a config whose `related-to` `[[relationships]]` entry has no `traversal = \"related\"`.
2. Add `related-to: SOME-DOC` to a document's frontmatter.
3. Open the TUI, view the doc's Relations tab -> related-to absent.
4. Open the same doc in the web view -> related-to present. (Confirms parse/store correct.)

## Fix Direction

Two options:

- **(a) Preferred -- render `doc.related` directly.** Make the TUI Relations tab (and CLI `context`) surface the document's declared `related` list in addition to the traversal-based context, matching the web view. Closes the TUI/web divergence and the CLI gap in one place, independent of traversal markers.
- **(b) Implicit-Related fallback.** When building `store.related_relationships` (`store.rs:120-125`), treat a symmetric (no-inverse) relationship with no traversal as implicitly `Related`. Narrower; still leaves asymmetric unmarked relations (`blocks`, `supersedes`) hidden.

## Acceptance Criteria

- [ ] `related-to` relations appear in the TUI Relations tab regardless of the config's `traversal` marker.
- [ ] TUI and web view show the same declared relations for a document.
- [ ] CLI `context` surfaces `related-to` under the same conditions as the TUI.
- [ ] Existing traversal-based chain/children/related context still renders.
- [ ] State-level tests cover a related-to relation with and without the traversal marker.
- [ ] Full check green: `cargo fmt --check`, `cargo clippy`, `cargo test`.
