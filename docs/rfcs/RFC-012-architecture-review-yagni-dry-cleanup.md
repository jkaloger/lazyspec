---
title: 'Architecture Review: YAGNI/DRY Cleanup'
type: rfc
status: accepted
author: jkaloger
date: 2026-03-06
tags:
- refactor
- architecture
related:
- related-to: RFC-009
---




## Summary

An audit of the lazyspec codebase against YAGNI and DRY principles surfaced
six findings across the CLI and TUI modules. None are critical, but the
medium-severity items add maintenance cost and increase the chance of
divergent behaviour when one copy gets updated and the other doesn't.

This RFC records the findings and identifies three Stories to address them.

## Context

RFC-009 (Codebase Quality Baseline) and STORY-028 delivered three cleanup
iterations (engine, CLI/TUI, validation extraction). This RFC continues
that quality trajectory with a narrower lens: eliminating duplication and
removing speculative code.

## Findings

### DRY Violations

#### 1. Frontmatter reconstruction (Medium)

Five call-sites reconstruct a markdown document after modifying its YAML
frontmatter:

| File | Occurrences |
|------|-------------|
| `src/cli/ignore.rs` | 2 (lines 15, 33) |
| `src/cli/link.rs` | 2 (lines 29, 55) |
| `src/tui/app.rs` | 1 (line 21) |

Each uses `format!("---\n{}---\n{}", new_yaml, body)`. The surrounding
read-parse-modify-write logic is also duplicated. A shared
`rewrite_frontmatter` utility would centralise this and prevent subtle
formatting drift (e.g. trailing newlines).

@ref src/cli/ignore.rs#15
@ref src/cli/link.rs#29
@ref src/tui/app.rs#21

#### 2. Document list rendering (Medium)

`draw_doc_list` (ui.rs:156-224) and the list section of `draw_filters_mode`
(ui.rs:736-806) build `ListItem` spans with near-identical logic: status
colour, title truncation, tag display with "N tags" overflow. When one
gets a fix or style change, the other is easy to miss.

@ref src/tui/ui.rs#draw_doc_list
@ref src/tui/ui.rs#draw_filters_mode

#### 3. Tag rendering loop (Low)

Three locations iterate over tags, call `tag_color()`, and build styled
spans with the same structure (ui.rs lines ~177, ~299, ~759). Could be
a small helper returning `Vec<Span>`.

### YAGNI Violations

#### 4. ViewMode::Metrics placeholder (Medium)

The `Metrics` variant exists in the `ViewMode` enum (app.rs:176), participates
in the mode cycle (app.rs:184-185), and has a render function
`draw_metrics_skeleton` (ui.rs:861) that draws empty blocks with no data.
This is speculative scaffolding with no backing implementation.

@ref src/tui/app.rs#ViewMode
@ref src/tui/ui.rs#draw_metrics_skeleton

#### 5. Dual JSON output paths in list and search (Low)

`cli/list.rs` and `cli/search.rs` each expose a `pub fn run_json()` alongside
`pub fn run()` which already handles JSON output via a boolean flag. The
`run_json` variants are only called from tests. This isn't dead code, but
it is a redundant API surface. The `run()` functions could be refactored so
tests call the same path the CLI does.

@ref src/cli/list.rs#run_json
@ref src/cli/search.rs#run_json

#### 6. resolve_editor_from over-abstraction (Low)

`resolve_editor_from(editor, visual)` (app.rs:26) exists as a parameterised
version of `resolve_editor()` (app.rs:41), but is only called by that wrapper.
The parameterised form is tested directly in `tui_editor_test.rs`, which is
the reason it's public. This is minor, but the indirection adds no value --
the test could call `resolve_editor` with controlled env vars, or the two
functions could be collapsed.

@ref src/tui/app.rs#resolve_editor_from

## Stories

### Story 1: Frontmatter Utility Extraction

Extract a shared `rewrite_frontmatter(path, mutate_fn)` utility into the
engine module. Replace all five call-sites. Pure refactor, no behaviour change.

### Story 2: TUI Rendering Consolidation

Extract the document list-item builder into a helper used by both
`draw_doc_list` and `draw_filters_mode`. Consolidate the tag-span rendering
loop into a small function. Pure refactor.

### Story 3: YAGNI Dead Code Removal

- Remove `ViewMode::Metrics` and `draw_metrics_skeleton`
- Collapse `resolve_editor_from` into `resolve_editor` (update tests)
- Unify `run`/`run_json` in list.rs and search.rs into a single code path

## Non-goals

- This RFC does not propose new features or abstractions beyond what's needed
  to eliminate the duplication
- No changes to public CLI behaviour
- No test coverage expansion (though tests will be updated where signatures change)
