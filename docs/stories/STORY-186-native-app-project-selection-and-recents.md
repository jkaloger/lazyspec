---
title: Native app project selection and recents
type: story
status: in-progress
author: unknown
date: 2026-07-01
tags: []
related:
- implements: RFC-054
---## Value

As a non-technical macOS collaborator with no terminal and no git, I want to pick which lazyspec project the app shows me — from a normal folder picker or a list of ones I opened before — so that I can open and switch between projects myself without anyone running a command for me.

Story #2 of RFC-054. Depends on story #1 (the in-process bridge), which already renders a single hardcoded project through the axum router in the webview. This story removes the hardcoded path: the collaborator chooses the project, gets told plainly when a folder is not a lazyspec project, and can jump between recent projects. The rendered surface is unchanged (RFC-052's list + doc page + relationship tree); this is about *which* project feeds the view and *how* the user selects it.

## Scope

- On launch, decide the starting project: reopen the most recent valid remembered project if one exists, otherwise show the native macOS folder picker (Tauri dialog API).
- Validate the chosen folder contains a `.lazyspec/` project root. If it does not, tell the user in plain language and re-prompt; never open a broken view.
- Persist a recents list in the platform config directory (`~/Library/Application Support/lazyspec/`, resolved via a config-dir crate — not hardcoded).
- A native File menu with **Open Project…** (opens the picker) and a **recents** submenu listing previously opened projects.
- Switching projects (via picker or recents) rebuilds the `AppState` — reloads the `Store` from the new root and re-derives `config`, `coords`, `issue_map`, `repo_name`, `branch` — swaps it behind the router, and re-points the watcher at the new root. (Watcher authoring itself is story #3; this story provides the re-point seam it needs, or a documented no-op stub if #3 lands after.)

Layering: `app -> web -> engine` only. Project selection, validation, recents, and menu live in `app::project` / the `app` shell; `Store::load` and `AppState` construction are consumed from `web`/`engine` verbatim — no new rendering path, no `app -> tui`/`cli`.

## Acceptance Criteria

**First launch, no remembered project — picker shown**
- Given the app has never opened a project (no recents on disk), When I launch the app, Then a native macOS folder picker appears before any document view.

**Pick a valid project**
- Given the picker is open, When I choose a folder that contains a `.lazyspec/` root, Then the app loads that project and renders `GET /` (the RFC-052 list view) in the webview, and the folder is added to the recents list.

**Pick an invalid folder — plain-language rejection and re-prompt**
- Given the picker is open, When I choose a folder that does **not** contain a `.lazyspec/` root, Then the app shows a plain-language message that the folder is not a lazyspec project (no stack trace, no path-jargon-only error) and re-prompts with the picker, and no document view is opened and the folder is **not** added to recents.

**Remembered project reopens on next launch**
- Given I previously opened a valid project and closed the app, When I launch the app again, Then it reopens that project directly without showing the picker.

**Remembered project is now missing/moved**
- Given the most-recent remembered project path no longer exists or no longer contains a `.lazyspec/` root, When I launch the app, Then the app does not error out on it — it falls back to the picker (and the stale entry does not silently render an empty/broken view).

**Recents persist across restarts in the platform config dir**
- Given I have opened two or more valid projects, When I quit and relaunch, Then the File > recents submenu still lists them, sourced from a file under `~/Library/Application Support/lazyspec/` (resolved via the config-dir crate, not a hardcoded home path).

**Open a different project from the File menu**
- Given a project is open, When I choose File > Open Project… and select a different valid project, Then the webview reloads showing the new project's documents (the header repo chip reflects the new `repo_name`/`branch`), and the new project is recorded in recents.

**Switch via recents submenu**
- Given a project is open and recents contains another valid project, When I choose that entry from File > recents, Then the app switches to it exactly as an Open Project selection would (store reloaded, `AppState` swapped, view reloaded).

**Switch re-points the watcher**
- Given a project is open and being watched, When I switch to another project, Then the watcher is re-pointed at the new root so subsequent live-reload observes the new project's files, not the old one's. (Verifiable once story #3 lands; until then, the re-point call site exists and is exercised.)

## Non-goals / Notes (inherited from RFC-054)

- No editing/write-back; GitHub deep-links continue to click out to github.com. Selection changes the data source, not the write path.
- No authentication — the webview is in-process, single-user, no bound port.
- macOS only; the config-dir crate keeps the path resolution portable but nothing here targets other platforms.
- Watcher **authoring** and htmx live-reload behaviour are story #3; this story only guarantees the re-point seam on switch.