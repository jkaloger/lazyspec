---
title: 'ClickUp status colour: capture, cache, resolver'
type: iteration
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-201
- blocks: ITERATION-284
---## Changes

1. **Capture `color` on `ClickupStatus`** — `src/engine/clickup.rs:157` struct. Add field after `status_type` (line 162):
   ```rust
   #[serde(default)]
   pub color: String,
   ```
   API `GET /list/{id}` already sends `"color":"#hex"` per status → serde `default` → absent/empty colour deserializes to `""`, no failure. Fixture already carries it (clickup.rs:1225-1227). Update the two struct-literal constructions the new field breaks: test helper `status(name, orderindex, ty)` at `src/engine/clickup_cache.rs:562-568` and the literal at `src/engine/clickup.rs:1249-1253` → add `color: String::new()` (or a hex where a test asserts it).

2. **New engine module `src/engine/status_colors.rs`** — mirror `TaskMap` (`src/engine/task_map.rs`). Register in `src/engine.rs` (add `pub mod status_colors;` near line 38, alpha order beside `task_map`).
   - `const MAP_PATH: &str = ".lazyspec/status-colors.json";`
   - Type keyed `type_name -> { status_name -> hex }`:
     ```rust
     #[derive(Debug, Clone, Default, Serialize, Deserialize)]
     pub struct StatusColors {
         #[serde(flatten)]
         types: HashMap<String, HashMap<String, String>>,
     }
     ```
   - `load(root: &Path) -> Result<Self>` — missing file → empty (copy TaskMap:35-45).
   - `save(&self, root: &Path) -> Result<()>` — `create_dir_all` parent + `to_string_pretty` (copy TaskMap:47-55).
   - `set_type(&mut self, type_name: impl Into<String>, colors: HashMap<String,String>)` — replace one type's map (full-fetch-owns-type posture, matches `fetch_tasks`).
   - **Resolver** `get(&self, type_name: &str, status: &str) -> Option<&str>` — nested lookup → `(type,status) -> Option<hex>`. This is ITERATION-284's entry point. Engine-only (principle 3): only fs read/write, no presentation.

3. **Derive colours from the already-fetched statuses** — `src/engine/clickup_cache.rs`. `fetch_lifecycle` (110-119) + `derive_lifecycle` (126-133) currently drop colour. To avoid a 2nd `list_statuses` round-trip:
   - Add pure fn `derive_status_colors(statuses: &[ClickupStatus]) -> HashMap<String,String>` beside `derive_lifecycle` — map each `s.status.clone() -> s.color.clone()`, **skip empty colour** (so `get` miss → 284 falls back).
   - Add `pub fn fetch_lifecycle_and_colors(client, token, list_id) -> Result<(Lifecycle, HashMap<String,String>)>` — fetches `list_statuses` ONCE, returns `(derive_lifecycle(&s), derive_status_colors(&s))`. Keeps derivation in engine + single API call (principle 6). Keep `fetch_lifecycle` for existing callers/tests or replace — replacing is cleaner; check no other caller (`fetch.rs:217` is the only sync caller).

4. **Capture at sync** — `src/cli/fetch.rs` clickup branch (200-230).
   - Before loop: `let mut status_colors = StatusColors::load(root)?;` (new import).
   - At line 217 swap `fetch_lifecycle` → `fetch_lifecycle_and_colors`; push lifecycle as today, and `status_colors.set_type(type_name, colors)`.
   - After loop, beside `task_map.save(root)?` (228): `status_colors.save(root)?;`. Colours go to the cache artifact ONLY — do NOT thread into `persist_clickup_lifecycles` (229), which writes `.lazyspec.toml`.

5. **Gitignore** — `.gitignore` (already lists `.lazyspec/task-map.json` after `.lazyspec/cache/`). Add line:
   ```
   .lazyspec/status-colors.json
   ```

## Test Plan

Unit tests in `src/engine/status_colors.rs` `#[cfg(test)]` (mirror task_map tests, `tempfile::TempDir`):
- `load_missing_file_returns_empty` — no file → `get` any → `None`.
- `set_type_and_get` — set `"story" -> {"pending":"#f00"}`; `get("story","pending") == Some("#f00")`.
- **Cache round-trip** (AC7 — on-disk JSON artifact, readable programmatically): `set_type` two types → `save` → `load` → both resolve; assert `.lazyspec/status-colors.json` exists.
- **Resolver miss** (feeds AC6 fallback): `get` unknown type / unknown status → `None`.

Derivation tests in `src/engine/clickup_cache.rs` tests (reuse `status()` helper, now with colour; use `FakeClickupClient::with_statuses`):
- `derive_status_colors_maps_name_to_hex` — statuses w/ colours → map name→hex.
- `derive_status_colors_skips_empty_color` — status w/ `color:""` (serde default) omitted from map → resolver miss → 284 fallback.
- `fetch_lifecycle_and_colors_returns_both_from_one_fetch` — `FakeClickupClient::with_statuses(...)` → assert lifecycle states AND colour map; single `list_statuses` call.
- error path: reuse `failing_statuses` → `fetch_lifecycle_and_colors` propagates (`"fetching ClickUp list statuses"`).

Colour-capture-at-sync (AC1): assert colours land in cache artifact, NOT config. Either extend fetch.rs test coverage or a focused engine test: run `set_type`+`save`, assert JSON file written under `.lazyspec/` and `.lazyspec.toml` untouched by this path.

## Notes

- **No double API fetch** — reuse the `list_statuses` set already pulled for lifecycle; single fn returns `(Lifecycle, colours)`. Do NOT add a sibling `fetch_status_colors` that re-hits the API.
- **serde `default` on `color`** — absent/empty colour is valid; empty skipped at derive → resolver miss → renderers (284) fall back to hardcoded name→colour map. No crash (AC6).
- **Cache, never config** — colours persist to gitignored `.lazyspec/status-colors.json` only; keep them out of `persist_clickup_lifecycles` / `.lazyspec.toml` (AC1). Add to `.gitignore`.
- **Engine boundary** (principle 3) — `StatusColors` does fs read/write exactly like `TaskMap`, nothing more; resolver lives in engine so CLI/TUI/web depend on engine, never each other (principle 4). Reuse existing `FakeClickupClient` fake — no new test double.
- **Struct-literal breakage** — adding `color` breaks 2 explicit `ClickupStatus { .. }` literals (clickup_cache.rs:562, clickup.rs:1249); update both.
- **No new command** — story AC7 tightened: artifact is on-disk JSON only, no `--json` command surface required this slice.
- **Blocks ITERATION-284** (renderers TUI/CLI/web) — they consume `StatusColors::get(type,status) -> Option<hex>`; this slice ships engine plumbing + resolver only, no rendering.