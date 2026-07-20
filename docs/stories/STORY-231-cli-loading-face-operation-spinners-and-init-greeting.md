---
title: "CLI loading face: operation spinners and init greeting"
type: story
status: draft
author: "Jack Kaloger"
date: 2026-07-20
tags: [cli, ux]
related:
- implements: RFC-063
---
<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

The CLI shows nothing during network waits: create passes no-op `on_progress` closures (`main.rs:194,207`), so reservation progress is discarded; fetch and prune block silently or print terse lines. RFC-063's face, built in STORY-230, is reused here for the CLI. `indicatif` is already a dependency (`Cargo.toml:45`) but unused — this slice wires it, using its steady-tick thread to animate the face during blocking ops. The init wizard also gains a talking-face greeting (Houston `say` equivalent).

Value lands for CLI users: long operations show a live face instead of silence, and `init` greets you.

## Acceptance Criteria

- **Given** a TTY and no `--json`
  **When** `create`/`fetch`/prune runs a network operation
  **Then** the compact face animates via `indicatif` steady-tick, mapping the progress enum to `SpinnerState`, and finishes with a green ✔ (success) or red ✗ (error) line.

- **Given** `--json` or a non-TTY stdout
  **When** any of those operations runs
  **Then** no animation is emitted and the output bytes are byte-for-byte unchanged from today (dictum 2).

- **Given** a blocking op where `on_progress` only fires on state change
  **When** the op is mid-network
  **Then** the steady-tick still animates (motion comes from the ticker, not progress events).

- **Given** the spinner is drawn
  **When** it renders and finishes
  **Then** it draws to stderr and clears cleanly on completion, not smearing stdout.

- **Given** a user runs `init` on a TTY
  **When** the wizard greets
  **Then** a talking full-box face cycles eyes/mouth per word, then rests on the happy face; suppressed under `--json`/non-TTY.

- **Given** `FrameColour`
  **When** the CLI renders a frame
  **Then** it maps to ANSI using terminal-default accent + dim/bold, no hardcoded hex.

## Scope

### In Scope

- CLI spinner helper wrapping `indicatif` (steady-tick + `tick_strings` = compact face frames, TTY/json guard, stderr, clean teardown).
- Hooks at create closures (`main.rs`), `cli/fetch.rs`, and prune (`cli/reservations.rs`).
- Hand-rolled talking-face greeting for the init wizard.
- `FrameColour` → ANSI mapping.

### Out of Scope

- The `src/spinners.rs` framework itself (delivered by STORY-230; consumed here).
- TUI surfaces (STORY-230).
- New spinner styles; richer expressions (RFC non-goals).
