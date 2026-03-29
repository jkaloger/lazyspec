---
title: "TUI Patterns"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [tui, patterns]
---

## State & Rendering

- State and rendering are separate concerns — `tui/state/` owns application state and transitions, `tui/views/` owns how state gets drawn. Views read state, they don't mutate it
- Key handling produces state transitions, not side effects. A keypress updates state, the next render cycle reflects it
- TUI state is testable without a terminal — tests construct state, call transition methods, and assert on the resulting state

## Event Loop

- The event loop in `tui/infra/event_loop.rs` is the single driver — it reads crossterm events, dispatches to state, and triggers renders. Don't add secondary event sources outside this loop
- `crossbeam-channel` handles communication between the event loop and background work. Don't spawn threads that mutate state directly — send messages through the channel

## Widgets & Rendering

- Widgets are ratatui widgets — compose them, don't fight the framework. If you need custom rendering, implement `Widget` rather than drawing raw cells
- Colors and theming go through `tui/views/colors.rs` — don't hardcode color values in view modules

## Overlays

- Overlays (dialogs, pickers, forms) are state variants, not separate widget trees. The view layer checks what overlay is active and renders accordingly

## Feature Gating

- Feature-gated code (`agent`) should be behind `#[cfg(feature = "...")]`, not runtime flags
