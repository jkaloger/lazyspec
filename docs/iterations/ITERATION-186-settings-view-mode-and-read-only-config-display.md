---
title: Settings view mode and read-only config display
type: iteration
status: accepted
author: agent
date: 2026-06-19
tags: []
related:
- implements: STORY-137
---

## Changes

Read-only only. NO edit/dirty-buffer/write/reload. All state new fields on `App`.

1. **`ViewMode::Settings` variant + cycle + name** (AC1, AC8) — `src/tui/state/app.rs`.
   - Add `Settings` to `ViewMode` enum (app.rs:165-174), after `Graph` (place before `#[cfg(feature="agent")] Agents` so cycle order = ...Graph→Settings→[Agents→]Types). NOT cfg-gated — always present.
   - `next()` (app.rs:177-193): rewire so `Graph→Settings`; `Settings→Agents` (agent feat) else `Settings→Types`. Drop old `Graph→Agents`/`Graph→Types` arms accordingly. Keep metrics/agent cfg arms intact.
   - `name()` (app.rs:195-206): add `ViewMode::Settings => "Settings"`.
   - Verify: `cargo build` (TUI). enum exhaustive matches still compile.

2. **Settings nav state on `App`** (AC2,AC3,AC5,AC6) — `src/tui/state/app.rs` struct (216-308) + both ctors (`App::new` 360-450 init block; test `make_test_app` 1561-1642).
   - Add fields: `pub settings_category: usize` (selected left-panel cat idx, 0..10), `pub settings_drill: Option<usize>` (Some(entry_idx) when drilled into a collection entry, None at category/entry-list level).
   - Init both ctors: `settings_category: 0`, `settings_drill: None`.
   - Add const/fn for category list. Add `impl App` method `pub fn settings_categories() -> &'static [&'static str]` returning exactly `["General","Document Types","Relationships","Validation Rules","Numbering","GitHub","Coordination","Certification","Agents","Interface"]` (len 10, order per STORY AC2 + RFC table 75-84). Use this everywhere (nav clamp + render) — single source of truth.
   - Verify: both ctors compile; `App::settings_categories().len() == 10`.

3. **Settings key handler** (AC2,AC5,AC6,AC8) — `src/tui/views/keys.rs`.
   - Add `fn handle_settings_key(&mut self, code: KeyCode, _modifiers: KeyModifiers)` mirroring `handle_graph_key` (keys.rs:590-635) no-wrap idiom.
   - In `handle_normal_key` dispatch match (keys.rs:645-651) add arm `ViewMode::Settings => return self.handle_settings_key(code, modifiers),` (mirrors Filters/Graph/Agents arms).
   - Key behaviour in `handle_settings_key`:
     - `j`/`Down`: if `settings_drill.is_none()` AND current category NOT a collection → `settings_category = (settings_category+1).min(App::settings_categories().len()-1)` (no wrap past end, per Graph idiom). If category IS collection (Document Types/Relationships/Validation Rules/Certification) AND `settings_drill.is_none()` → move entry selection down (reuse `settings_category`? NO — separate cursor). Add field `pub settings_entry: usize` (see task 2 amend below) clamped to entry count-1. If `settings_drill.is_some()` → no-op (drilled field list is static read-only, no nav needed).
     - `k`/`Up`: symmetric `saturating_sub(1)` on whichever cursor is active (category vs entry), per drill level.
     - `Enter`: only when on a collection category AND `settings_drill.is_none()` AND entry-count>0 → `settings_drill = Some(settings_entry)` (drill in, AC5).
     - `Esc`: if `settings_drill.is_some()` → `settings_drill = None` (back to entry list, AC6). Else no-op.
     - `q` → `self.should_quit = true` (AC8). Backtick `` ` `` → `self.cycle_mode()` (AC8 cycle; cycle_mode app.rs:467 needs no Settings-specific reset this slice — leaving Settings does nothing, entering resets handled by ctor defaults persisting; OPTIONAL: reset `settings_drill=None`,`settings_entry=0` when entering Settings in cycle_mode for clean state — add `if self.view_mode==ViewMode::Settings { self.settings_drill=None; self.settings_entry=0; }` after the `view_mode=next()` line).
   - AMEND task 2: also add `pub settings_entry: usize` field + init `0` in both ctors.
   - Critical AC7: NO key arm in `handle_settings_key` mutates config/store/any doc state. Only the three cursor fields + should_quit + cycle. No `editor_request`, no writes.
   - Verify: dispatch compiles; pressing j on General-category app advances category not entry.

4. **Number-key entry into Settings** (AC1,AC8) — `src/tui/views/keys.rs` `handle_normal_key` match-block (keys.rs:653-749, the `match (code, modifiers)` for types-mode).
   - Currently NO number keys bound to modes (only `` ` `` cycle at keys.rs:738). RFC line 33 says number key "likely 5 or 0". Bind `(KeyCode::Char('5'), _) => { self.view_mode = ViewMode::Settings; self.settings_category=0; self.settings_drill=None; self.settings_entry=0; }`. Place near the `` ` `` cycle arm (keys.rs:738).
   - NOTE: number-key only reachable from types-mode dispatch (the `_ =>{}` modes fall through to this block). That satisfies AC1 "from any existing view mode" partially — from Filters/Graph/Agents the dedicated handlers return early. To satisfy AC1 fully ("any existing view mode"), ALSO add the `5`→Settings arm into `handle_filters_key`(492-588), `handle_graph_key`(590-635), `handle_agents_key`(~457-490). Minimal: add `KeyCode::Char('5') => { self.view_mode=ViewMode::Settings; self.settings_category=0; self.settings_drill=None; self.settings_entry=0; }` arm to each (and to settings_key itself = no-op re-entry, harmless). Keep one private helper `fn enter_settings(&mut self)` on `App` to dedupe.
   - Verify: from Types/Filters/Graph[/Agents], `5` sets `view_mode==Settings`.

5. **Settings render: two-panel + category list** (AC1,AC2,AC3) — `src/tui/views.rs` draw dispatch (views.rs:173-205) + new fn in `src/tui/views/panels.rs`.
   - In `views.rs` match add arm `ViewMode::Settings => draw_settings(f, app, outer[1], config),`. Import `draw_settings` from panels (add to `use panels::{...}` at views.rs:35-38).
   - New `panels.rs` fn `pub fn draw_settings(f:&mut Frame, app:&App, area:Rect, config:&Config)`:
     - Horizontal `Layout` 20/80 (mirror Types main split views.rs:175-178) → `[cats_area, settings_area]`.
     - Left: render `App::settings_categories()` as a list (mirror `draw_type_panel` style in panels.rs), highlight idx `app.settings_category`. Block title "Categories".
     - Right: dispatch on category + drill state (task 6). Block title "Settings" or breadcrumb (task 6).
   - Verify: entering Settings shows two panels, left lists 10 cats, selected highlighted.

6. **Settings render: per-category field display + drill + breadcrumb + `(unset)`** (AC3,AC4,AC5,AC6,AC7) — `src/tui/views/panels.rs` (in/around `draw_settings`), reads `config: &Config`.
   - Right panel content by `app.settings_category` (index into `settings_categories()`):
     - 0 General (top-level): lines `naming.pattern: {config.documents.naming.pattern}`, `ref_count_ceiling: {config.ref_count_ceiling}`, `templates.dir: {config.filesystem.templates.dir}` (RFC 75; fields verified config.rs Naming.pattern 292, Config.ref_count_ceiling 277, FilesystemConfig.templates.dir 201/288).
     - 1 Document Types (`[[types]]` collection): entry list = `config.documents.types` names (TypeDef.name). On drill (`settings_drill==Some(i)`) show entry field list for `types[i]`: name, plural, dir, prefix, icon (Option→`(unset)` if None), numbering (NumberingStrategy Display/Debug → incremental/sqids/reserved), subdirectory (bool), store (StoreBackend Display, store.rs:123 impl), singleton (bool), parent_type (Option→`(unset)`), agents (Vec<String> join `, ` or `(unset)` if empty). 11 fields per STORY AC5. (config.rs TypeDef 133-152.)
     - 2 Relationships (`[[relationships]]` collection): entry list = `config.relationships` names (RelationshipDef.name). Drill fields: name, inverse (Option→`(unset)`). (config.rs RelationshipDef 158-163.)
     - 3 Validation Rules (`[[rules]]` collection): entry list = `config.rules` (ValidationRule). Use the rule `name` field as entry label. Drill fields by variant (config.rs ValidationRule 13-32): ParentChild → shape=`parent-child`, name, child, parent, link, severity (Severity Debug→error/warning); RelationExistence → shape=`relation-existence`, name, type (doc_type), require, severity.
     - 4 Numbering (`[numbering.sqids]`/`[numbering.reserved]`, both optional): render two sub-blocks. sqids: `salt`,`min_length` from `config.documents.sqids` (Option<SqidsConfig> config.rs:193) → whole block `(unset)` if None else field values. reserved: `remote`,`format`(ReservedFormat Debug),`max_retries` from `config.documents.reserved` (Option config.rs:195) → `(unset)` if None. (AC4 unset path.)
     - 5 GitHub (`[github]` optional): from `config.documents.github` (Option<GithubConfig> config.rs:197). If None → render `repo: (unset)`, `cache_ttl: (unset)` (AC4). Else `repo` (Option<String>→`(unset)` if None), `cache_ttl`. (GithubConfig config.rs:341-346.)
     - 6 Coordination (`[coordination]` optional): from `config.coordination` (Option config.rs:281). None → all 5 fields `(unset)` (AC4). Else `remote`,`lease_duration`,`grace_period`,`max_push_retries`,`max_clock_skew`. (CoordinationConfig 98-110.)
     - 7 Certification (`[certification]` + `overrides` map = collection): top fields `normalize` (bool, config.certification.normalize). overrides = `config.certification.overrides` (HashMap<String,CertificationOverride>) rendered as entry list keyed by spec-path; drill into one → field `normalize` (bool). (CertificationConfig 296-329.) Iterate map in sorted-key order for deterministic display/tests.
     - 8 Agents (`[agents]` optional fields): `interactive` from `config.agents.interactive` (Option<String> config.rs:354) → value or `(unset)`. (AgentsConfig 351-355.)
     - 9 Interface (`[tui]`): `ascii_diagrams` (bool), `statusbar.enabled` (bool), `statusbar.left`/`center`/`right` (each Option<Vec<String>>→join `, ` or `(unset)`), `multiline.max_expanded_height` (usize). (UiConfig 250-258, StatusBarConfig 205-215, MultiLineConfig 236-240.)
   - Collection categories (1,2,3,7): when `settings_drill.is_none()` render entry list w/ `app.settings_entry` highlighted, block title plain category name. When `settings_drill==Some(i)` render that entry's field list + breadcrumb header `"{Category} > {entry_label}"` (AC5; Esc clears drill → list, AC6 handled in task 3). Non-collection cats (0,4,5,6,8,9): always field list, no drill, ignore `settings_entry`/`settings_drill`.
   - ALL lines plain `Span`/`Paragraph` read-only — no input widget, no cursor (AC7).
   - Verify: General shows 3 named fields w/ values; GitHub-absent shows `(unset)`; drilling rfc shows 11 fields + breadcrumb `Document Types > rfc`.

7. **README key reference** (housekeeping per CLAUDE.md) — `README.md`.
   - If README documents TUI keybinds/view modes, add `5` = Settings view and list it among modes. Grep README for existing mode/keybind table; append Settings row only if such a table exists. If none, skip.
   - Verify: README mode list (if present) includes Settings.

## Test Plan

App-state unit tests on `App` methods (mirror existing `src/tui/state/app.rs` `#[cfg(test)] mod tests`, `make_test_app` 1545-1644 / `app_with_store` 2091). Build the test config with `Config::default()` (has starter types incl `rfc`, starter relationships, starter rules) for populated-field assertions; construct a minimal/empty-section config for `(unset)` assertions. Render-string assertions go through the `draw_settings` text builder — extract field-line construction into a pure helper `fn settings_lines(app:&App, config:&Config) -> Vec<String>` (or `settings_right_lines`) in panels.rs so tests assert on `Vec<String>` without a `Frame` (mirrors `doc_row_cells_for_test` test-seam pattern, panels.rs/views.rs tests:283-507). Note tradeoff: testing the pure line-builder not the ratatui paint — accepted, same seam the existing doc-row tests use.

- **AC1** (number key enters Settings, two-panel): unit — new App (`view_mode=Types`), call `enter_settings()` (or `handle_key(Char('5'),...)`); assert `app.view_mode==ViewMode::Settings`, `settings_category==0`, `settings_drill==None`. Layout: assert `App::settings_categories().len()==10` (panel left content) + a render test that `draw_settings` produces a left list of 10 cats (via a `settings_categories()` assertion since two-panel split is structural).
- **AC2** (10 cats, j/k no wrap): unit — `settings_categories()` equals the exact ordered 10-name slice. Nav: from `settings_category=0`, press `k` (Up) → stays `0` (no wrap past start). Press `j` 9x → reaches `9`; press `j` again → stays `9` (no wrap past end). Mirror Graph no-wrap test if one exists.
- **AC3** (selected category → its fields w/ current values): unit on `settings_lines` w/ `Config::default()` — set `settings_category=0` (General) → lines contain `naming.pattern:` + the config's pattern value, `ref_count_ceiling:` + value, `templates.dir:` + value. Repeat one populated assert per simple category (e.g. Interface `ascii_diagrams:`).
- **AC4** (`(unset)` for absent optional sections): unit — config w/ `github=None`, `coordination=None`, `sqids=None`, `reserved=None`, `agents.interactive=None`. `settings_category=5` (GitHub) → line `repo: (unset)` present (NOT omitted/blank). Category=6 (Coordination) → all 5 fields `(unset)`. Category=4 (Numbering) → sqids+reserved blocks `(unset)`. Category=8 (Agents) → `interactive: (unset)`.
- **AC5** (Enter drills, breadcrumb): unit w/ `Config::default()` — `settings_category=1` (Document Types), `settings_entry` = index of `rfc`, `settings_drill=None`; press `Enter` → `settings_drill==Some(rfc_idx)`. `settings_lines` then contains all 11 field labels (name,plural,dir,prefix,icon,numbering,subdirectory,store,singleton,parent_type,agents) AND breadcrumb string `Document Types > rfc`.
- **AC6** (Esc returns to entry list, breadcrumb drops entry): unit — from drilled state (`settings_drill=Some(i)`), press `Esc` → `settings_drill==None`; `settings_lines` is the entry list (contains other type names, NOT the breadcrumb `> rfc`). Also assert `Esc` at non-drilled level is no-op (drill stays None, mode stays Settings).
- **AC7** (every field read-only, drill all 4 collections, no mutation): unit — for each collection category (Document Types `[[types]]`, Relationships `[[relationships]]`, Validation Rules `[[rules]]`, Certification `overrides`) drill into entry 0 and assert its full field set renders. Mutation guard: snapshot the `Config` (clone) + assert no store/doc mutation — feed every key (`j k Enter Esc h l Space x s d e r p`) through `handle_settings_key` from a Settings-mode app and assert: config clone unchanged (config is `&` borrowed, never mutated by design — assert by construction), `editor_request` stays `None`, `store` doc count unchanged, `should_quit` only flips on `q`. Confirms no keypress stages/writes.
- **AC8** (cycle + quit, no save prompt): unit — Settings-mode app: press `q` → `should_quit==true`. Fresh Settings-mode app: press `` ` `` → `cycle_mode()` ran, `view_mode != Settings` (advanced per `next()`: Settings→Agents or →Types). Assert NO save-prompt state exists (no dirty-buffer field this slice — assert by absence: no `create_form`/dialog activated, mode just changed). Also `ViewMode::next()` table test: `Graph.next()==Settings`, `Settings.next()==` Agents(agent feat)/Types(else).

## Notes

- Build dep: none. Self-contained TUI slice on existing `App`/`ViewMode`/render scaffold.
- Number key = `5` (RFC line 33 "likely 5 or 0"; existing modes have NO number bindings, only `` ` `` cycle — picking `5` adds direct entry; cycle still works). Revisit if RFC-022/slice-7 renumbers.
- `ViewMode::Settings` NOT cfg-gated (unlike Metrics/Agents) — always available. Cycle order inserts after Graph: `...Graph→Settings→[Agents→]Types`. Keep metrics/agent `#[cfg]` arms when editing `next()`.
- Three cursor fields chosen over one packed enum for minimal diff + direct test access: `settings_category` (0..10), `settings_entry` (collection entry cursor), `settings_drill: Option<usize>` (None=list level, Some=drilled). Non-collection categories ignore entry/drill.
- Collection categories = {Document Types, Relationships, Validation Rules, Certification(overrides)}; rest are flat field lists. Drill/Enter/Esc only meaningful on collections (AC5/AC6/AC7).
- Render seam: pure `settings_lines(app,config)->Vec<String>` builder + thin `draw_settings` painter, mirroring existing `doc_row_cells_for_test` seam (views.rs tests). Lets every AC3-AC7 content assert run without a `Frame`.
- `(unset)` rule (AC4, RFC 88): optional sections `[github]`/`[coordination]`/`[numbering.sqids]`/`[numbering.reserved]`/`[agents]` + optional fields (`icon`,`parent_type`,`inverse`,`repo`,`statusbar.*`,`agents.interactive`) render literal `(unset)` when None/absent; never omit the row.
- Field sources verified in `src/engine/config.rs`: Config (260-284), DocumentConfig (185-198), TypeDef (133-152), RelationshipDef (158-163), ValidationRule (13-32), Severity (6-11), NumberingStrategy (34-41), SqidsConfig (43-48), ReservedConfig (61-68), CoordinationConfig (98-110), GithubConfig (341-346), CertificationConfig (296-311), CertificationOverride (313-316), AgentsConfig (351-355), UiConfig (250-258), StatusBarConfig (205-215), MultiLineConfig (236-240), Naming (291-294), Templates (286-289). StoreBackend Display impl store.rs:123.
- Deterministic ordering: iterate `certification.overrides` (HashMap) by sorted key for stable render + tests.
- AC7 is enforced by design (config passed as `&Config`, no mut path in `handle_settings_key`), not a new guard — the test asserts the absence of mutation.
