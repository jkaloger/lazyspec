---
title: Reload the TUI and web view on an extended config edit
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-284
- blocks: ITERATION-446
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Objective

The TUI and the web view watch the *extended* `.lazyspec.toml` and the extended doc dirs; an edit to either triggers the reload a local edit does today, and an edited external document re-reads in place.

## Satisfies

STORY-284 AC12. AC13 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-284. Contract: RFC-072 Goals (one-liner config; docs from the extended root). Depends on ITERATION-442's `doc_root` in `watch_paths`.
- **Watch set** (`src/engine/watch.rs:14-27`): after `root.join(".lazyspec.toml")` at `:16-19`, push `e.root.join(".lazyspec.toml")` for `config.extends` when it exists. The type-dir loop already goes through `doc_root` since ITERATION-442. Both frontends read this one function: TUI `rewatch` (`src/tui/infra/event_loop.rs:450-467`, at startup `:864`, after reload `:509`) and web `watch_with` (`src/web/watch.rs:105`).
- **TUI `FileChange` arm** (`event_loop.rs:519-547`). Two fixes. (1) `config_path` at `:522` is the local file only; the compare at `:537` must also match the extended one -- compute both up front, `path == &config_path || Some(path) == extended_path.as_ref()`. `reload_session` (`:485-510`) then re-runs `Config::load(root)` which re-resolves `extends`, so the new extended types land (`:493`). (2) `:525` `path.strip_prefix(root)` skips any `.md` outside the local root, so an edit to a doc under the extended root never reaches `reload_file` (`:526`). STORY-283 made external doc paths absolute in `meta.path` (`src/engine/store/loader.rs:75`), so pass `path.strip_prefix(root).unwrap_or(path)` and the same key to `expanded_body_cache.remove` / `disk_cache.invalidate` (`:527-528`).
- **Web** is free after the watch set: `reload_and_swap` (`watch.rs:38`) rebuilds `Store::load(root, config)` with the captured `worker_config` (`:110`). It never re-reads config today for a local edit either, so parity is "the extended config and dirs are watched and a doc edit swaps the store". A config-edit reload in the web view is pre-existing scope, not this story's.
- **URL `extends`** edits arrive by `fetch` (ITERATION-444) rewriting `.lazyspec/cache/config/.lazyspec.toml`; the watcher sees it as a file change and the same arm fires. Nothing extra.
- Test shapes: `event_loop.rs:1397` `file_change_on_lazyspec_toml_requests_reload`; `watch.rs:51`, `:67`; `src/web/watch.rs:133-141` `write_project`, `:241` the poll-watch swap test with `start_poll_watch` (`:252`).

## Tasks

1. Test-first, `watch.rs` tests: `config.extends = Some(<shared>)` with `<shared>/.lazyspec.toml` present -> `watch_paths` contains both config paths and `<shared>/docs/<dir>`, not `<root>/docs/<dir>`.
2. Test-first, `event_loop.rs` tests beside `:1397`: (a) a `FileChange` on `<shared>/.lazyspec.toml` sets `config_reload_request`; (b) a `FileChange` `Modify` on `<shared>/docs/rfcs/RFC-001-x.md` after the file's title changed -> `app.store` reports the new title (`reload_file` ran with the absolute path); (c) a local `.md` edit still reloads, pinning the `unwrap_or` did not break the relative case.
3. `watch_paths`, the two `FileChange` fixes. Green.
4. Test-first, `web/watch.rs` beside `:241`: project B extending `<shared>`; edit a doc under `<shared>/docs/<dir>` -> the shared store swaps to the new title within the existing test's timeout shape.
5. Run the TUI by hand once (task list Verification); `clippy -D warnings`.

## Out of scope

- The web view re-reading `.lazyspec.toml` on a config edit (local or extended); it does not today.
- The TUI settings pane editing the extended config: saves go through `Config::parse` read-back and refuse under `extends` (ITERATION-441). A "this project extends X" banner in the settings pane is a nice-to-have, not an AC.
- Watching the config clone's remote; that is `fetch` (ITERATION-444).
- AC13 -> ITERATION-446.

## Principles/conventions

`cargo run -q -- convention`. TUI Patterns / Event Loop: one watcher, one channel, no second event source. Principle 3: the watch set is engine code both frontends consume. DICTUM-004: `PollWatcher` in tests as `:252` does, no sleeps beyond the existing timeout idiom.

## Verification

ITERATION-441's scratch dir extending this repo: `cargo run -q` opens the TUI listing this repo's docs. In another shell, `touch`-edit a title in `docs/rfcs/RFC-072-*.md`: the TUI row updates. Edit this repo's `.lazyspec.toml` (add a blank line): the TUI reloads. `cargo run -q -- serve` in the scratch dir, edit an RFC here, reload the browser: new title.
