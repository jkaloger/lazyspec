---
title: "TUI loading face: create overlay and sync header"
type: story
status: complete
author: "Jack Kaloger"
date: 2026-07-20
tags: [tui, ux]
related:
- implements: RFC-063
---
<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

TUI loading states are static text today. The create overlay shows a plain status line (`overlays.rs:131-136`); the top-right header shows static sync text (`views.rs:150-180`). RFC-063 introduces an extensible spinner framework and an animated face. This slice is the walking skeleton: it builds the shared `src/spinners.rs` foundation and proves it end-to-end on the two TUI surfaces, driven by the existing ~16ms render loop (`event_loop.rs:752`, free clock, no new timer).

Value lands for anyone watching the TUI: creating a doc or waiting on a GitHub sync now shows a live animated face instead of frozen text.

## Acceptance Criteria

- **Given** the shared framework is needed by both TUI surfaces
  **When** `src/spinners.rs` is built
  **Then** it exposes `SpinnerState {Idle,Loading,Success,Error}`, `FrameColour`, `Frame`, `trait Spinner`, a `FaceSpinner`, and a name→spinner registry, all pure (no ratatui/crossterm), with unit tests covering idx→frame for every state and the ASCII fallback.

- **Given** a create is reserving a remote doc (`form.loading == true`)
  **When** the create overlay renders
  **Then** it shows the full-box loading face animating each frame off the render loop.

- **Given** a create finishes
  **When** it succeeds (`Reserved`) or fails
  **Then** the overlay shows the success face or the error face respectively before dismissal.

- **Given** a GitHub poll or push is in flight (`refresh_in_flight` / `gh_push_in_flight`)
  **When** the header renders
  **Then** the top-right shows the compact loading face; on completion it shows a brief success face, then returns to idle.

- **Given** the face is Idle, Success, or Error (non-animated states)
  **When** the loop redraws
  **Then** the frame is static — no per-tick recomputation churn; only Loading animates.

- **Given** `FrameColour`
  **When** the TUI renders a frame
  **Then** it maps to a `ratatui::Style` using terminal-default accent + dim/bold (green success, red error), no hardcoded hex.

## Scope

### In Scope

- New `src/spinners.rs` framework + `FaceSpinner` + registry + unit tests (foundation for STORY-231 too).
- TUI create-overlay hook (`overlays.rs`), gated on `form.loading`, driven by `ReservationProgress`.
- TUI header hook (`views.rs`); surface `refresh_in_flight` onto `App`.
- `FrameColour` → `ratatui::Style` mapping.

### Out of Scope

- CLI spinners and the init-wizard greeting (STORY-231).
- Alternate spinner styles beyond the face (registry supports them; none added here).
- Richer per-event expressions (RFC non-goal).
