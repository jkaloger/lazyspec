---
title: Markdown newline preservation
type: iteration
status: accepted
author: agent
date: 2026-05-07
tags: []
related:
- implements: STORY-116
---



## Changes

1. **Probe `tui_markdown` HardBreak handling** [AC1, AC4, AC5]
   - File: `src/tui/content/gfm.rs` (existing `#[cfg(test)] mod tests`)
   - Add test `tui_markdown_preserves_hard_break`: input `"foo  \nbar\n"` → `tui_markdown::from_str` returns ≥2 `Line`s (one ending after `foo`, one starting `bar`).
   - Input `"foo  \nbar"` → `render_gfm_segments(extract_gfm_segments(input), 80).len() >= 2`.
   - Purpose: lock in current behaviour. If tui_markdown 0.3 already preserves HardBreak, AC1/4/5 pass via existing pipeline; only admonition needs fix. If it doesn't, escalate (replace markdown segment renderer or upgrade crate).
   - Verify: `cargo test tui_markdown_preserves_hard_break --lib`.

2. **Fix `AdmonitionExtractor` SoftBreak vs HardBreak** [AC2]
   - File: `src/tui/content/gfm/parse.rs:154`
   - Current: `Event::SoftBreak | Event::HardBreak => self.body.push('\n');`
   - Change: `Event::SoftBreak => self.body.push(' ');` and `Event::HardBreak => self.body.push('\n');`
   - Why: CommonMark SoftBreak = source line wrap (render as space). HardBreak = explicit `<br>` (`  \n` or `\\\n`) — render as `\n`. STORY-116 AC2 is about *explicit* newlines.
   - `render_admonition` already splits on `\n` via `body.lines()` (`render.rs:134`). No change required there.
   - Verify: `cargo test gfm` passes existing tests.

3. **Add AC tests** [AC1, AC2, AC3, AC4, AC5]
   - File: `src/tui/content/gfm.rs` test mod
   - `markdown_segment_preserves_hard_break` (AC1): `"line one  \nline two"` → `render_gfm_segments(...).len() >= 2`.
   - `admonition_preserves_internal_hard_break` (AC2): `"> [!NOTE]\n> first  \n> second"` → resulting `GfmSegment::Admonition.body` contains `'\n'` separating `first` and `second`. Render output ≥ 3 lines (label + 2 body lines).
   - `admonition_soft_break_renders_as_space` (AC2 sibling): `"> [!NOTE]\n> first\n> second"` (no trailing two-spaces) → body = `"first second"`, no `'\n'`.
   - `code_block_preserves_newlines` (AC3): ```"```\nfn a()\nfn b()\n```"``` → rendered output contains separate lines `fn a()` and `fn b()`.
   - `mixed_paragraphs_and_hard_breaks` (AC4): `"para1 line  \npara1 line2\n\npara2"` → ≥3 non-empty lines; blank line between paragraphs preserved.
   - `hard_break_survives_long_lines` (AC5): single rendered line is the `Line` value; `Paragraph::wrap` applied later — assert `Line` boundary count, not pixel wrap. Construct line longer than `max_width=20` containing hard break; assert ≥2 lines emitted by `render_gfm_segments`. Verify `Paragraph::new(lines).wrap(Wrap { trim: false })` does not merge `Line`s (this is a ratatui invariant — note in test comment, no widget render assertion).
   - Per DICTUM-004: behavioral (assert content), isolated (per-test fixtures), deterministic (literal strings), readable (one AC per test).
   - Verify: `cargo test gfm --lib`.

4. **Manual TUI smoke** [AC1–AC5]
   - Run `cargo run`, open a doc with mixed content (admonition, code block, table, hard breaks).
   - Confirm preview panel renders breaks correctly under window resize (AC5 soft-wrap interaction).
   - Note in iteration close: visual check passed.

5. **Validate** [all ACs]
   - `cargo test --all`
   - `cargo clippy --all-targets -- -D warnings`
   - `lazyspec validate --json`

## Test Plan

Per DICTUM-004: behavioral, isolated, deterministic, readable, real types.

- AC1 — `markdown_segment_preserves_hard_break`: literal markdown w/ `  \n` hard break → assert ≥2 `Line`s out of `render_gfm_segments`.
- AC2 — `admonition_preserves_internal_hard_break` + `admonition_soft_break_renders_as_space`: extractor body content + render line count.
- AC3 — `code_block_preserves_newlines`: multi-line fenced code → render emits separate `Line` per source line.
- AC4 — `mixed_paragraphs_and_hard_breaks`: blank-line paragraph break + hard-break inline; assert structure.
- AC5 — `hard_break_survives_long_lines`: hard break preserved in `Vec<Line>` output regardless of `max_width`. ratatui `Paragraph::wrap` documented as wrapping within `Line`, never merging across. No widget snapshot test (out per DICTUM-004 "predictive" — would assert ratatui internals).

Probe test (task 1) is a baseline lock; if it fails, scope grows and we revisit plan with user.

Out of test scope: visual diff of rendered TUI frames (snapshot tests on widgets). Verified manually in task 4.

## Notes

- `tui-markdown = "0.3"` already in Cargo.toml. No new deps.
- `textwrap = "0.16"` already present (from ITER-162). Not needed for this iteration but available for STORY-117 (sibling).
- `AdmonitionExtractor` SoftBreak handling looks like a pre-existing bug (collapses both into `\n`). Fix scoped to this iteration since it's blocking AC2.
- Tree intent (per RFC-040 §2): hard `\n` in markdown source → visible line break in preview; soft wrap (terminal width overflow) handled by `Paragraph::wrap`. Two layers, don't conflate.
- If task 1 reveals tui_markdown 0.3 collapses HardBreak: stop, re-plan w/ user. Options = upgrade crate, replace markdown renderer, or pre-process input.

## Findings

- Task 1 probe: `tui_markdown` 0.3.7 preserves CommonMark HardBreak as separate `Line`s. AC1/4/5 baseline locked via existing pipeline; only admonition path needed fix.
- Task 2 surfaced parallel issue in `FootnoteExtractor` (`parse.rs:211`) — same `SoftBreak | HardBreak => '\n'` collapse. Out of STORY-116 scope (story is `render_gfm_segments` markdown + admonition path). Flag for follow-up iteration if footnote rendering becomes a target.
- `TableExtractor` (`parse.rs:76`) maps both breaks to space — relevant for STORY-117 (GFM table multi-line cells). Note for that iteration: hard-break-as-newline inside cells will need parallel fix.
