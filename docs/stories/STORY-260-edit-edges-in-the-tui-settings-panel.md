---
title: Edit edges in the TUI settings panel
type: story
status: in-progress
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
---
As a DAG designer, I want to add and edit edges in the TUI settings panel, so that shaping the DAG does not require hand-editing TOML.

## Acceptance criteria

- Given the settings view, when the designer opens validation configuration, then declared edges are listed by `name` with their `from`, `via`, and target set visible.
- Given an edge row, when the designer drills in, then `from`, `to`, `via`, `traversal`, and `required` are all editable.
- Given the `to` field, when edited, then multiple target types can be added and removed individually, and `"*"` is offered as a choice alongside declared type names.
- Given an edit that would produce a load error (unknown type, unknown relationship, contradictory traversal, `required` on wildcard `from`), when the designer saves it, then the TUI refuses the save with the same message the loader would give — not a different wording — and lands the cursor on the edge field at fault.
- Given a committed edit, when the config is written, then unrelated sections, comments, and ordering are preserved.
- Given a new edge seeded from the panel, when created, then it lands with a valid default shape rather than an empty row that fails to load.

## Notes

`src/tui/state/app.rs` carries 16 `ValidationRule::ParentChild` sites and `src/tui/views/panels.rs` carries the shape-cycling logic that switches a rule between `parent-child` and `relation-existence`. With one edge shape, shape-cycling has nothing to cycle and comes out.

Sharing the loader's validation rather than reimplementing it is the point of the fourth criterion — two spellings of the same error is how they drift.

**Amended 2026-09-02 (ITERATION-389):** the fourth criterion said "when the designer commits it", which read as the field commit. The panel has two commits and only the save meets the loader: `Config::parse` is whole-config and all-or-nothing, so running it at field-commit time would refuse an edge edit over an unrelated hole elsewhere in the buffer, and would refuse legitimate two-field waypoints (widening `from` to `*` before clearing `required`). Narrowing it to the edited row would be the second spelling this criterion exists to prevent. The criterion now says "saves", names the unknown-relationship error it always covered, and asks for the cursor landing the iteration built.

**Amended 2026-09-02 (ITERATION-387):** the second criterion listed `require_to_status`. ADR-033 abandoned status-conditioned gating outright and the field was removed from `EdgeDef` in `40b91f3`, so there is nothing to make editable. The criterion now names the five fields an edge row actually carries beyond `name`.
