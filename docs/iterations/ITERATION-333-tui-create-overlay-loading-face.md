---
title: "TUI create-overlay loading face"
type: iteration
status: complete
author: "Jack Kaloger"
date: 2026-07-20
tags: [tui]
related:
- implements: STORY-230
---

# Iteration: TUI create-overlay loading face

## Objective

Full-box face in the TUI create overlay: animate loading off the render loop; show success/error on finish.

## Satisfies

STORY-230 AC2 (loading animates), AC3 (success/error on finish), colour AC (`FrameColour`→`ratatui::Style`).

## Context

- Story + AC: STORY-230. Design: RFC-063 §Design (TUI). Point, don't restate.
- Foundation (consume): `src/spinners.rs` (ITERATION-332) — `spinner("face").full(state, idx)`.
- Overlay render (TOUCH): `src/tui/views/overlays.rs:131-136` (status line).
- Form state (TOUCH): `src/tui/state/forms.rs:55` (`loading`, `status_message`); add `state: SpinnerState`.
- Progress source (READ): create thread `src/tui/state/app.rs:2444-2486`; `AppEvent::CreateProgress` handler `src/tui/infra/event_loop.rs:590-593`; `ReservationProgress` enum `src/engine/reservation.rs:14-42`.
- Clock (READ): render loop `event_loop.rs:751-799`, `loop_count:751`.

## Tasks

1. Map `ReservationProgress`→`SpinnerState`: `QueryingRemote|PushAttempt|PushRejected`→`Loading`; `Reserved`→`Success`; create error→`Error`. Store on create-form when handling `CreateProgress` (event_loop.rs:590-593).
2. Surface frame idx into `draw`: pass `loop_count` (or store on `App`).
3. `overlays.rs`: while `form.loading`, render `spinner("face").full(form.state, idx)` above/replacing the status line.
4. `FrameColour`→`ratatui::Style` helper (TUI side): `Accent`→default fg, `Dim`→`Modifier::DIM`, `Success`→`Green`, `Error`→`Red`. No hex.
5. Hold success/error frame briefly on finish (reuse existing dismissal timing).

## Test Plan

- Unit: `ReservationProgress`→`SpinnerState` map; `FrameColour`→`Style` map.
- Manual TUI: create into remote store → loading animates, success face on done, error face on failure.

## Notes / conventions

- Dictum 3: TUI-only; no CLI dep. Dictum 6: reuse 332, no new abstraction.
- Only `Loading` animates; success/error held.

## Out of scope

- Header face (336). CLI (334/335). Idle state (overlay hidden when not loading).
