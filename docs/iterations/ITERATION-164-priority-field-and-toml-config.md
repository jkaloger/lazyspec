---
title: Priority field and TOML config
type: iteration
status: complete
author: agent
date: 2026-04-30
tags: []
related:
- implements: STORY-124
---



## Summary

Single iteration covers all 9 ACs of STORY-124. Two strands share `TypeDef` plumbing per RFC-041 + STORY-124 grilling Q4 (one PR; no split):

1. **Priority** — new `priority` frontmatter field, `[priorities.*]` TOML w/ MoSCoW default, `TypeDef::requires_priority`, frontmatter validation, engine weight-map accessor.
2. **Terminal statuses** — `TypeDef::terminal_statuses` config field w/ RFC-041 spec defaults, `is_terminal` migrated off hardcoded match in `engine::sequencing`.

Pure engine + config + validation. No CLI surface, no TUI. STORY-123 + STORY-121 consume.

## Acceptance Criteria covered

- AC1: empty toml → resolved priority vocab == MoSCoW (`must=4, should=3, could=2, wont=1`).
- AC2: custom `[priorities.*]` blocks → resolved vocab == only user keys, no MoSCoW merged.
- AC3: doc w/ unknown priority key → validation error names key.
- AC4: doc of `requires_priority=true` type, no priority → validation error names type.
- AC5: doc of `requires_priority=false` type, no priority → accepted.
- AC6: `Config::priority_weights()` returns map matching parsed config exactly.
- AC7: no `terminal_statuses` overrides → resolved sets per type match RFC-041 (RFC/Story `{complete,superseded,rejected}`; Iteration/Audit `{complete}`; ADR/Convention/Dictum `{accepted,superseded}`).
- AC8: toml override on a type → only override honoured, default not merged.
- AC9: STORY-120 existing terminal tests pass after migration; hardcoded match in `engine::sequencing::is_terminal` removed.

## Test Plan

All tests `#[cfg(test)] mod tests` at bottom of touched source files. Pure in-memory fixtures. Deterministic. Reuse iter 162/163 fixture builders where applicable. DICTUM-004 conformant.

### `src/engine/config.rs` tests

- AC1 — `parse("")` (empty toml) → assert `cfg.priority_weights()` == `BTreeMap{must:4, should:3, could:2, wont:1}`. BTreeMap for deterministic key order.
- AC2 — parse toml w/ `[priorities.high] weight=10\n[priorities.low] weight=1` only → assert keys == `{high, low}` exact set; no `must`/`should`/`could`/`wont`.
- AC6 — parse toml w/ 3 custom blocks → assert `priority_weights()` returned map equals 3-entry expected map (key + weight).
- AC7-a — `Config::default()` → assert `cfg.documents.type_by_name("rfc").unwrap().resolved_terminal_statuses()` == `[Complete, Superseded, Rejected]` (set equality, normalize order).
- AC7-b through AC7-g — parametric: one assertion per (type, expected set) pair: `story`→`{Complete,Superseded,Rejected}`; `iteration`→`{Complete}`; `audit`→`{Complete}`; `adr`→`{Accepted,Superseded}`; `convention`→`{Accepted,Superseded}`; `dictum`→`{Accepted,Superseded}`.
- AC8 — parse toml w/ `[[types]] name="rfc" terminal_statuses=["accepted"]` (override) → `type_by_name("rfc").resolved_terminal_statuses()` == `[Accepted]` only; `Complete` absent.
- AC8-b — partial-override regression: same toml override on `rfc`; assert `story` (not overridden) still has spec default `{Complete,Superseded,Rejected}`.
- requires_priority defaults — parametric: `default()` config; assert `type_by_name("story").resolved_requires_priority() == true`, `iteration` == true, `rfc`/`adr`/`audit`/`convention`/`dictum`/`spec` == false.

### `src/engine/validation.rs` tests

- AC3 — store w/ DocMeta carrying `priority = Some("bogus")`, default config → `validate_full` returns issue containing substring `bogus` AND substring `priority`. Severity error.
- AC4 — DocMeta of type `story`, `priority = None`, default config → issue containing `story` AND `priority`. Severity error.
- AC5 — DocMeta of type `rfc`, `priority = None`, default config → no priority-related issue (filter by message substring `priority`; assert empty).
- AC5-b — DocMeta of type `story` w/ valid priority `Some("must")` → no priority issue.

### `src/engine/sequencing.rs` tests

- AC9 — existing `is_terminal_per_type_status_table` and `is_terminal_rejects_accepted_for_work_item_types` pass against new signature. Adapt call sites to thread `&Config::default()`. Same expected outcomes.
- AC9-b — grep guard: `cargo test` plus a one-shot test asserts `is_terminal` honours config: build a `Config` w/ override `[[types]] name="story" terminal_statuses=["accepted"]`; call `is_terminal(&story_accepted_doc, &config)` → true. Confirms config-driven, not hardcoded.

## Changes

Tasks self-contained for zero-context subagent. Each lists ACs, files, intent, verification.

### 1. Add `Priority` value + `priority` field on `DocMeta`

- ACs: foundation for 3, 4, 5, 6.
- File: `src/engine/document.rs`.
- Intent:
  - Add `pub type PriorityKey = String` (or new `pub struct Priority(pub String)` newtype if downstream prefers; pick `String` for first impl, no second consumer yet — Principle 6).
  - Add field `pub priority: Option<String>` to `DocMeta` (Option since not all types require it).
  - Add field `priority: Option<String>` to `RawFrontmatter` w/ `#[serde(default)]`.
  - Populate in `from_frontmatter` parser (line ~278): `priority: raw.priority`.
  - Update all `DocMeta { ... }` literal constructions (test fixtures, virtual docs at ~336) to include `priority: None`.
- Verify: `cargo build`. Run existing `cargo test engine::document`.

### 2. Add `[priorities.*]` parsing + `Config::priority_weights()`

- ACs: 1, 2, 6.
- File: `src/engine/config.rs`.
- Intent:
  - Add `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)] pub struct PriorityDef { pub weight: u32 }`.
  - Extend `RawConfig` w/ `#[serde(default)] priorities: Option<HashMap<String, PriorityDef>>`.
  - Extend `DocumentConfig` w/ `pub priorities: BTreeMap<String, u32>` (BTreeMap for determinism per DICTUM-004).
  - In `Config::parse`: `let priorities = raw.priorities.map(|m| m.into_iter().map(|(k,v)| (k, v.weight)).collect()).unwrap_or_else(default_priorities);`. `default_priorities()` returns `{must:4, should:3, could:2, wont:1}` BTreeMap.
  - In `Config::default()`: same `default_priorities()`.
  - Add `impl Config { pub fn priority_weights(&self) -> &BTreeMap<String, u32> { &self.documents.priorities } }`.
- Verify: `cargo test engine::config::tests` covers AC1, AC2, AC6 fixtures from Test Plan.

### 3. Extend `TypeDef` w/ `requires_priority` + `terminal_statuses`

- ACs: 4, 5, 7, 8.
- File: `src/engine/config.rs`.
- Intent:
  - Add fields to `TypeDef`:
    - `#[serde(default)] pub requires_priority: Option<bool>`
    - `#[serde(default)] pub terminal_statuses: Option<Vec<crate::engine::document::Status>>`
  - Both `Option`, so `Default` not required for the field. Resolution helpers fall back to spec defaults when None.
  - Add free fns:
    - `fn default_requires_priority(type_name: &str) -> bool` → true for `story`, `iteration`; false otherwise.
    - `fn default_terminal_statuses(type_name: &str) -> Vec<Status>` → RFC-041 table (RFC/Story `{Complete,Superseded,Rejected}`; Iteration/Audit `{Complete}`; ADR/Convention/Dictum `{Accepted,Superseded}`; everything else `[]`).
  - Add methods on `TypeDef`:
    - `pub fn resolved_requires_priority(&self) -> bool { self.requires_priority.unwrap_or_else(|| default_requires_priority(&self.name)) }`
    - `pub fn resolved_terminal_statuses(&self) -> Vec<Status> { self.terminal_statuses.clone().unwrap_or_else(|| default_terminal_statuses(&self.name)) }`
  - Update `build_type_def` + literal `TypeDef { ... }` constructions (default_types, test_fixture, ~344, ~384) to set new fields = `None`.
  - `Status` already serde-deserializable via `#[serde(rename_all = "lowercase")]`-friendly `FromStr` (verify; if not, add serde `Deserialize` manually).
- Verify: `cargo test engine::config::tests` covers AC7, AC8, requires_priority defaults.

### 4. Migrate `is_terminal` to config-driven

- ACs: 9.
- Files: `src/engine/sequencing.rs`, all callers.
- Intent:
  - Change signature: `pub fn is_terminal(doc: &DocMeta, config: &Config) -> bool`.
  - Body: `let Some(td) = config.documents.type_by_name(doc.doc_type.as_str()) else { return false; }; td.resolved_terminal_statuses().contains(&doc.status)`.
  - Remove the hardcoded match block entirely (lines 567-579).
  - Update internal call sites (`is_node_terminal` closures at ~356 and ~521; any other `is_terminal(d)` references) to thread `&Config`.
  - Update `next_ready` signature: `pub fn next_ready(graph: &Graph, opts: &NextOpts, leases: &LeaseView, config: &Config) -> NextResult`. Existing test fixtures pass `&Config::default()`.
  - If `Graph` already holds `&Config`, prefer that over an extra param. Check current iter 162/163 ctor; if `Graph::from_store(&Store)` already has access to `&Config`, store it on `Graph` and read `graph.config()` rather than threading. Pick whichever is fewer changes.
- Verify: `cargo test engine::sequencing::tests` — AC9 existing tests pass under new signature.

### 5. Frontmatter validation: priority rules

- ACs: 3, 4, 5.
- File: `src/engine/validation.rs`.
- Intent:
  - Add a new validation pass `validate_priorities(store: &Store, config: &Config) -> Vec<(Severity, ValidationIssue)>`.
  - Iterate docs; skip `validate_ignore`. For each doc:
    - If `doc.priority == Some(key)` and `key` not in `config.priority_weights()` → push `Severity::Error` issue: `"unknown priority key '{key}' on document {doc.id}"`.
    - If `doc.priority.is_none()` and `td.resolved_requires_priority()` → push `Severity::Error`: `"priority field required for type '{type}' on document {doc.id}"`.
  - Wire into `validate_full` (line ~880) — append results to `issues`.
- Verify: `cargo test engine::validation::tests` — covers AC3, AC4, AC5, AC5-b.

### 6. Update fixtures + test seams

- ACs: cleanup; ensures suite green.
- Files: any `DocMeta { ... }` literal in `src/`, `tests/`, `src/engine/sequencing.rs::tests::doc()` helper.
- Intent:
  - Anywhere `DocMeta { ... }` constructed without `priority`, add `priority: None`.
  - In `sequencing.rs` test helper `fn doc(...)`, default `priority: None`. Add a parallel helper `fn doc_with_priority(...)` if any test needs to set it (none in this iter; skip).
  - Anywhere `TypeDef { ... }` constructed without new fields, add `requires_priority: None, terminal_statuses: None`.
- Verify: `cargo build && cargo test`.

### 7. Doc + example update

- ACs: discoverability; non-AC.
- Files: `.lazyspec.toml`, `README.md`.
- Intent:
  - Append to `.lazyspec.toml` example a commented-out section showing `[priorities.must] weight = 4` etc. plus per-type `requires_priority`/`terminal_statuses` examples.
  - README: brief mention of new TypeDef fields + MoSCoW default. Single section under existing config docs.
- Verify: `lazyspec validate --json` passes; manual read of README.

## Notes

### Decisions (locked)

- **Single iteration, not split.** Story body says one PR; both strands share `TypeDef` plumbing. Splitting would force two passes over `TypeDef { ... }` literals + duplicate config tests.
- **`priority: Option<String>` (no newtype).** Validation enforces vocab; engine consumers (graph weighting) read via `config.priority_weights().get(key)`. Newtype wrapper deferred until a second consumer demonstrates need (Principle 6).
- **`BTreeMap` for `priorities`.** Determinism per DICTUM-004. Insertion order from TOML not guaranteed by `HashMap`. Iter through priorities in tests + future critical-path tie-breaks needs stable order.
- **`Option<Vec<Status>>` on `TypeDef::terminal_statuses` (None = use default).** Per AC8, override replaces — does not merge — defaults. None-vs-Some distinction encodes "not configured" vs "explicitly empty".
- **Hardcode lives in `config.rs` defaults, not `sequencing.rs`.** AC9 says hardcode in `engine::sequencing` removed. Spec defaults still need a home; `config::default_terminal_statuses(type_name)` is correct level — config layer owns vocab + lifecycle defaults; sequencing reads via `TypeDef` accessor only.

### Open / deferred

- **Priority newtype.** If STORY-123 critical-path or STORY-121 TUI colour theming both need a typed handle, promote `String` → `Priority(String)` newtype then. Two-uses rule.
- **Per-priority colour in TOML.** RFC-041 defers; not in this iter.
- **`Config` threading vs storing on `Graph`.** Pick whichever yields fewer call-site changes. Subagent decides during impl. Document the decision in iter completion notes.

### Codebase anchors

- `DocMeta` literal sites (need `priority: None`):
  - `src/engine/document.rs:189` (struct), `:336` (virtual_doc default).
  - `src/engine/sequencing.rs::tests::doc()` helper (~588).
  - Any test fixture in `tests/`.
- `TypeDef { ... }` literal sites: `config.rs:311, 332, 344, 583` (test_fixture). Update each.
- `is_terminal` callers: `sequencing.rs:358, 521, 567`. Plus tests at `:764, :793`.
- Status enum: `src/engine/document.rs:90`, derive `Deserialize`. Verify FromStr-compatible serde for TOML; if needed, manually impl.
- Existing `Config::parse` test pattern: `src/engine/config.rs:615+`. Mirror style (raw toml str → assert).
- Validation entry: `validate_full` at `src/engine/validation.rs:880`.

### Caveats

- Working tree currently has `M src/engine/sequencing.rs` and `M docs/stories/STORY-120-engine-blocks-dag-primitives.md`. Likely STORY-120 iter 163 leftovers. **Confirm clean tree** before iter 164 build kicks off; uncommitted changes may collide w/ Task 4 migration.
- Audit type appears in default `TypeDef` list (`.lazyspec.toml`) but iter 162 notes claimed it wasn't. Verify before Task 3: if absent, ensure `default_terminal_statuses("audit")` still returns `{Complete}` so AC7 passes regardless.
