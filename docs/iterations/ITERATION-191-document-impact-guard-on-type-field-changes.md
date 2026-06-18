---
title: Document-impact guard on type field changes
type: iteration
status: draft
author: agent
date: 2026-06-19
tags: []
related:
- implements: STORY-142
---

## Changes

Slice 6 of RFC-023. Inserts detection+confirm step INTO slice-3 (ITER-188 / STORY-139) save flow. Slice 3 owns dirty buffer + atomic `toml_edit` write + `w`/`Ctrl-S` write path; THIS slice only pauses that write when a load-bearing type-field change would orphan docs on disk, reports impact, requires explicit confirm. NEVER moves/renames/renumbers docs (not in RFC-023 — RFC-023:140 "edits config only").

Three `[[types]]` fields load-bearing for docs on disk (config.rs:133-152): `dir` (`String`, where type's docs live), `prefix` (`String`, ID prefix new docs inherit), `store` (`StoreBackend`, backend type points at). Changing any on a type w/ ≥1 existing doc silently orphans/desyncs — settings screen edits config only, files NOT touched.

Verified seams:
- Save method: ITER-188 Task 6 `settings_save(&mut self, root, ...) -> Result<()>` on `App` (`src/tui/state/app.rs`). Sequence: read `.lazyspec.toml` src → `write_config_in_place(&src, &self.buffer)` → validate (`Config::parse`) → `fs::write` ONCE → slice-2 reload. Guard inserts BETWEEN validate-OK and `fs::write`.
- Dirty buffer = `self.settings.buffer: Config` (ITER-188 Task 2); on-disk = session `Config` / `Config::load` (config.rs:621). Diff = match `buffer.documents.types` vs on-disk `config.documents.types` by `TypeDef.name`.
- Count docs of a type: `store.list(&Filter{ doc_type: Some(DocType::new(&type_def.name)), ..Default::default() })` (store.rs:116, Filter store.rs:20-25). `DocType::new` lowercases (document.rs:63); `DocMeta.doc_type`/`.path` (document.rs:186-188). Store on `App` = `self.store` (overlays.rs:314 `app.store`).
- `StoreBackend` Display (config.rs:123-131): `filesystem`/`github-issues`/`git-ref` — use for old/new value strings.
- Confirm-state model: mirror `DeleteConfirm` forms.rs:89-111. Key dispatch: mirror `handle_delete_confirm_key` keys.rs:79-87 (gated keys.rs:34-36 `if self.delete_confirm.active`). Overlay: mirror `draw_delete_confirm` overlays.rs:168-217.
- NOTE sibling slices ITER-186/187/188 may be stubs. Reference `SettingsState`, `self.settings.buffer`, `settings_save` BY NAME; do not assume concrete API beyond seam.

### Task 1 — Pure detection fn: diff buffer vs on-disk → list of impacted types
ACs: AC1 AC6.
Files: new `src/tui/state/settings_guard.rs` (+ `mod settings_guard;` in `src/tui/state/mod.rs`); OR co-locate in `src/tui/state/app.rs` if `settings` submodule layout prefers (follow slice-1 module layout once landed — pick the one matching ITER-186/188).
- Define `pub struct TypeFieldImpact { pub type_name: String, pub field: &'static str, pub old: String, pub new: String, pub affected_count: usize }` (`field` ∈ `"dir"`/`"prefix"`/`"store"`).
- `pub fn detect_type_field_impacts(buffer: &Config, on_disk: &Config, store: &Store) -> Vec<TypeFieldImpact>`:
  - For each `bt` in `buffer.documents.types`, find on-disk entry `dt` w/ `dt.name == bt.name` (match by name; entries w/ no on-disk match = new type, slice 5, skip — no existing docs possible).
  - For each load-bearing field where `bt` differs from `dt`, push ONE `TypeFieldImpact` per changed field: `dir` (`bt.dir != dt.dir`), `prefix` (`bt.prefix != dt.prefix`), `store` (`bt.store != dt.store` — compare via `to_string()`/Display config.rs:123).
  - `affected_count = store.list(&Filter{ doc_type: Some(DocType::new(&dt.name)), ..Default::default() }).len()` (count uses ON-DISK name — that's the type as currently loaded). (AC6: count attributed to the specific type whose field changed.)
  - EXCLUDE any impact where `affected_count == 0` (AC5: load-bearing change on zero-doc type → no guard).
  - `old`/`new`: render field value as string (`dir`/`prefix` = the `String`; `store` = `StoreBackend::to_string()`).
- Result is empty ⇒ no guard, save passes through (AC5). Non-empty ⇒ guard (AC1). >1 entry ⇒ per-type rows (AC6).
- PURE: takes `&Config` + `&Config` + `&Store`, no `App`, no terminal → directly unit-testable.
Verify: deferred to Test Plan (AC1/AC5/AC6 unit tests on this fn).

### Task 2 — Confirm-guard state on `App` (mirror `DeleteConfirm`)
ACs: AC2 AC3 AC4.
Files: `src/tui/state/forms.rs` (new struct); `src/tui/state/app.rs` (field on `App`, init).
- Add `pub struct SettingsImpactConfirm { pub active: bool, pub impacts: Vec<TypeFieldImpact> }` to forms.rs (mirror `DeleteConfirm` forms.rs:89-111 incl `Default`/`new()`; `active=false`, `impacts=Vec::new()`).
- Add `pub settings_impact_confirm: SettingsImpactConfirm` field to `App` struct + init in `App::new`/`Default` (mirror existing `delete_confirm` wiring).
- The pending dirty buffer stays where slice 3 keeps it (`self.settings.buffer`) — guard does NOT copy the buffer; on confirm it re-runs the same write against the live buffer (AC3); on cancel buffer is left untouched (AC4). Guard holds only the computed `impacts` for rendering.
Verify: deferred to Test Plan.

### Task 3 — Wire guard into `settings_save`: pause before write, commit on confirm, preserve on cancel
ACs: AC1 AC3 AC4 AC5.
Files: `src/tui/state/app.rs`.
- Refactor slice-3 `settings_save` so the validated-and-rendered write is reachable as a private step. Concretely extract `fn settings_commit_write(&mut self, root) -> Result<()>` = ITER-188 Task 6 steps 1-5 (read src → `write_config_in_place` → validate → `fs::write` ONCE → reload + `dirty=false`). This is the unchanged slice-3 write (AC3 "exactly as slice 3 writes"; AC5 "identical to slice-3").
- Rewrite `settings_save(&mut self, root, config_on_disk: &Config) -> Result<()>` to gate on detection:
  1. `let impacts = detect_type_field_impacts(&self.settings.buffer, config_on_disk, &self.store)` (Task 1). `config_on_disk` = the currently-loaded session `Config` (what docs were enumerated against). [If slice 2 reload makes session `Config` owned on `App`, read from there instead of param — follow slice-2 seam.]
  2. If `impacts.is_empty()` ⇒ no load-bearing orphaning change (or all affected types have 0 docs) ⇒ `return self.settings_commit_write(root)` — pass through, no guard (AC5).
  3. Else ⇒ `self.settings_impact_confirm.impacts = impacts; self.settings_impact_confirm.active = true;` and RETURN WITHOUT WRITING (AC1: save pauses, confirmation guard shown instead of committing). `.lazyspec.toml` untouched, `self.settings.buffer` retains pending edits.
- `pub fn confirm_settings_impact(&mut self, root) -> Result<()>`: `self.settings_impact_confirm.active = false`; clear `.impacts`; `self.settings_commit_write(root)` (AC3: explicit confirm ⇒ buffer committed atomically exactly as slice 3; no doc files moved — `settings_commit_write` only `fs::write`s `.lazyspec.toml`). NOTE: if slice-3 validation here fails it surfaces slice-3's footer error (AC9 of ITER-188) — pass through unchanged.
- `pub fn cancel_settings_impact(&mut self)`: `self.settings_impact_confirm.active = false`; clear `.impacts`; NO write; `self.settings.buffer` + `self.settings.dirty` untouched so user can amend/discard (AC4).
- AC3 doc-file guarantee: `settings_commit_write` touches ONLY `root.join(".lazyspec.toml")` — assert in Notes no file-move call exists anywhere in this slice.
Verify: deferred to Test Plan (App-state show/confirm/cancel).

### Task 4 — Key dispatch for guard (mirror `handle_delete_confirm_key`)
ACs: AC1 AC3 AC4.
Files: `src/tui/views/keys.rs`.
- In `handle_key` (keys.rs:11-57), add gate BEFORE `handle_normal_key` (alongside keys.rs:34-36 `delete_confirm` gate): `if self.settings_impact_confirm.active { return self.handle_settings_impact_key(code, root, config); }`. Place high enough it intercepts all keys while active (it's an overlay-style modal).
- `fn handle_settings_impact_key(&mut self, code, root, config)` (mirror keys.rs:79-87):
  - `KeyCode::Enter` (or `KeyCode::Char('y')`) ⇒ `let _ = self.confirm_settings_impact(root);` (AC3).
  - `KeyCode::Esc` (or `KeyCode::Char('n')`) ⇒ `self.cancel_settings_impact();` (AC4).
  - `_ => {}`.
- This sits in the settings-screen save path: slice-3 `w`/`Ctrl-S` calls `settings_save` (Task 3) → if guard activates, subsequent keys route here until confirm/cancel.
Verify: deferred to Test Plan.

### Task 5 — Guard overlay render: per-type affected count + field + old/new + plain consequence
ACs: AC2 AC6.
Files: `src/tui/views/overlays.rs` (new `draw_settings_impact_confirm`); render-dispatch site `src/tui/views.rs` (overlay-stacking block — slice-3 ITER-188 Task 7 referenced views.rs:211-242; add arm `if app.settings_impact_confirm.active { draw_settings_impact_confirm(f, app); }` alongside `draw_delete_confirm`).
- `pub fn draw_settings_impact_confirm(f: &mut Frame, app: &App)` (mirror `draw_delete_confirm` overlays.rs:168-217; red/yellow border; centered popup; height scales w/ `impacts.len()`).
- Per `TypeFieldImpact` in `app.settings_impact_confirm.impacts`, render a block stating (AC2 + AC6 — one block per changed type, attributed to its `type_name`):
  - field changed + old→new: e.g. `"<type_name>.<field>: <old> → <new>"` (uses `.field`/`.old`/`.new`).
  - affected count + plain-language consequence: e.g. `"<affected_count> documents in <old-dir-or-prefix-or-store> will no longer be found; files are not moved"` (RFC-023:140 wording). For `dir` change phrase as RFC example ("12 documents in `docs/rfcs` will no longer be found; files are not moved"). For `prefix`/`store` adapt consequence ("… will no longer be found" / "points at a different backend"); keep "files are not moved".
- Footer line: `"[Enter/y: confirm write]  [Esc/n: cancel]"` (mirror delete overlay footer overlays.rs:204-207).
- AC6: when `impacts.len() > 1`, EACH type renders its own count+field+consequence block, each labeled w/ its `type_name`.
Verify: pure label/consequence string builder factored out for unit test (see Test Plan AC2/AC6); render fn itself not unit-tested (frame-dependent).

## Test Plan

One entry per AC. Detection fn (Task 1) is pure → unit-tested directly (on-disk `Config` + dirty `buffer` `Config` + `Store` → `Vec<TypeFieldImpact>`). Guard show/confirm/cancel tested App-state w/o terminal. AC1/AC6 affected counts via `Store::list` type-filtered. Build a `Store` over a `tempfile::tempdir()` w/ on-disk docs (or use `Store::load_with_fs` w/ a fake `FileSystem`, store.rs:43) so `store.list(&Filter{doc_type:Some(...)})` returns real counts.

- **AC1 (guard triggers on load-bearing change w/ existing docs)**: unit + App-state. (a) Build on-disk `Config` w/ type `rfc` (`dir="docs/rfcs"`), Store w/ ≥1 `rfc` doc on disk (`store.list(&Filter{doc_type:Some(DocType::new("rfc")),..})` len ≥1). Clone to `buffer`, change `rfc.dir` to `"docs/proposals"`. `detect_type_field_impacts(&buffer,&on_disk,&store)` ⇒ exactly 1 `TypeFieldImpact{type_name:"rfc", field:"dir", old:"docs/rfcs", new:"docs/proposals", affected_count:N>0}`. (b) App-state: `settings_save` w/ that buffer ⇒ `settings_impact_confirm.active==true`, NO write (assert temp `.lazyspec.toml` content unchanged / mtime). Seam: pure `detect_type_field_impacts` + `settings_save`. Property: asserts guard shown INSTEAD of write.
- **AC2 (guard reports affected docs + consequence)**: unit on the pure label/consequence builder (factored Task 5). Given a `TypeFieldImpact{type_name:"rfc",field:"dir",old:"docs/rfcs",new:"docs/proposals",affected_count:12}`, assert produced strings (i) name field changed w/ old+new (`contains("docs/rfcs")` && `contains("docs/proposals")`), (ii) state count (`contains("12")`), (iii) plain-language consequence (`contains("no longer be found")` && `contains("not moved")`). Seam: pure string builder. Property: asserts all three required facts present (count, field old/new, consequence).
- **AC3 (confirming writes changed config)**: App-state + temp file. Trigger guard (AC1 setup). Snapshot original `.lazyspec.toml` + the on-disk doc file paths/names. Call `confirm_settings_impact(root)`. Assert: `.lazyspec.toml` now contains the new `dir` value (written atomically via slice-3 `settings_commit_write`); `settings_impact_confirm.active==false`; `settings.dirty==false`; AND every on-disk doc file still exists at its ORIGINAL path w/ original name (no move/rename/renumber). Seam: `confirm_settings_impact` → `settings_commit_write`. Property: doc-files-untouched is the load-bearing assertion (AC3 "no document files moved/renamed/renumbered").
- **AC4 (cancelling preserves buffer + config)**: App-state + temp file. Trigger guard (AC1 setup) w/ `settings.buffer` holding pending `rfc.dir` edit + `dirty==true`. Snapshot `.lazyspec.toml` content. Call `cancel_settings_impact()`. Assert: `settings_impact_confirm.active==false`; `.lazyspec.toml` content byte-identical to snapshot (NO write); `settings.buffer`'s `rfc.dir` still the pending value; `settings.dirty==true` (user can amend/discard). Seam: `cancel_settings_impact`. Property: zero-write + buffer-retained.
- **AC5 (non-load-bearing changes save w/o guard)**: unit + App-state, two cases. (a) on-disk `rfc` w/ docs; buffer changes only `rfc.icon` (or `plural`) ⇒ `detect_type_field_impacts` returns EMPTY (non-load-bearing fields ignored). (b) on-disk type `note` w/ ZERO docs on disk (`store.list(&Filter{doc_type:Some(DocType::new("note")),..}).len()==0`); buffer changes `note.dir` ⇒ `detect_type_field_impacts` returns EMPTY (load-bearing but zero affected). App-state: `settings_save` for either ⇒ `settings_impact_confirm.active==false` AND `.lazyspec.toml` written once (committed identical to slice-3, assert new value present). Seam: pure fn + `settings_save`. Property: both no-guard paths (non-load-bearing field; zero-doc type) pass straight through.
- **AC6 (guard scopes affected counts per changed type)**: unit on detection fn. on-disk `Config` w/ types `rfc` (≥1 doc, e.g. 12) AND `story` (≥1 doc, e.g. 5); Store w/ those docs on disk. buffer changes `rfc.dir` AND `story.prefix` (load-bearing on BOTH). `detect_type_field_impacts` ⇒ exactly 2 `TypeFieldImpact`: one `{type_name:"rfc",field:"dir",affected_count:12}`, one `{type_name:"story",field:"prefix",affected_count:5}`. Assert each count attributed to the correct `type_name`+`field` (rfc count != story count, each matches its own `store.list` type-filtered len). Seam: pure `detect_type_field_impacts` w/ multi-type Store. Property: per-type attribution (count tied to the specific type whose field changed).

## Notes

- Build dep: **STORY-139 / ITERATION-188** (slice 3) — provides scalar editing + atomic `toml_edit` save (`write_config_in_place`) + dirty buffer (`SettingsState.buffer: Config`) + `w`/`Ctrl-S` write path (`settings_save`). THIS slice only inserts a confirm step: it refactors the write into `settings_commit_write` (the unchanged slice-3 write) and gates `settings_save` on `detect_type_field_impacts`. Also depends transitively on **ITER-186** (slice 1, `ViewMode::Settings` + `SettingsState`) and **ITER-187** (slice 2, reloadable session `Config`). All three were TODO stubs when this was authored, so `SettingsState` field names + `settings_save` signature follow those slices once landed; tasks describe the seam, not their internals.
- Key decision 1 — **detection is pure** (`fn detect_type_field_impacts(buffer, on_disk, store) -> Vec<TypeFieldImpact>`), no `App`/terminal, so AC1/AC5/AC6 unit-test the diff+count logic directly. App-state only wires show/confirm/cancel.
- Key decision 2 — **affected counts via `Store::list` type-filtered** (store.rs:116): `store.list(&Filter{doc_type:Some(DocType::new(&type_def.name)),..Default::default()}).len()`, using the ON-DISK type name (docs were loaded against the current config). `DocType::new` lowercases (document.rs:63); `TypeDef.name` matches by name across buffer↔on-disk.
- Key decision 3 — **zero-doc load-bearing changes are excluded at detection** (`affected_count==0` filtered out in Task 1), so AC5's "load-bearing field on a type w/ zero docs" naturally yields an empty impact list ⇒ pass-through, identical to slice-3. No separate App-side check needed.
- Key decision 4 — guard holds ONLY computed `impacts` for rendering (mirror `DeleteConfirm`); it does NOT snapshot the buffer. Confirm re-runs the live slice-3 write against `self.settings.buffer` (AC3); cancel leaves buffer+dirty untouched (AC4).
- **OUT (explicit, RFC-023:140)**: migrating/moving/renaming/renumbering docs to match new field values — NOT part of RFC-023 at all; the user performs any file migration. `settings_commit_write` touches only `.lazyspec.toml`; no file-move/rename/renumber call exists in this slice (asserted AC3 test). Also out: scalar edit + atomic save machinery (slice 3, reused); read-only view (slice 1); reload after external change (slice 2); add/delete collection entries (slice 5).
- After a confirmed write, slice-3's reload runs validation and surfaces any new doc warnings/errors inline (RFC-023:140) — that surfacing is slice-3/slice-2 behaviour reused unchanged, not re-implemented here.
