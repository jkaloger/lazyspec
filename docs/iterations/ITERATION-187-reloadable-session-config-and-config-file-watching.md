---
title: Reloadable session config and config-file watching
type: iteration
status: draft
author: agent
date: 2026-06-19
tags: []
related:
- implements: STORY-138
---

## Changes

All work in `src/tui/infra/event_loop.rs` unless noted. Today `config: &Config` threaded immutably through `run` (sig line 239) + `handle_app_event` (line 121). Goal: own `Config` in `run`, single reload primitive, watch `.lazyspec.toml`, no settings UI.

### 1. Own session Config as reloadable state (AC1, AC2)

- File: `src/tui/infra/event_loop.rs`.
- `run` sig stays `pub fn run(store: Store, config: &Config) -> Result<()>` (caller `src/main.rs:479` unchanged). At top of `run` (after line 239, before `App::new`) shadow into owned mutable: `let mut config: Config = config.clone();`.
- Replace every `config` use in `run` body w/ `&config` where `&Config` expected (App::new line 256, `app.refresh_validation(config)` lines 262/501/523/548/563, `views::draw(..., config)` line 353, `handle_app_event(&mut app, event, &root, config)` lines 386/389, clone points lines 415/475/490/513-style threads). The thread-spawn clones already do `config.clone()` → keep as `config.clone()` (now clones owned val).
- `handle_app_event` sig (line 121) stays `&Config` — call sites pass `&config`.
- Reload reassigns `config` (owned) → all subsequent `&config` reads see new value. AC1/AC2 satisfied: reload re-parses `.lazyspec.toml` from disk → owned `config` becomes new active session value.
- Verify: `cargo build` clean; `config` is `Config` not `&Config` inside `run`.

### 2. Reload primitive fn (AC2, AC3, AC4, AC5, AC7, AC8)

- File: `src/tui/infra/event_loop.rs`. Add free fn (module-private):
  ```
  fn reload_session(
      app: &mut App,
      config: &mut Config,
      watcher: &mut notify::RecommendedWatcher,
      root: &Path,
      picker_protocol: terminal_caps::TerminalImageProtocol,
      tool_availability: ...,           // see note
  ) -> Result<()>
  ```
  Simplest viable signature: pass only what's needed to rebuild App-derived config state (see step 4). Final arg set chosen by builder; MUST allow App config-derived caches to refresh.
- Order (matches Scope IN + RFC-023):
  1. Re-load Config: `let new_config = Config::load(root, &crate::engine::fs::RealFileSystem)?;` (`src/engine/config.rs:621`, strict parse via `Config::parse`). On `Err` → return early, leave `*config`/`app`/`watcher` untouched (AC8).
  2. Rebuild Store: `let new_store = Store::load(root, &new_config)?;` (`src/engine/store.rs:38`). On `Err` → return early, prev Config/Store/watch set intact (AC8). NOTE: only commit ANY mutation AFTER both fallible ops succeed → compute `new_config` + `new_store` into locals first, then assign.
  3. Commit: `*config = new_config;` then `app.store = new_store;` then refresh App config-derived state (step 4) then `app.refresh_validation(config);`, `app.filtered_docs_cache = None;`, `app.rebuild_search_index();`, `app.build_doc_tree();`, `app.git_status_cache.invalidate();`, `app.expanded_body_cache.clear();`, `app.disk_cache.clear();` (mirror `CacheRefresh` line 163-172 + `GhPushResult` Ok arm lines 177-185).
  4. Re-establish watcher: call rewatch helper (step 3) against `&config` type dirs + `.lazyspec.toml`.
- AC7 redraw: no explicit "request redraw" flag — `run` loop redraws every iteration (`terminal.draw` line 353 each pass). Reload runs inside loop body → next `terminal.draw` renders new state. (If builder prefers explicit, add `app.needs_redraw = true` style flag, but loop-always-draws makes it a no-op; document choice.)
- Verify: unit test (Test Plan AC2/AC3/AC8) exercises locals-first commit ordering.

### 3. Watch-set computation as pure fn + rewatch helper (AC4, AC5)

- File: `src/tui/infra/event_loop.rs`. Extract watch-set as pure fn for testability:
  ```
  fn watch_paths(root: &Path, config: &Config) -> Vec<PathBuf>
  ```
  Returns: `.lazyspec.toml` (`root.join(".lazyspec.toml")`) PLUS each existing type dir `root.join(&t.dir)` for `t in config.documents.types` where `full.exists()`. Mirror existing dir loop lines 313-324 but ADD `.lazyspec.toml` + return computed set instead of watching inline. `.lazyspec.toml` always included (gate on `.exists()` too — always true in running session; AC5).
- Rewatch helper:
  ```
  fn rewatch(watcher: &mut notify::RecommendedWatcher, root: &Path, config: &Config) -> Result<()>
  ```
  Recreate watch set: `notify::RecommendedWatcher` has no "unwatch all" guarantee across reloads w/ changed dirs → cleanest = build a NEW watcher each reload and replace `*watcher`. So rewatch instead REPLACES watcher: construct fresh `notify::recommended_watcher(move |res| { ... send AppEvent::FileChange })?` (copy closure lines 307-311, needs `fs_tx`/`tx.clone()` captured — pass `tx: &Sender<AppEvent>` arg), watch each `watch_paths(root, config)` entry `NonRecursive`, assign `*watcher = new_watcher`. Dropping old watcher stops all its watches → AC4 "no longer watching dirs new Config omits" satisfied by replacement.
  Signature: `fn rewatch(watcher: &mut notify::RecommendedWatcher, root: &Path, config: &Config, tx: &crossbeam_channel::Sender<AppEvent>) -> Result<()>`.
- Startup: replace inline watcher dir loop (lines 313-324) w/ `rewatch(&mut _watcher, &root, &config, &tx)?;` so `.lazyspec.toml` watched from startup (AC5) AND startup + reload share one code path.
- Verify: `watch_paths` returns `.lazyspec.toml` + only existing type dirs; unit-testable (Test Plan AC4/AC5).

### 4. Refresh App config-derived caches on reload (AC1, AC3, AC7)

- File: `src/tui/state/app.rs`. App caches config-derived state at construction (`App::new` lines 311-348): `doc_types` (line 365-370), `type_icons` (318-330), `type_plurals` (331-336), `has_github_issues` (341-345), `status_bar_components`/`status_bar_warnings` (347-348). After reload these are STALE → new type set/icons/plurals not reflected → AC1/AC3 fail.
- Add method on `App`:
  ```
  pub fn apply_config(&mut self, config: &Config)
  ```
  Recompute `self.doc_types`, `self.type_icons`, `self.type_plurals`, `self.has_github_issues`, `(self.status_bar_components, self.status_bar_warnings)` from `config` (factor the exact expressions out of `App::new` lines 318-348 + 365-370; have `App::new` also call `apply_config` to avoid duplication, OR keep `App::new` inline + duplicate expressions in `apply_config` — prefer factoring). Clamp `self.selected_type` to `< self.doc_types.len()` (new config may have fewer types → avoid OOB at `app.rs:678` `&self.doc_types[self.selected_type]`).
- `reload_session` (step 2 item 3) calls `app.apply_config(config)` before `refresh_validation`.
- Verify: after `apply_config` w/ config that drops a type, `app.doc_types` excludes it + `selected_type` in bounds.

### 5. External `.lazyspec.toml` change → reload when clean (AC6)

- File: `src/tui/infra/event_loop.rs`, `handle_app_event` `AppEvent::FileChange` arm (lines 126-148).
- Watcher now emits `FileChange` for `.lazyspec.toml` (step 3). Detect: in the `event.paths` scan (loop line 129), check if any path == `root.join(".lazyspec.toml")` (canonicalize both or compare file_name `== ".lazyspec.toml"` — notify may emit absolute/canonical paths; compare `path.file_name() == Some(OsStr::new(".lazyspec.toml"))` AND under `root`). If matched: set a flag e.g. `let config_changed = true;`.
- `handle_app_event` currently has no access to `&mut Config`/`watcher` (it gets `config: &Config`). Two options — builder picks:
  - (a) Plumb `&mut Config` + `&mut RecommendedWatcher` + `&tx` into `handle_app_event` and call `reload_session` directly from the FileChange arm.
  - (b) Add `pub config_reload_request: bool` field to `App` (mirror `fix_request` idiom `app.rs:263`, `event_loop.rs:551`); FileChange arm sets `app.config_reload_request = true`; drain in `run` loop body (alongside `fix_request` block ~line 551) → there `&mut config` + `&mut _watcher` + `&tx` in scope → call `reload_session`. PREFER (b): keeps `handle_app_event` sig stable + matches existing drain idiom.
- "When buffer clean" (AC6): slice has no dirty buffer (Scope OUT slice 3) → buffer always clean here → unconditionally honor request. Leave TODO/comment noting slice 3 gates this on dirty buffer + adds keep/discard prompt. Do NOT implement dirty gate.
- Note: `.lazyspec.toml` is non-`.md` → current FileChange arm sets `has_non_md = true` (line 137) → clears caches + `refresh_validation`. Ensure config-change detection happens BEFORE/ALONGSIDE without breaking md-reload path; config reload (drain) supersedes the cache-clear (reload clears caches anyway).
- Verify: simulate FileChange w/ `.lazyspec.toml` path → `app.config_reload_request == true`.

### 6. Manual no-op reload trigger (validate end-to-end)

- File: `src/tui/views/keys.rs` (`handle_key`, sig line 11). Add a keybinding (pick unused key, e.g. `Ctrl+R` or `R` in a non-conflicting mode — builder checks for conflicts in `keys.rs`) that sets `self.config_reload_request = true` (same flag as step 5b). Drained in `run` → exercises `reload_session` end-to-end against unchanged `.lazyspec.toml` (no-op reload: re-parse same file → same Config → rebuild Store → rewatch → redraw).
- File: `README.md` — add the new keybinding to the TUI keybindings list (project rule: update README on CLI/interface change). Locate TUI keys section + add row.
- Verify: pressing key with unchanged config leaves session functional (no-op); pressing after editing `.lazyspec.toml` applies changes without restart (AC7).

### 7. App field + ctor default

- File: `src/tui/state/app.rs`. Add `pub config_reload_request: bool` to `App` struct (near `fix_request` line 263). Init `false` in `App::new` (near line 410) AND in the test ctor (the second struct literal ~line 1591-1616 that also sets `fix_request: false`).
- Verify: `cargo build` + existing app tests compile.

## Test Plan

Prefer testing the reload primitive decomposed into pure/unit-testable seams: (1) `watch_paths` pure fn, (2) `apply_config` App mutation, (3) commit-ordering of `reload_session` (Config re-parse + Store rebuild + failure rollback). notify-over-real-FS (AC5 actual event, AC6 end-to-end) is hard to unit-test deterministically — see AC5/AC6 notes.

- **AC1 (config is reloadable state):** Construct `App` from config A (types [rfc]). Write config B (types [rfc, story]) to a temp `.lazyspec.toml`. Run `reload_session` (or its decomposed steps: `Config::load` → `Store::load` → `app.apply_config`). Assert `app.doc_types` now reflects B (contains `story`) and a subsequent `views`-facing read uses B not A. Seam: `App` + `apply_config` post-reload, against tempdir root.
- **AC2 (re-load from disk):** `reload_session` with modified temp `.lazyspec.toml` → assert the committed `config` equals `Config::load(root)` of the new file (e.g. compare `config.documents.types` names/dirs). Seam: `Config::load` (`config.rs:621`) inside reload; assert owned `config` mutated.
- **AC3 (rebuild Store):** Config B adds a type whose `dir` contains a doc file (write fixture md). After reload assert `app.store` contains docs from the new dir (e.g. `store` doc count / type lookup reflects B). Seam: `Store::load` (`store.rs:38`) result assigned to `app.store`.
- **AC4 (rewatch over new type dirs — pure-fn):** Unit-test `watch_paths(root, config)`. Case: config with type dirs `docs/rfcs` (exists) + `docs/gone` (missing) → result contains `root/docs/rfcs`, excludes `root/docs/gone`. Case: reload from config with dir X to config without X → `watch_paths` for new config omits X (proves "no longer watching omitted dirs", paired w/ watcher-replacement note). Seam: pure fn, deterministic, no notify. (Actual notify rewatch covered by AC4 design via watcher replacement; assert at fn level.)
- **AC5 (`.lazyspec.toml` in watch set):** Unit-test `watch_paths` always includes `root.join(".lazyspec.toml")` for any config (startup + post-reload share this fn → both covered). Deterministic pure-fn assertion. NOTE tradeoff: asserting the computed watch SET (pure, isolated, deterministic) does NOT prove notify actually delivers a `.lazyspec.toml` event over real FS. Realistic coverage = optional integration test (below). Chosen property: isolated/deterministic via pure `watch_paths`; realism deferred to optional integration test.
- **AC6 (external change → reload when clean):** Two layers. (a) Unit: feed a synthetic `AppEvent::FileChange` whose `paths` include `root/.lazyspec.toml` into the detection logic → assert `app.config_reload_request == true` (clean buffer → unconditional). (b) Optional integration (call out as non-deterministic): real `notify` watcher over a tempdir, write `.lazyspec.toml`, await event w/ timeout, assert reload applied. Mark optional/flaky-prone; gate behind `#[ignore]` or an `integration` cfg if added. Primary deterministic assertion = (a).
- **AC7 (redraw against new state):** Loop redraws every iteration (`terminal.draw` line 353) → after `reload_session` mutates `app.store`/`config`/`doc_types`, the next draw reads new state by construction. Assert via AC1/AC3 (post-reload App state is new) — a headless draw assertion is unnecessary; document that redraw is guaranteed by always-draw loop, no restart needed.
- **AC8 (failure leaves session intact):** Two failure modes.
  - Parse error: temp `.lazyspec.toml` with invalid TOML (or missing `[[relationships]]` → strict `Config::parse` line 508 errors). Run `reload_session` → assert returns `Err`, and `config` + `app.store` + `app.doc_types` UNCHANGED (still config A). Proves locals-first commit ordering.
  - `Store::load` error: config that parses but whose `Store::load` fails (e.g. force error path) → assert `Err`, prev Config/Store retained. Seam: `reload_session` early-return before any `*config`/`app.store` mutation.

## Notes

- **Build dependency: NONE.** Infra slice — no upstream iteration. This is the foundation slice-3's save action calls (slice 3 invokes `reload_session` after writing `.lazyspec.toml`). Ships machinery + manual/external-change triggers only; NO settings-editing UI (Scope OUT slice 1) and NO dirty buffer / save / discard / conflict prompt (Scope OUT slice 3).
- **Key decision — own Config in `run`:** `run` sig kept `(&Config)` (caller `src/main.rs:479` unchanged); shadow to owned `let mut config: Config = config.clone();` at top. Minimal churn: only `&config` re-borrows + reload reassignment. Alternative (change `run` sig to take `Config` by value) rejected — larger blast radius, no benefit.
- **Key decision — watcher replacement over incremental unwatch:** reload constructs a FRESH `notify::RecommendedWatcher` and replaces `*watcher`; dropping the old one stops all prior watches. Avoids tracking per-path watch handles for unwatch. Satisfies AC4 "no longer watching omitted dirs" cleanly.
- **Key decision — reload trigger via `app.config_reload_request: bool` drained in run loop** (mirrors `fix_request` idiom: set in `keys.rs`/`handle_app_event`, drained near `event_loop.rs:551`). Keeps `handle_app_event` sig stable; gives reload access to `&mut config` + `&mut watcher` + `&tx` only at the drain site where they're in scope.
- **Key decision — commit ordering for AC8:** compute `new_config` + `new_store` into LOCALS; assign to `*config`/`app.store` only after BOTH fallible ops succeed. Any early `Err` leaves session untouched.
- **App config-derived caches:** `doc_types`/`type_icons`/`type_plurals`/`has_github_issues`/`status_bar_components` are computed once in `App::new` → reload MUST recompute via `apply_config` or new types/icons won't render. Also clamp `selected_type` to new `doc_types.len()` (fewer types → OOB guard at `app.rs:678`).
- **AC6 clean-buffer caveat:** no dirty buffer exists this slice → request honored unconditionally. Slice 3 adds the dirty gate + keep/discard prompt; leave a comment marker, do NOT implement.
- **Non-`.md` FileChange path:** `.lazyspec.toml` is non-md → existing arm sets `has_non_md` → clears caches (`event_loop.rs:140-143`); reload also clears caches so no conflict. Ensure config-path detection runs in same arm without regressing md `reload_file` path.
- **README:** new manual-reload keybinding must be added to README TUI keybindings (project rule).
