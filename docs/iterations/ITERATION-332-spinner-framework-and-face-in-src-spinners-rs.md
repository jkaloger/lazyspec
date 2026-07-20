---
title: "Spinner framework and face in src/spinners.rs"
type: iteration
status: draft
author: "Jack Kaloger"
date: 2026-07-20
tags: [foundation]
related:
- implements: STORY-230
---

# Iteration: Spinner framework and face in src/spinners.rs

## Objective

New pure crate-root `src/spinners.rs`: `Spinner` trait + registry + `FaceSpinner` frames + tests. No I/O, no ratatui/crossterm. Foundation for all other slices.

## Satisfies

STORY-230 AC1 (framework). Partial: purity precondition for the `FrameColour`→terminal ACs (mapping itself deferred to consumers).

## Context

- Story + AC: STORY-230.
- Design (types, frame catalogue, ASCII fallback): RFC-063 §Design, §"The face — frame catalogue". Point, don't restate.
- Placement rationale: RFC-063 ADR "spinner logic lives at crate root". Register `mod spinners;` in crate root (`src/main.rs` / lib root beside existing root modules).

## Tasks

1. New `src/spinners.rs`; `mod spinners;` at crate root.
2. Types: `SpinnerState {Idle,Loading,Success,Error}`, `FrameColour {Accent,Dim,Success,Error}`, `Frame { lines: Vec<String>, colour: FrameColour }`.
3. `trait Spinner { fn compact(&self, state: SpinnerState, idx: u64) -> Frame; fn full(&self, state: SpinnerState, idx: u64) -> Frame; }`.
4. `struct FaceSpinner { ascii: bool }` impl per RFC-063 catalogue: idle 2f, loading 4f, success/error 1f held. `idx % cycle_len`. Unicode + ASCII glyph sets.
5. Registry `fn spinner(name: &str) -> &'static dyn Spinner`, default `"face"`.

## Test Plan

- idx→frame per state (all 4), cycle wrap at boundary.
- ASCII set selected when `ascii=true`; no non-ASCII bytes in output.
- Box-width invariant: every `full` frame = equal line count + equal display width across all states/frames.
- Registry returns `FaceSpinner` for `"face"` + default.

## Notes / conventions

- Convention dictum 3: crate-root module, not `engine`; no CLI/TUI dep.
- Dictum 6: registry indirection accepted — RFC goal is pluggable spinners (stated exception).
- No `FrameColour`→`Style`/ANSI here; consumers own it (333/334).

## Out of scope

- All wiring: TUI overlay 333, TUI header 336, CLI spinner 334, greeting 335.
- Colour→terminal mapping. Extra spinner styles.
