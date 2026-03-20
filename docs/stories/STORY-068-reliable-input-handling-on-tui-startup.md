---
title: Reliable input handling on TUI startup
type: story
status: draft
author: jkaloger
date: 2026-03-19
tags: []
related:
- related-to: docs/audits/AUDIT-005-tui-startup-performance.md
---


## Context

AUDIT-005 identified that the probe thread (`Picker::from_query_stdio()`) and the input thread (`crossterm::event::read()`) both read from stdin concurrently during startup. This race causes keypresses to be swallowed. Additionally, spawned threads have no lifecycle management: no `JoinHandle` storage, no shutdown signal, and no ordering guarantee that the input thread is reading before the first frame renders.

## Acceptance Criteria

- **Given** the TUI has entered the alternate screen
  **When** the user presses any navigation key (h/j/k/l, arrows, etc.) within the first 100ms
  **Then** the keypress is processed and the UI responds

- **Given** the terminal capability probe is running
  **When** the input thread is active
  **Then** they never read from stdin concurrently

- **Given** the probe thread and input thread are running
  **When** the user quits the TUI
  **Then** both threads receive a shutdown signal and are joined before `run()` returns

- **Given** the TUI is starting up
  **When** the input thread is spawned
  **Then** it confirms readiness (via barrier or channel) before the main loop begins processing events

## Scope

### In Scope

- Sequencing the terminal capability probe and input thread to eliminate stdin contention (F1)
- Decoupling `ToolAvailability::detect()` from the probe thread (F4)
- Storing `JoinHandle`s, adding a shutdown `AtomicBool`, joining threads on exit (F6)
- Adding a readiness signal from the input thread before the main loop starts (F7)

### Out of Scope

- Async/tokio migration of the event loop
- Changes to `Store::load` (covered by STORY-069)
- Changes to validation timing (covered by STORY-069)
