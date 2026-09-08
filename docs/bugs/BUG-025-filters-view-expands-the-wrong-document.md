---
title: Filters view expands the wrong document
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: STORY-275
---

## What happens

In the Filters view, the preview body shows an empty or unrelated document. `request_expansion` (`src/tui/state/expansion.rs:14`) reads `selected_doc_meta()` — `doc_tree[selected_doc]`, scoped to the current type — while the preview header and body render from `selected_filtered_doc()` — `filtered_docs_cache[selected_doc]`, all types, status- and tag-filtered. Two different lists indexed by the same cursor.

The body falls back to empty unless the tree's document at that index happens to have been visited in the Types view already.

## What should happen

The expansion dispatched is for the document the view renders.

## Notes

Found by ITERATION-429's review, which fixed the identical divergence for the staleness badge and left this one alone as out of scope. The fix is a one-word change: `selected_doc_for_view()` was added in that iteration and sits in the same file, 226 lines below `request_expansion`.

Expansion partly masks this today because it is re-keyed by path at the read site (`panels.rs:1211`, `expanded_body_cache.get(&doc.path)`), so it renders empty rather than wrong. That is why it reads as a blank preview and not as a mismatch.
