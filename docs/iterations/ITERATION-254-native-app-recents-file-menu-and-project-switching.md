---
title: Native app recents, File menu, and project switching
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-186
---## Objective

Persist a recents list in the platform config dir, add a native File menu (Open Project… + recents), reopen the most-recent valid project on launch (falling back to the picker), and switch projects by rebuilding `AppState` and re-pointing the watcher.

## Satisfies

STORY-186 AC "Recents persist across restarts in the platform config dir", "Remembered project reopens on next launch", "Remembered project is now missing/moved", "Open a different project from the File menu", "Switch via recents submenu", "Switch re-points the watcher". AC "First launch, no remembered project — picker shown", "Pick a valid project", "Pick an invalid folder" already satisfied by ITERATION-253 (picker + `.lazyspec/` validation + launch-load) — reused, not re-implemented.

## Context

- Story + ACs: STORY-186 (`implements` RFC-054).
- Design (project selection, app config, recents dir, switch reload): RFC-054 §Design "Project selection and app config"; module surfaces `app::project`, `app::run()` under §Interfaces.
- Preceding slice this builds on: ITERATION-253 — folder picker, `.lazyspec/` validation, and launch-load of a chosen project into `AppState`. Consume its picker + validation entry points verbatim; this slice adds persistence, menu, launch-decision, and switching around them.
- Bridge/runtime already built: ITERATION-252 — `app::run()`, app-owned tokio runtime, custom scheme, `http::Request` → `web::server::router` adapter. The window and router are consumed as-is.
- State to rebuild on switch: `AppState` fields `store`/`config`/`coords`/`issue_map`/`repo_name`/`branch` (`src/web/server.rs:22`); mirror ITERATION-253's construction path, not a new one.
- Config-dir resolution: a platform config-dir crate (e.g. `directories`/`dirs`) — not a hardcoded `~/Library/...`.

## Principles / conventions

- Convention doc (layering): `app -> web -> engine`; project/menu/recents live in `app::project` / the `app` shell, `Store::load` + `AppState` construction consumed from `web`/`engine` (principle 3).
- Rust ecosystem norms for the config-dir crate + serde on the recents file (principle 5); no indirection before a second consumer (principle 6).

## Tasks

1. Add the config-dir crate (and serde if not present) as optional deps under the `app` feature in `Cargo.toml`.
2. In `app::project`, resolve the config dir via the crate and add load/save of a recents file (ordered, deduped, MRU-first); tolerate a missing/corrupt file as empty recents.
3. Add a `record_recent(path)` step invoked on every successful open (initial launch-open and switches) so the picker/validation path from ITERATION-253 feeds recents.
4. Add the launch decision to `app::run()`: reopen the most-recent recents entry if it still exists and passes ITERATION-253's `.lazyspec/` validation; on a gone/invalid entry, drop it and fall through to ITERATION-253's picker. No broken/empty view is opened.
5. Build a native Tauri menu: File > Open Project… (invokes ITERATION-253's picker) and a File > recents submenu populated from the recents file; wire menu events to the switch flow.
6. Implement `switch_project(path)` in the `app` shell: reload `Store` and rebuild `AppState` (all six fields) the ITERATION-253 way, swap it behind the router, reload the webview, and call the watcher re-point seam.
7. Add the watcher re-point seam only — a single call site (documented no-op stub if STORY-187/ITERATION-255 lands after) that `switch_project` invokes with the new root; do not author the watcher.

## Out of scope

- `notify` watcher authoring, `Arc<Store>` live swap on file change, htmx live reload — STORY-187 / ITERATION-255 (this slice provides only the re-point call site).
- The picker, `.lazyspec/` validation, and first-launch/valid/invalid-pick ACs — ITERATION-253 (consumed here).
- The in-process bridge, custom scheme, runtime, `web::server::router` — ITERATION-252 (consumed as-is).
- `cargo tauri build`, embedded assets, unsigned `.app` — STORY-188.
- Code signing / notarization; Windows/Linux; auth; any edit/write path; any change to routes/templates.

## Verification

- Open two valid projects, quit, relaunch → last project reopens without the picker; File > recents lists both, sourced from a file under the config-dir crate's path (not a hardcoded home path).
- Move/delete the remembered project, relaunch → picker appears, stale entry dropped, no broken view.
- File > Open Project… (and a recents entry) → webview shows the new project; header repo chip reflects the new `repo_name`/`branch`; the watcher re-point seam is invoked with the new root.