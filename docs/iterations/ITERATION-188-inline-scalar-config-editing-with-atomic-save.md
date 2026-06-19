---
title: Inline scalar config editing with atomic save
type: iteration
status: accepted
author: agent
date: 2026-06-19
tags: []
related:
- implements: STORY-139
---

## Changes

Slice 3 of RFC-023. Builds on slice 1 (read-only settings view + `ViewMode::Settings` + j/k field nav + right-panel render) and slice 2 (reloadable session `Config` + `.lazyspec.toml` in watch set). Adds inline scalar editing into in-memory dirty `Config` buffer + atomic `toml_edit` write-back. All settings-screen state assumed on a `SettingsState` struct owned by `App` (delivered slice 1); this slice extends it w/ edit-mode + dirty buffer. Verified paths: editor field model + state `src/tui/state/forms.rs`; key dispatch `src/tui/views/keys.rs` (mirror `handle_create_form_key` keys.rs:59-77 + `form_type_char`); render dispatch `src/tui/views.rs:173-205` + overlay stacking views.rs:211-242; config `src/engine/config.rs`; cross-field validation `Config::parse` config.rs:520-592; current write path `src/cli/init.rs:54` (`config.to_toml()` — do NOT reuse for in-place edit). NO `toml_edit` dep today (only `toml = "0.8"`, `tempfile = "3"` — Cargo.toml:22,39).

### Task 1 — Add `toml_edit` dep + in-place format/comment-preserving writer
ACs: AC8 (enables atomic single write).
Files: `Cargo.toml`; new `src/engine/config_write.rs` (+ `mod config_write;` in `src/engine/mod.rs`).
- Cargo.toml: add `toml_edit = "0.22"` to `[dependencies]` (alongside `toml = "0.8"` line 22). toml + toml_edit coexist; serde-read stays `toml`, in-place edit uses `toml_edit`.
- `config_write.rs`: `pub fn write_config_in_place(existing_src: &str, buffer: &Config) -> Result<String>`. Parse `existing_src` into `toml_edit::DocumentMut` (`existing_src.parse::<DocumentMut>()?`) → mutate ONLY the keys/tables the buffer differs on → return `doc.to_string()`. MUST NOT serialize whole `Config` via `to_toml`/`to_string_pretty` (would drop comments + reorder). Apply scalar edits into existing items so doc tree (formatting, comments, key order, blank lines) survives:
  - top-level: `naming.pattern` (`doc["naming"]["pattern"]`), `ref_count_ceiling`, `templates.dir`.
  - `[[types]]` entries: index the `types` array-of-tables, set per-entry scalar keys (`numbering`,`subdirectory`,`store`,`singleton`,`parent_type`,`agents`,`name`,`plural`,`dir`,`prefix`,`icon`). enum→string value (`numbering="sqids"`, `store="github-issues"` etc, matching serde renames config.rs:36-41/113-121); bool→bool; `agents: Vec<String>`→toml array; `parent_type: Option<String>`→remove key when None else string.
  - `[tui]`/`[tui.statusbar]`/`[tui.multiline]`: `ascii_diagrams`,`statusbar.enabled`,`multiline.max_expanded_height`.
  - `[numbering.sqids]` `salt`/`min_length`; `[numbering.reserved]` `remote`/`format`/`max_retries`; `[github]` `repo`(Option→insert/remove)/`cache_ttl`; `[coordination]` `remote`/`lease_duration`/`grace_period`/`max_push_retries`/`max_clock_skew`; `[certification]` `normalize`; `[agents]` `interactive`(Option); `[[rules]]` `severity`/`shape` (string).
- Helper `fn set_scalar(table: &mut toml_edit::Item, key: &str, value: toml_edit::Value)` that preserves any existing decor (prefix/suffix whitespace) when overwriting via `as_value_mut`.
- Implementation note for build: drive mutation FROM the buffer field set the editor model can touch (Task 2 enumerates the editable fields w/ a path each), so writer + editor share one field-path vocabulary rather than hand-walking every section.
Verify: unit test in `config_write.rs` — given a `.lazyspec.toml` src w/ a `# comment` line + custom key spacing, write a single scalar edit, assert returned string still contains the comment + only the one value changed + re-`Config::parse` succeeds.

### Task 2 — Editable field model + per-type editor enum on settings state
ACs: AC1 AC2 AC3 AC4 AC5 AC6 AC7 (defines the editor each field type uses).
Files: `src/tui/state/forms.rs` (new types); `src/tui/state/app.rs` (extend `SettingsState` from slice 1 if not already carrying these).
- Add `pub enum FieldEditor` in forms.rs enumerating the scalar editor kinds (mirrors RFC-023 editor table):
  - `Text` — free `String` (e.g. `naming.pattern`, type `dir`). (AC1)
  - `Toggle` — `bool`, flips on `Space`, no edit mode (e.g. `tui.statusbar.enabled`, type `subdirectory`, `certification.normalize`, `tui.ascii_diagrams`). (AC2)
  - `BoundedNum { min: u64, max: u64 }` — `u8`/`usize`, range-checked as typed (e.g. `sqids.min_length` 1..=10, `ref_count_ceiling`). (AC3)
  - `Nullable` — `Option<String>`, empty confirm = None else Some (e.g. `github.repo`, type `parent_type`, `agents.interactive`, relationship `inverse`). (AC4)
  - `Duration` — duration string, validated via `crate::engine::lease::parse_duration` (lease.rs:18) on confirm (e.g. `coordination.lease_duration`,`grace_period`,`max_clock_skew`). (AC5)
  - `List` — `Vec<String>`, comma-separated round-trip, trimmed, entered order, empty = `[]` (e.g. type `agents`, `statusbar.left`). (AC6)
  - `EnumCycle { variants: &'static [&'static str], idx: usize }` — cycles declared variants on `Space`, wraps to first (AC7). Variant sets (exact serde strings):
    - `numbering`: `["incremental","sqids","reserved"]` (config.rs:36-41).
    - `store`: `["filesystem","github-issues","git-ref"]` (config.rs:113-121).
    - reserved `format`: `["incremental","sqids"]` (config.rs:54-59).
    - rule `severity`: `["error","warning"]` (config.rs:6-11).
    - rule `shape`: `["parent-child","relation-existence"]` (config.rs:13-32).
- Add `pub struct EditableField { pub label: String, pub path: FieldPath, pub editor: FieldEditor }` where `FieldPath` identifies the buffer target Task 1's writer mutates (section + optional collection index + key). Slice 1 already builds the read-only field list per category/entry; this attaches the matching `FieldEditor` + `FieldPath` to each.
- Extend settings state on `App` (call it `SettingsState`, slice 1 owns it) with: `pub buffer: Config` (the in-memory dirty buffer, init = clone of session `Config`), `pub dirty: bool`, `pub editing: bool` (edit mode active on focused field), `pub edit_input: String` (text-entry scratch for Text/BoundedNum/Nullable/Duration/List), `pub edit_error: Option<String>` (field-level reject msg), `pub footer_error: Option<String>` (save-time cross-field msg, AC9).
Verify: unit-construct `SettingsState`, assert `buffer` equals session config clone, `dirty == false`, and each demo field maps to the expected `FieldEditor` variant (e.g. `sqids.min_length` → `BoundedNum{min:1,max:10}`).

### Task 3 — Enter/Esc/Enter text-entry editors (Text, Nullable, List) confirming into buffer
ACs: AC1 AC4 AC6.
Files: `src/tui/state/app.rs` (settings methods); `src/tui/views/keys.rs` (new `handle_settings_key` branch).
- keys.rs: add `ViewMode::Settings => return self.handle_settings_key(code, modifiers, root, config)` arm in `handle_normal_key` match keys.rs:645-651 (next to Filters/Graph/Agents). `handle_settings_key` dispatch (mirror `handle_create_form_key` keys.rs:59-77):
  - when `editing == false`: `j`/`k` move focused field (slice 1), `Enter` → `settings_start_edit()` (only for editor kinds w/ edit mode: Text/BoundedNum/Nullable/Duration/List), `Space` → Task 4 toggle/cycle, `w` or `Ctrl-S` → Task 6 save, `q`/`Esc` → Task 7 quit-guard, `` ` `` → cycle_mode.
  - when `editing == true`: `Esc` → `settings_cancel_edit()` (drop `edit_input`+`edit_error`, `editing=false`, buffer untouched), `Enter` → `settings_confirm_edit()`, `Backspace` → pop `edit_input`, `Char(c)` → push to `edit_input` + clear `edit_error`.
- `settings_start_edit(&mut self)`: set `editing=true`, seed `edit_input` from the focused field's current buffer value rendered as string (Nullable None → empty string, List → comma-joined).
- `settings_confirm_edit(&mut self)`: branch on focused `FieldEditor`:
  - `Text` → write trimmed-as-typed `String` into buffer at `FieldPath`, set `dirty=true`, `editing=false`. (AC1)
  - `Nullable` → if `edit_input.trim().is_empty()` write `None` else `Some(trimmed)`, `dirty=true`. (AC4: empty ≠ "" — must store None.)
  - `List` → split `edit_input` on `,`, `trim` each, drop empties, collect `Vec<String>` in order; empty input → `vec![]`; write to buffer, `dirty=true`. (AC6)
  - (BoundedNum/Duration handled Task 5.)
Verify: deferred to Test Plan (App-state unit tests on confirm methods).

### Task 4 — Space-driven Toggle (bool) + EnumCycle editors
ACs: AC2 AC7.
Files: `src/tui/state/app.rs`.
- `settings_space(&mut self)` (called from `handle_settings_key` on `Space` when `editing==false`):
  - focused `Toggle` → flip the buffer `bool` at `FieldPath` (`v = !v`), `dirty=true`. (AC2)
  - focused `EnumCycle{variants,idx}` → `idx = (idx+1) % variants.len()`, write `variants[idx]` (the new enum value) into buffer at `FieldPath`, `dirty=true`. Wraps to first after last. (AC7)
  - other editor kinds → no-op (Space does nothing for Text/etc when not editing).
- Enum write maps the variant string back to the config enum (`NumberingStrategy`/`StoreBackend`/`ReservedFormat`/`Severity`/`ValidationRule` shape) for the buffer; Task 1 writer re-serializes to the same string.
- NOTE: switching `numbering`→sqids/reserved or `store`→github-issues here only changes the enum in-buffer; auto-scaffolding the dependent section is slice 4 (OUT). This slice leaves the dependency unmet → caught at save by Task 6 cross-field validation → AC9 footer error.
Verify: deferred to Test Plan.

### Task 5 — Field-level validation as typed: BoundedNum + Duration reject before buffer
ACs: AC3 AC5.
Files: `src/tui/state/app.rs`; reuse `crate::engine::lease::parse_duration` (lease.rs:18).
- In `settings_confirm_edit` (Task 3), add:
  - `BoundedNum{min,max}` → parse `edit_input.trim()` as `u64`. On parse-fail OR out of `[min,max]` → set `edit_error = Some(..)`, KEEP `editing=true`, DO NOT write buffer, DO NOT set `dirty`. On success → write the numeric into buffer (as `u8`/`usize` per field), `dirty=true`, `editing=false`. (AC3: `0`/`11`/`abc` for `sqids.min_length` rejected, buffer keeps prior valid value, no dirty change.)
  - `Duration` → `parse_duration(edit_input.trim())`. `Err` → `edit_error=Some(..)`, keep editing, no buffer write, no dirty. `Ok` → write the raw STRING (not the parsed `Duration`) into buffer, `dirty=true`. (AC5: `abc` no-unit/`30` no-unit rejected, buffer keeps prior duration.)
- Field-level validators are pure: factor `fn validate_bounded(input:&str,min:u64,max:u64)->Result<u64,String>` + reuse `parse_duration` so they're unit-testable without `App`.
Verify: deferred to Test Plan (pure validator tests).

### Task 6 — Save: whole-buffer validation (mirror Config::parse) + single atomic toml_edit write + reload
ACs: AC8 AC9.
Files: `src/engine/config.rs` (factor reusable validate fn); `src/tui/state/app.rs` (`settings_save`); `src/engine/config_write.rs` (Task 1 writer); slice-2 reload primitive call site.
- Reuse `Config::parse` cross-field logic. Cheapest correct path: serialize the buffer to a toml string and run it through `Config::parse` (config.rs:508) — this re-runs EVERY constraint (sqids salt non-empty + min_length 1..=10 config.rs:548-561; reserved section + reserved.format=sqids salt config.rs:563-587; github-issues needs [github] config.rs:589-592; [[relationships]] non-empty config.rs:523-536). BUT buffer→toml must go through Task 1's `write_config_in_place` (against current file src) NOT `to_toml`, so we validate the exact bytes we'd write. So: `let new_src = write_config_in_place(&current_file_src, &buffer)?; Config::parse(&new_src)?`.
  - If a single shared validate fn is preferred over re-parse, factor `pub fn validate_config(cfg: &Config) -> Result<()>` in config.rs holding the same checks lifted out of `parse_inner` (config.rs:540-592) + have `parse_inner` call it; then `settings_save` calls `validate_config(&buffer)`. Pick re-parse-the-rendered-src if simpler — it also catches any toml_edit serialization slip. Document which in Notes.
- `settings_save(&mut self, root, config_reload_hook) -> Result<()>`:
  1. read current `.lazyspec.toml` src from disk (`root.join(".lazyspec.toml")`).
  2. `new_src = write_config_in_place(&src, &self.buffer)?`.
  3. validate (`Config::parse(&new_src)` or `validate_config`). On `Err`: set `footer_error = Some(msg)`, jump focus to the offending field (Task 8), DO NOT write, leave `dirty=true`, return early. (AC9)
  4. On `Ok`: write `new_src` to `.lazyspec.toml` EXACTLY ONCE (`fs::write` of the single rendered string — not per-field writes). (AC8)
  5. trigger slice-2 live reload (re-load `Config`, rebuild `Store`, re-watch type dirs, redraw). Set `dirty=false`, `footer_error=None`. Re-seed `buffer` from the reloaded config.
- Single-write guarantee: the rendered string is computed in memory; disk touched once in step 4. Failed validation (step 3) never reaches step 4 → file never holds invalid intermediate.
- Borrow note: session `config: &Config` is currently borrowed immutably through `run()`/`views::draw` (event_loop.rs:353,386). Slice 2 (ITER-187) owns making it reloadable (owned `Config` on `App`, or reload signal drained in `run()`). This slice's `settings_save` calls that slice-2 reload primitive — it does NOT re-architect the borrow. If slice 2 exposes reload as an `App` flag (e.g. `config_reload_request: bool` drained in `run()`), set it here after a successful write.
Verify: deferred to Test Plan (temp-file write + re-parse; single-write assertion).

### Task 7 — Save/discard prompt on q/Esc when dirty
ACs: AC10.
Files: `src/tui/state/forms.rs` (prompt state); `src/tui/views/keys.rs`; `src/tui/views.rs` (render overlay).
- Add `pub struct SettingsQuitPrompt { pub active: bool }` (or a bool on settings state). On `q`/`Esc` in `handle_settings_key` when `editing==false`: if `self.settings.dirty` → activate prompt instead of quitting/leaving view; if not dirty → normal leave/quit. (AC10: not immediate exit when dirty.)
- Prompt key handling: `s` (save) → run `settings_save` (Task 6 same validate-and-write path; on save Err keep prompt-dismissed + footer error, stay in view per AC9); `d` (discard) → re-seed `buffer` from session `Config`, `dirty=false`, dismiss prompt, leave view; `Esc` → dismiss prompt, stay (cancel the quit). (AC10: discard leaves `.lazyspec.toml` untouched — discard path performs NO write.)
- views.rs: render the prompt overlay in the overlay-stacking block (views.rs:211-242, alongside `draw_delete_confirm` etc) when active. Mirror `DeleteConfirm` overlay pattern (forms.rs:89-111, `draw_delete_confirm`).
Verify: deferred to Test Plan.

### Task 8 — Footer error render + focus jump to offending field
ACs: AC8 (clear on success) AC9 (show + jump).
Files: `src/tui/views.rs` / right-panel render (slice 1) ; `src/tui/state/app.rs`.
- Render `settings.footer_error` (when `Some`) as a footer line under the settings right panel (re-use the `CreateForm.error` footer styling pattern). Also render the `dirty` indicator (e.g. `●` / `[modified]`) in the settings panel title when `dirty==true`. (AC1/AC2/AC7 surface dirty; AC8 clears it.)
- Focus jump: `settings_save` on validation `Err` must move settings focus (selected category + entry + field index from slice 1's nav model) to the field the failed constraint names. Map constraint→field:
  - sqids salt empty / min_length OOR → focus `[numbering.sqids]` `salt`/`min_length` (drill into Numbering category).
  - reserved missing / reserved.format=sqids salt → `[numbering.reserved]` `format` or `[numbering.sqids]` `salt`.
  - github-issues needs [github] → focus the offending type's `store` field (the field the user changed), or the GitHub category `repo`.
  - [[relationships]] empty → Relationships category (this slice doesn't delete relationships, so unlikely; still map defensively).
- On successful save clear `footer_error`. Field-level `edit_error` (Task 5) renders separately while `editing==true`.
Verify: deferred to Test Plan.

## Test Plan

One entry per AC. Prefer App-state unit tests on the editor/confirm methods + pure validators; atomic write tested against a temp `.lazyspec.toml`. Slice 1 (`ViewMode::Settings`, field nav, read-only render) + slice 2 (reload primitive) assumed present; tests stub the reload hook (no-op) where a save's reload would otherwise need a live event loop.

- **AC1 (text free string)**: App-state unit. Seed `SettingsState` w/ focus on a `Text` field (`naming.pattern`). `settings_start_edit()`, push chars into `edit_input`, `settings_confirm_edit()`. Assert buffer `naming.pattern` == typed value, `dirty==true`, `editing==false`. Seam: `App::settings_*` methods (no terminal). Property: behavior-focused (asserts buffer + dirty, not internal scratch).
- **AC2 (bool toggle Space)**: App-state unit. Focus `tui.statusbar.enabled` (`Toggle`). Capture prior bool, call `settings_space()`. Assert buffer bool flipped, `dirty==true`. Repeat → flips back. Also assert `subdirectory` on a `[[types]]` entry flips. Seam: `settings_space`.
- **AC3 (bounded numeric reject)**: pure validator unit on `validate_bounded(input,1,10)`. Assert `"0"`→Err, `"11"`→Err, `"abc"`→Err, `"5"`→Ok(5). Plus App-state: focus `sqids.min_length`, edit to `"0"`, confirm → assert buffer keeps prior value, `dirty` unchanged (still false), `edit_error.is_some()`, `editing` still true. Then edit `"7"`, confirm → buffer updated, dirty true. Seam: pure fn + `settings_confirm_edit`. Property: tests the boundary (0/11 reject, 1/10 accept).
- **AC4 (nullable empty vs unset)**: App-state unit. Focus `github.repo` (`Nullable`). Edit empty string, confirm → assert buffer `github.repo == None` (NOT `Some("")`). Edit `"owner/repo"`, confirm → `Some("owner/repo")`. Seam: `settings_confirm_edit`. Note: distinguishing None from `Some("")` is the load-bearing assertion.
- **AC5 (duration reject)**: pure unit reusing `crate::engine::lease::parse_duration`: assert `parse_duration("abc").is_err()`, `parse_duration("30").is_err()` (no unit), `parse_duration("60m").is_ok()`. Plus App-state: focus `coordination.lease_duration`, edit `"abc"`, confirm → buffer keeps `"60m"`, `edit_error.is_some()`, no dirty. Edit `"30m"`, confirm → buffer `"30m"`, dirty. Seam: pure fn + `settings_confirm_edit`.
- **AC6 (list round-trip)**: App-state unit. Focus a type's `agents` (`List`). Edit `"expand, create-children"`, confirm → buffer `agents == vec!["expand","create-children"]` (trimmed, entered order). Edit `""`, confirm → `vec![]`. Edit `" a , b ,"` → `vec!["a","b"]` (empties dropped). Seam: `settings_confirm_edit`.
- **AC7 (enum cycle Space wrap)**: App-state unit. Focus `numbering` (`EnumCycle`, variants incremental/sqids/reserved), start at incremental. `settings_space()` ×1 → buffer numbering == Sqids, dirty. ×2 → Reserved. ×3 → wraps to Incremental. Assert each step records buffer + dirty. Repeat shape for `store` (filesystem/github-issues/git-ref) + rule `severity` (error/warning) + rule `shape` (parent-child/relation-existence). Seam: `settings_space`. Property: asserts order + wrap.
- **AC8 (atomic save + reload + dirty clear)**: temp-file integration. Write a valid `.lazyspec.toml` (with a `# comment` + `[[types]]` + `[[relationships]]`) into a `tempfile::tempdir()`. Build buffer from `Config::parse` of that src, make one scalar edit (e.g. `naming.pattern`), `dirty=true`. Call `settings_save` w/ a no-op reload hook. Assert: file written exactly once (wrap the write so the test can count, or assert via re-read that content changed in exactly the edited value); re-`Config::parse` of the file succeeds; the `# comment` survives (formatting/comments preserved by `toml_edit`); only `naming.pattern` differs from original; `dirty==false`, `footer_error==None`. Seam: `write_config_in_place` + `settings_save`. Property: comment-survival is the toml_edit-specific assertion; single-write asserted by content-diff (no partial intermediate).
- **AC9 (failed save: footer + jump + stays dirty + no write)**: temp-file + App-state. Buffer with a cross-field violation: a `[[types]]` entry set `numbering = sqids` while `[numbering.sqids].salt` empty (OR `store = github-issues` w/ no `[github]`). Snapshot file mtime/content. Call `settings_save`. Assert: returns w/o writing (file content unchanged), `footer_error.is_some()` and message describes the constraint (contains `"sqids"`/`"salt"` or `"[github]"`), focus moved to the offending field (assert settings nav indices point at the salt field / the type's store field), `dirty` still true. Seam: `settings_save` + `Config::parse` reuse. Property: asserts NO write happened (the invalid-intermediate guard).
- **AC10 (quit prompt save/discard)**: App-state unit. Set `dirty=true`. Send `q` (or `Esc`) via `handle_settings_key` → assert quit prompt active, `should_quit==false`, still in `ViewMode::Settings` (not immediate exit). Then: (a) `d` discard → assert buffer re-seeded from session config, `dirty==false`, prompt dismissed, `.lazyspec.toml` untouched (no write occurred — assert against a temp file's unchanged content). (b) fresh dirty state, `s` save → assert it runs the same validate-and-write path as `w` (file written when valid; footer error when invalid). Seam: `handle_settings_key` + prompt handlers + `settings_save`. Property: discard performs zero writes (load-bearing).

## Notes

- Build deps: **STORY-137 / ITERATION-186** (slice 1) — provides `ViewMode::Settings`, the read-only field list per category/entry, j/k field nav, and the right-panel render this slice attaches editors + footer + dirty indicator to. **STORY-138 / ITERATION-187** (slice 2) — provides the reloadable-session-`Config` primitive (re-load `Config`, rebuild `Store`, re-watch type dirs, redraw) that `settings_save` calls after a successful write, and adds `.lazyspec.toml` to the watch set. Both sibling iterations were still TODO stubs when this was authored, so the exact `SettingsState` field names and reload-hook signature follow slice 1/2 once landed; tasks here describe the seam, not those slices' internals.
- New crate dep: **`toml_edit`** (Cargo.toml). Coexists with existing `toml = "0.8"` — serde reads stay on `toml`, in-place edit uses `toml_edit::DocumentMut`. `tempfile = "3"` already present (Cargo.toml:39) — reused for AC8/AC9/AC10 temp-file tests.
- Key decision 1 — **reuse `Config::parse` for whole-buffer cross-field validation** rather than duplicating the constraints. Two acceptable forms: (a) render the buffer via `write_config_in_place` then `Config::parse(&new_src)` — validates the exact bytes to be written and catches any serialization slip; or (b) factor `validate_config(&Config)` out of `parse_inner` (config.rs:540-592) and call it on the buffer. Prefer (a) unless it proves awkward; record the chosen form in the implementation. Either way the constraints (sqids salt/min_length config.rs:548-561, reserved + reserved.format=sqids config.rs:563-587, github-issues→[github] config.rs:589-592, non-empty [[relationships]] config.rs:523-536) are NOT re-implemented.
- Key decision 2 — **`toml_edit` in-place write, never `Config::to_toml`** (config.rs:655 = `toml::to_string_pretty`, drops comments + reorders). `write_config_in_place(existing_src, buffer)` parses the current file into a `DocumentMut` and mutates only changed keys so comments/formatting/key-order survive (AC8). The current `init.rs:54` `config.to_toml()` write path is for fresh scaffolding only and stays as-is; settings save uses the new writer.
- Single-atomic-write guarantee (AC8) = render full new file string in memory, validate it, then one `fs::write`. Failed validation (AC9) returns before the write, so `.lazyspec.toml` never holds an invalid intermediate.
- OUT of this slice (downstream): auto-scaffolding a dependent section when an enum edit creates a dependency (slice 4) — here such an edit simply fails save validation w/ an AC9 footer error; collection add/delete `n`/`d` (slice 5); document-impact confirm on `dir`/`prefix`/`store` change (slice 6); statusbar component ordering (slice 7). This slice edits scalar fields of existing entries only.
