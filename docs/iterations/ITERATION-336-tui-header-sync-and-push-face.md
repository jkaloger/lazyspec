---
title: "TUI header sync and push face"
type: iteration
status: draft
author: "Jack Kaloger"
date: 2026-07-20
tags: [tui]
related:
- implements: STORY-230
---

# Iteration: TUI header sync and push face

## Objective

Compact face top-right reflecting GitHub poll/push: loading while in flight, brief success, then idle.

## Satisfies

STORY-230 AC4 (poll/push loading face), AC5 (idle/success static, no churn), colour AC.

## Context

- Story + AC: STORY-230. Design: RFC-063 §Design (TUI). Point, don't restate.
- Foundation (consume): `src/spinners.rs` (ITERATION-332) — `spinner("face").compact(state, idx)`.
- Header render (TOUCH): `src/tui/views.rs:150-180` (`right_spans`); existing `sync_indicator_text:45-63`.
- Flags (READ/surface): `gh_push_in_flight` (used views.rs:165); `refresh_in_flight` `AtomicBool` `event_loop.rs:813`; `last_sync` set `event_loop.rs:564`.
- Clock: `loop_count` (`event_loop.rs:751`).
- Colour helper: reuse `FrameColour`→`Style` from ITERATION-333.

## Tasks

1. Surface `refresh_in_flight` onto `App` (mirror `AtomicBool`→`bool` each loop iteration, or hold the `Arc` and read in view).
2. `views.rs` `right_spans`: derive `SpinnerState` — `Loading` if `refresh_in_flight || gh_push_in_flight`; `Success` if within N ms of `last_sync`; else `Idle`. Render `spinner("face").compact(state, idx)`.
3. Idx used only for `Loading`; `Idle`/`Success` are static frames — cheap match, no per-tick recompute.
4. Keep/relocate `sync_indicator_text` wording beside the face (face = activity, text = "synced Ns ago").

## Test Plan

- Unit: state-selection given (`refresh_in_flight`, `gh_push_in_flight`, `last_sync`) → `SpinnerState`.
- Manual TUI: trigger poll/push → loading face; on complete → brief success → idle.

## Notes / conventions

- Dictum 3: TUI-only. Dictum 6: reuse 332/333 helpers.
- Depends on ITERATION-333 for the `Style` mapper (land 333 first).

## Out of scope

- Create overlay (333). CLI (334/335). Reworking sync polling itself.
