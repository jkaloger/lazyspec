---
title: Native app project picker and launch-load
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-186
---## Objective

On launch the app shows the native macOS folder picker, validates the chosen folder is a `.lazyspec/` project (re-prompting in plain language on failure), and loads it into `AppState` so the webview's `GET /` renders that project instead of a hardcoded path.

## Satisfies

STORY-186: "First launch, no remembered project — picker shown", "Pick a valid project", "Pick an invalid folder — plain-language rejection and re-prompt". Remaining ACs deferred — see Out of scope.

## Context

- Story + ACs: STORY-186 (`implements` RFC-054).
- Selection/validation design: RFC-054 §Design "Project selection and app config".
- Bridge this builds on (hardcoded path to remove): ITERATION-252 — `app::run()` loads a fixed path via `Store::load` and drives `web::server::router` through the custom scheme.
- `AppState` construction to mirror per project: `src/main.rs:578-597` (`serve` call site — `store`, `config`, `coords`, `issue_map`, `repo_name`, `branch`); struct at `src/web/server.rs:23`.
- `Store::load(&root, &config)` is the load path consumed verbatim; project root name feeds `repo_name`.
- Picker: Tauri dialog API. Validation predicate: chosen folder contains a `.lazyspec/` directory.

## Principles / conventions

- Convention doc (layering): `app -> web -> engine`, never `app -> tui`/`cli` (principle 3). Picker + validation live in `app`; `Store::load`/`AppState` consumed from `web`/`engine` as-is.
- Rust ecosystem norms + feature gating under `#[cfg(feature = "app")]` (principle 5); no indirection before a second consumer (principle 6).

## Tasks

1. Add `app::project` under `#[cfg(feature = "app")]`: a `validate_project_root(path) -> Result` checking the folder contains a `.lazyspec/` root, and a picker driver that loops picker → validate until a valid folder is chosen or the user cancels.
2. On invalid selection, surface a plain-language message (native dialog; no stack trace, no path-jargon-only text) and re-open the picker; never proceed to a view. On cancel, exit cleanly.
3. Build `AppState` from the chosen root exactly as `src/main.rs:578-597` does — `Store::load` into `Arc<Store>`, re-derive `config`, `coords`, `issue_map`, `repo_name`, `branch`. Extract this into a reusable `app`-side helper (it is re-called on switch in ITERATION-254).
4. In `app::run()`, replace the ITERATION-252 hardcoded path: run the picker loop first, then hand the resulting `AppState` to the existing scheme/router bridge; open the window only after a valid project loads.

## Out of scope

- Recents persistence in the platform config dir; remembering/reopening the most recent project; stale-entry fallback → ITERATION-254 (STORY-186 ACs "Remembered project reopens", "Remembered project is now missing/moved", "Recents persist across restarts").
- File menu (Open Project… / recents submenu) and in-session project switching → ITERATION-254 (ACs "Open a different project from the File menu", "Switch via recents submenu").
- Watcher re-point seam → ITERATION-254 / STORY-187 (AC "Switch re-points the watcher").
- Any change to `web::server::router`, routes, or templates — consumed as-is. No write path.

## Verification

- Launch with `--features app`, no folder argument → native picker appears before any window.
- Choose a folder with `.lazyspec/` → window renders that project's list; header repo chip shows its `repo_name`/`branch`.
- Choose a folder without `.lazyspec/` → plain-language rejection dialog, picker re-opens, no view shown.