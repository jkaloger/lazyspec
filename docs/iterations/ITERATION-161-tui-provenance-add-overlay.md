---
title: TUI provenance add overlay
type: iteration
status: accepted
author: agent
date: 2026-04-30
tags: []
related:
- implements: STORY-113
---



## Context

STORY-113 wires a TUI affordance to append a single provenance entry to a
document. Engine has `DocumentStore::set_provenance` per backend
(filesystem, github-issues, git-ref) and `DocMeta.provenance` validation
rejecting empty strings at load. CLI shipped via STORY-111.

The CLI orchestrates store selection in `src/cli/provenance.rs:79`
(`dispatch_set_provenance`) and exposes `run_add` which:
1. Resolves the doc.
2. Clones provenance, pushes new entry.
3. Dispatches to the backing store.
4. Reloads and returns updated list.

TUI cannot depend on CLI per principle 3, but `confirm_link` already
calls `crate::cli::link::link_with_config` (precedent of layering
violation). For this iteration, prefer a clean fix: lift the dispatch
into the engine layer. Two concrete callers (CLI add/remove + TUI add)
satisfies principle 6 -- abstract when there are two uses, not before.

The overlay shape is a single text input plus error line, modelled on
`LinkEditor` (`src/tui/state/forms.rs:137`) but simpler -- one field,
no candidate list, no relation type cycling.

## Changes

1. **Lift store dispatch into engine.** New module
   `src/engine/provenance.rs` exporting:
   ```rust
   pub fn set_provenance(
       root: &Path,
       config: &Config,
       type_name: &str,
       doc_id: &str,
       new_list: &[String],
   ) -> anyhow::Result<()>
   ```
   Body: copy of `dispatch_set_provenance` from
   `src/cli/provenance.rs:79`. Register module in `src/engine.rs`
   (`pub mod provenance;`). Update `src/cli/provenance.rs` to delete
   the local helper and call `engine::provenance::set_provenance`
   from `run_add` and `run_remove`. Verify: existing CLI integration
   tests in `tests/` covering provenance still pass; `cargo test`
   clean. ACs: 2 (mutation path for TUI submission).

2. **Provenance citation validation helper.** In
   `src/engine/provenance.rs` add:
   ```rust
   pub fn validate_citation(citation: &str) -> Result<&str, ProvenanceError>
   pub enum ProvenanceError { Empty, Duplicate(String) }
   ```
   `validate_citation` returns trimmed citation when non-empty after
   trim, else `Err(ProvenanceError::Empty)`. Implement `Display` for
   `ProvenanceError`. Migrate `cli/provenance.rs:142`
   `bail!("citation must not be empty")` to call this helper, mapping
   `Empty` to `bail!`. ACs: 3, 6.

3. **`ProvenanceEditor` state.** Add to `src/tui/state/forms.rs`
   after `LinkEditor`:
   ```rust
   pub struct ProvenanceEditor {
       pub active: bool,
       pub doc_path: PathBuf,
       pub input: String,
       pub error: Option<String>,
   }
   ```
   `Default` + `new()` mirror `LinkEditor`. Re-export from
   `src/tui/state.rs` line 13 alongside other forms. ACs: 1, 4.

4. **App wiring.** `src/tui/state/app.rs`:
   - Add `pub provenance_editor: ProvenanceEditor` field near
     `link_editor` declaration on line 213, and at the two
     construction sites (lines 337 and 1473).
   - Add methods after `close_link_editor` (line 1293):
     ```rust
     pub fn open_provenance_editor(&mut self) { ... }
     pub fn close_provenance_editor(&mut self) { ... }
     pub fn provenance_type_char(&mut self, c: char) { ... }
     pub fn provenance_backspace(&mut self) { ... }
     pub(crate) fn submit_provenance(&mut self, root: &Path, config: &Config) -> Result<()>
     ```
   - `open_provenance_editor` mirrors `open_link_editor:1263-1284`:
     resolve selected doc via `view_mode` branch, set `active=true`,
     capture `doc_path`, clear input/error. Returns silently if no
     selected doc (AC7 guard).
   - `submit_provenance`:
     1. Trim `input`. On empty: `error = Some("citation must not be
        empty".into())`, return `Ok(())` (overlay stays open). AC3.
     2. Resolve doc by `doc_path` via `self.store`. Clone provenance.
        If trimmed input already in list: `error = Some("citation
        already present".into())`, return `Ok(())`. AC6.
     3. Push trimmed input. Call
        `engine::provenance::set_provenance(root, config, &type_name,
        &doc_id, &new_list)`. On `Err`: `error = Some(e.to_string())`,
        return `Ok(())` (AC8).
     4. On success:
        `self.store.reload_file(root, &doc_path, &*self.fs)?`,
        `self.filtered_docs_cache = None`, `rebuild_search_index()`,
        `build_doc_tree()`, `close_provenance_editor()`. AC5.
   ACs: 1, 2, 3, 4, 5, 6, 8.

5. **Keybinding wiring.** `src/tui/views/keys.rs`:
   - Insert early-return after `link_editor.active` block (line 41):
     ```rust
     if self.provenance_editor.active {
         return self.handle_provenance_editor_key(code, root, config);
     }
     ```
   - Add `handle_provenance_editor_key` near other handler impls.
     `Esc` -> `close_provenance_editor()` (AC4); `Enter` -> `let _ =
     self.submit_provenance(root, config);` (AC2/3/5/6/8); `Backspace`
     -> `provenance_backspace()`; `Char(c)` ->
     `provenance_type_char(c)`.
   - In `handle_normal_key` near line 621: add
     `(KeyCode::Char('p'), _) => self.open_provenance_editor(),`.
     `open_provenance_editor` guards via `selected_doc_meta` returning
     early when none, satisfying AC7.
   ACs: 1, 4, 7.

6. **Overlay rendering.** `src/tui/views/overlays.rs` after
   `draw_link_editor:267`:
   ```rust
   pub fn draw_provenance_editor(f: &mut Frame, app: &App)
   ```
   Centered popup, ~60 cols x 8 rows. Title shows doc id. Input row
   renders `editor.input` with cursor block. Hint row: `Enter to add,
   Esc to cancel`. Error row in red when `editor.error.is_some()`.
   In `src/tui/views.rs:211`-style block, add:
   ```rust
   if app.provenance_editor.active {
       draw_provenance_editor(f, app);
   }
   ```
   Add to the `views` re-export at line 28-29. ACs: 1, 3, 8.

7. **Help overlay update.** `src/tui/views/overlays.rs:34-46`. Add
   `Line::from("  p         Add provenance entry"),` near the relations
   section. Bump `popup_height` from 24 if overflow.

## Test Plan

Per DICTUM-004: behavioural, isolated, deterministic, through public
APIs.

### Engine (unit, in `src/engine/provenance.rs` `mod tests`)

- `validate_citation_rejects_empty` -- `validate_citation("")` returns
  `Err(ProvenanceError::Empty)`. AC3.
- `validate_citation_rejects_whitespace_only` --
  `validate_citation("   ")` returns `Err(ProvenanceError::Empty)`. AC3.
- `validate_citation_trims_and_returns` --
  `validate_citation("  X  ")` returns `Ok("X")`. AC2.

`set_provenance` dispatch is thin; coverage already exists in
`store_dispatch.rs` and `git_ref_store.rs` test modules.

### TUI state (integration, new file `tests/tui_provenance_add.rs`)

`tempfile::TempDir`, the existing test ctor pattern from
`app.rs:1473`-area. Fixture: filesystem store, one RFC doc with
empty provenance (and a second test doc with `["X"]` for duplicate
case).

- `open_provenance_editor_activates_on_selected_doc` -- select doc,
  call `open_provenance_editor`, assert `provenance_editor.active`,
  `doc_path` matches, `input` empty, `error` None. AC1.
- `open_provenance_editor_noop_when_no_selection` -- empty store, no
  selection, call `open_provenance_editor`, assert
  `provenance_editor.active == false`. AC7.
- `submit_provenance_appends_and_persists` -- open, type citation
  chars, submit, assert: editor closed, fresh `Store::load` shows
  new entry, in-memory `App::store` shows it. AC2, AC5.
- `submit_provenance_rejects_empty` -- open, submit with empty input,
  editor still active, error contains `"empty"`. AC3.
- `submit_provenance_rejects_whitespace` -- open, type `"   "`,
  submit, editor active, error contains `"empty"`. AC3.
- `submit_provenance_rejects_duplicate` -- doc has provenance `["X"]`,
  type `"X"`, submit, editor active, error contains `"already"`,
  on-disk unchanged. AC6.
- `close_provenance_editor_clears_state` -- open, type chars, close,
  assert active false, input empty, error None. AC4.
- `submit_provenance_engine_error_keeps_overlay_open` -- doc removed
  from disk after open. Submit. Assert: editor active, error populated,
  in-memory store unchanged. AC8.

### Keybinding (state-level, same integration file)

- `key_p_opens_overlay_when_doc_selected` -- send `KeyCode::Char('p')`
  via `handle_key`, assert overlay active. AC1.
- `key_p_noop_when_no_selection` -- empty store, send `p`, assert
  overlay not active. AC7.
- `key_esc_closes_overlay` -- open, send `Esc`, assert closed. AC4.
- `key_enter_submits_overlay` -- open, type chars, send `Enter`,
  assert closed and persisted. AC5.

### Tradeoffs

- **Error surface from `submit_provenance`**: returns `Result<()>` but
  for *user* validation errors and engine failures, sets
  `editor.error` and returns `Ok(())`. Caller does `let _ = ...`.
  Engine errors must not crash the event loop. Tests assert on
  `editor.error` instead of returned `Err`.
- **Trim semantics**: `validate_citation` trims. CLI today checks
  `is_empty`, not `trim().is_empty()`. Trimming is a behaviour change
  for CLI -- treat as fix-aligned-with-RFC and call out in commit. If
  preserving CLI behaviour unchanged is preferred, validate without
  trim and trim only at TUI input boundary.
- **AC8 error injection**: removing the file under the TUI is the
  cleanest deterministic trigger. If flaky on macOS, fall back to
  read-only `chmod` on the doc file. If both prove unreliable, drop
  this AC from automated tests and verify manually, documented here.
- **Backend parity**: TUI tests cover filesystem only. github-issues
  and git-ref dispatch are exercised by existing per-store tests
  for `set_provenance`.

## Notes

- `set_provenance` writes the full new list (not append delta). This
  iteration load-modify-writes through `App::store`, matching
  `cli/provenance.rs:149-152`.
- After step 1, `cli/provenance.rs` should be net-shorter; dispatch
  lives only in `engine::provenance`.
- Manual smoke after build: `cargo run` -> select RFC -> press `p` ->
  type citation -> Enter -> list-view `Provenance` column updates and
  detail panel header line shows new entry. Press `p` on empty
  selection -> no overlay opens.
