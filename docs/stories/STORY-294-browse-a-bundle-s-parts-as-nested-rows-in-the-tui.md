---
title: Browse a bundle's parts as nested rows in the TUI
type: story
status: complete
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-074
reviewed: a4aaece7eae9555a95f80191783a11988ab9ab0a
---

## Value

As a lazyspec user browsing a bundle in the TUI, I see its parts as rows under it and can preview or open one part, so I navigate a change's `design.md` without scrolling the concatenated preview.

## Acceptance Criteria

- AC1: A document with parts (or missing declared parts) is expandable in the doc list, like a parent with children. Expanded, it shows child docs first, then one depth-1 row per part in `show --parts` order.
- AC2: Part rows render as `§ <name>`, dimmed, with no id or status cells.
- AC3: Selecting the parent row keeps the concatenated preview. Selecting a part row previews that part's body only.
- AC4: `e` on a part row opens the part file in the editor.
- AC5: A part the directory template declares but the folder lacks shows as a ghost row `§ <name> (missing)`. Its preview says the part is missing; `e` on it is a no-op.
- AC6: No config toggle. Sidecars are not listed as rows.

## Out of scope

Web view (removed, ADR-037). CLI already lists parts in `show`. Creating a missing part from the TUI.
