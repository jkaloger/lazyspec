---
title: App live reload via a web-reachable file watcher
type: story
status: in-progress
author: unknown
date: 2026-07-01
tags: []
related:
- implements: RFC-054
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

As a non-technical reviewer with the app open on a synced project folder, I want the view to reflect edits without me relaunching or manually refreshing, so that when a teammate saves a doc (or my sync tool pulls a change), the next page I open shows the current content.

RFC-054 story 3. This is the freshness half of the desktop app (RFC-054): the in-process bridge (STORY-185) serves the current `Arc<Store>`, and project selection (STORY-186) chooses which folder is open. Without a watcher, that store is a one-shot snapshot taken at load; every later disk edit is invisible until the process restarts, which defeats the "hand a reviewer a folder they already have" premise.

**This is new work, not reuse.** The RFC's design prose leans on "the `notify` watcher the TUI already uses", but that watcher does not exist in a form `web`/`app` can call. It lives in `src/tui/infra/event_loop.rs`: `notify::recommended_watcher` sends `AppEvent::FileChange(notify::Event)` (`src/tui/state/app.rs`), and every consumer (`rewatch`, `reload_session`, `handle_app_event`) mutates TUI `App` state. `web`/`app` cannot depend on `tui` (dependency principle 3: `app -> web -> engine`, never `web -> tui`). So the reload loop must be authored fresh in `web`, reachable by both `app` and hosted `serve`. Only the pure watch-set + rebuild logic is lifted into `engine` if genuinely shared with the TUI (precedent: RFC-052's `flatten_forest` lift); a coupling to `tui` is out of the question.

The existing `watch_paths(root, config)` helper in the TUI is a pure function (no `notify`, no `App`) and is the shared-logic candidate for an `engine` lift; the surrounding `notify` loop and `Arc<Store>` swap are new and live in `web`.

## Acceptance Criteria
<!-- guidance: one given/when/then per criterion; each independently verifiable -->

- **Given** the app has a project folder open and a document under `docs/` is rendered in the webview,
  **When** that document's `.md` file is edited on disk and saved,
  **Then** the next htmx poll or navigation renders the updated content, without relaunching the app.

- **Given** a running project,
  **When** the watcher observes a change under the opened folder's `docs/` or `.lazyspec/` (create, modify, or remove),
  **Then** `Store::load` runs against the project root and the router's shared `Arc<Store>` is atomically swapped to the freshly loaded store.

- **Given** an in-flight request is being serviced against the current store,
  **When** a reload swaps in a new `Arc<Store>` concurrently,
  **Then** the in-flight request completes against a consistent store snapshot and the swap is visible only to subsequent requests (no torn read, no lock held across a request).

- **Given** the watcher reload loop,
  **When** it is exercised in a `web`-layer test,
  **Then** it compiles and runs with no dependency on `crate::tui`, and does not reference `AppEvent` or the TUI `App` type. (Grounds the "new, not reuse" boundary.)

- **Given** the user switches to a different project (STORY-186),
  **When** the new store is loaded,
  **Then** the watcher stops observing the previous folder and re-points at the new folder's `docs/`/`.lazyspec/`, so subsequent edits to the new project drive reloads and edits to the old one do not.

- **Given** a change fires that yields an unloadable store (e.g. a mid-write file or a transiently invalid `.lazyspec.toml`),
  **When** the reload's `Store::load` returns `Err`,
  **Then** the swap is skipped, the previous `Arc<Store>` stays served, and the app keeps running (a failed reload never blanks or crashes the view).

- **Given** the pure watch-set helper (which paths under the root to observe for a given config),
  **When** it is unit-tested,
  **Then** it returns the expected set independent of `notify` and independent of any UI layer — matching the existing TUI `watch_paths` coverage so the lifted/shared logic is verified in one place.

- **Given** a project served by hosted `lazyspec serve` (not the desktop app),
  **When** a doc under the served folder is edited on disk,
  **Then** the same reload loop swaps the store and the served HTTP view reflects the edit on the next request — the watcher is wired into `serve`, not only the app.

## Scope

### In Scope
<!-- guidance: what this slice will deliver; keep it to one shippable increment -->

- A new `notify`-based reload loop authored in `web` (e.g. `web::watch`), reachable from both `app` and `serve`, with no `tui` dependency.
- Observing `docs/` and `.lazyspec/` under the opened project root; on a relevant change, `Store::load` + atomic `Arc<Store>` swap into the router's shared state.
- Re-pointing the watcher when the open project changes (integrates with STORY-186 project switching).
- Wiring the same loop into hosted `serve` so the HTTP path gains live reload.
- Lifting only the pure watch-set / rebuild logic into `engine` **if** it is genuinely shared with the TUI (following the `flatten_forest` precedent); otherwise authoring it in `web`.
- Failed-reload resilience: keep serving the last-good store on `Store::load` error.

### Out of Scope
<!-- guidance: what is deliberately deferred, so reviewers know the boundary -->

- Any `web -> tui` or `app -> tui` dependency, or reuse of `tui::infra::event_loop`'s watcher/`reload_session`/`AppEvent`. The TUI watcher is left exactly as it is; this slice does not refactor the TUI.
- Push/WebSocket/SSE live updates. Freshness is realized on the next htmx poll or navigation (RFC-054); no new push transport.
- Debounce/coalescing tuning, config-hot-reload semantics, or `.lazyspec.toml`-driven type-set changes beyond a straight `Store::load` (the TUI's richer `reload_session` behavior is not ported).
- Editing / write-back (inherited RFC-054 non-goal) and the folder picker / recents (STORY-186).
- Windows/Linux watcher specifics; macOS is the target surface for the app, though the loop is platform-neutral where `notify` allows.