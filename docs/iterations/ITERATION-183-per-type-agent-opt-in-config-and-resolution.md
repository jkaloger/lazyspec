---
title: Per-type agent opt-in config and resolution
type: iteration
status: complete
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-134
---

## Context

Slice 3 of RFC-046 (STORY-134). Today agent mode always on -> every doc type offers same fixed action set regardless of project intent. ADR-016 moves gating per-type: `agents: Vec<String>` on `TypeDef`, off by default. Entries = template file STEMS under `.lazyspec/agents/` (e.g. `"expand"`, `"create-children"`).

Zero defaults is organising principle (ADR-011 "Strict config load with no engine-baked defaults" + ADR-015 "Agent prompts are user-authored templates with zero engine defaults"). Engine bakes no ontology -> no built-in action list, nothing for `init` to write. Absent/empty `agents` = valid "off" state, NOT load error. Strict-load applies to field SHAPE only (must be list-of-strings if present); does NOT inject defaults.

This slice = field + resolution fn only. Given doc's type -> intersect type's `agents` list with templates that actually LOADED -> action set for that doc. Config name w/ no matching loaded template -> reported (user named action they didn't author). Loaded template referenced by no type -> simply unused (no report). NO global toggle; gating stays per-type.

Dep (OUT OF SCOPE, consumed not built):
- slice 2 / STORY-133 (ITERATION-182): `AgentPrompt` discovery + load. This slice CONSUMES loaded template set; depends on `AgentPrompt` (carries frontmatter `name`) existing in engine. ITERATION-182 still stub at authoring time -> consume only the template `name` identity (the stem), not its body/render. Resolution intersects on `name`.
- slice 1 / STORY-132 (ITERATION-181): `AgentRunner` / `AgentContext` / `AgentHandle`. Not touched here.
- slice 4 / STORY-135 (ITERATION-184): TUI dialog displays resolved set. This slice PROVIDES the resolution fn the dialog calls; dialog wiring out of scope.
- slice 5 (ITERATION-185): interactive run mode + global `[agents]` block. Distinct from per-type `[[types]].agents`. Out of scope.

Verified anchors:
- `src/engine/config.rs:134` `pub struct TypeDef` -> add field.
- `src/engine/config.rs:365` `starter_types()` (closure `simple` + two explicit literals) -> must set field or won't compile.
- `src/engine/config.rs:696` `TypeDef::test_fixture` (`#[cfg(test)]`) -> must set field or test build breaks.
- `src/engine/config.rs:639` `Config::type_by_name(&str) -> Option<&TypeDef>` -> resolution entry point.
- `src/engine/agent.rs` -> home for resolution fn (engine layer; no I/O; DICTUM principle 3 deps flow inward).

## Test Plan

DICTUM-004 (`cargo run --quiet -- convention --tags iteration,testing --json`): each test isolated (own fixtures), behavioral (assert on returned action set + report, not internals), deterministic (no clock/net/process), structure-insensitive (public fns). Field-shape tests inline in `src/engine/config.rs` `#[cfg(test)] mod tests` (mirror existing `store`/`certification` parse tests). Resolution tests inline in `src/engine/agent.rs` `#[cfg(test)] mod tests`, building `AgentPrompt` set + `TypeDef` fixtures by hand (fast, no FS, no process spawn).

Resolution fn does NOT need `AgentPrompt` bodies -> tests construct minimal `AgentPrompt`-name set (or `Vec<&str>` of loaded stems if fn takes name slice — see Notes signature decision). Keep tests coupled to the loaded-NAME identity, not template internals, so slice-2 churn doesn't break them.

- **AC1 — absent `agents` key => off, not error.**
  Test `type_without_agents_key_parses_and_is_empty` (inline `src/engine/config.rs`).
  Given: TOML `[[types]]` entry, NO `agents` key (reuse `TYPES` const preamble pattern). When: `Config::parse`. Then: load `Ok`; `config.type_by_name("rfc").unwrap().agents` is empty. Asserts the `#[serde(default)]` => empty path AND that absence is not a `bail!`.

- **AC2 — empty list => off.**
  Test `type_with_empty_agents_list_parses_empty` (inline `config.rs`).
  Given: `[[types]]` w/ `agents = []`. When: `Config::parse`. Then: `Ok`; that type's `agents.is_empty()`. Distinguishes explicit empty from absent (both => off, neither errors).

- **AC3 — listed stems resolve to that action set (intersection).**
  Test `resolve_intersects_type_agents_with_loaded` (inline `src/engine/agent.rs`).
  Given: `TypeDef` w/ `agents = ["expand", "create-children"]`; loaded template set = {`expand`, `create-children`} (+ maybe an extra unrelated loaded name to prove intersection not union). When: call resolution fn for that type. Then: resolved action set == exactly {`expand`, `create-children`}; missing-report empty. Behavioral: assert on returned set membership, order-insensitive.

- **AC4 — listed-but-missing template reported.**
  Test `resolve_reports_named_but_missing` (inline `agent.rs`).
  Given: `TypeDef` `agents = ["expand", "nonexistent"]`; loaded set = {`expand`} only. When: resolve. Then: resolved set == {`expand`} (nonexistent excluded); missing-report contains `"nonexistent"` (and ONLY it). Asserts both the intersection drop AND the structured missing-name surfacing.

- **AC5 — unreferenced loaded template is unused, not error.**
  Test `resolve_ignores_unreferenced_loaded_template` (inline `agent.rs`).
  Given: loaded set = {`expand`, `orphan`}; `TypeDef` `agents = ["expand"]` (no type references `orphan`). When: resolve. Then: `Ok`/no error; resolved set == {`expand`}; `orphan` absent from resolved set AND absent from missing-report. Proves an extra loaded template is silently dropped, never reported.

- **AC6 — resolution per-type, independent.**
  Test `resolve_is_per_type_independent` (inline `agent.rs`).
  Given: type A `agents = ["expand"]`, type B no `agents` key (empty); loaded set = {`expand`}. When: resolve A then resolve B. Then: A => {`expand`}; B => empty set, no error; neither call affects the other (A's report empty, B's report empty). Asserts gating is per-type with no global coupling.

- **AC2-shape (strict-load) — malformed `agents` shape rejected.**
  Test `type_with_malformed_agents_shape_is_error` (inline `config.rs`).
  Given: `[[types]]` w/ `agents = "expand"` (string, not list) OR `agents = [1, 2]` (non-string elems). When: `Config::parse`. Then: returns `Err` (serde type error). Asserts strict-load validates field SHAPE (list-of-strings) even though empty/absent is allowed. Mirror existing `bail!`/parse-error tests; assert on `is_err()` (serde msg, not engine `bail!`).

## Changes

Sequenced. Task 1 (field) must compile before Task 2 (resolution consumes the field). Each task zero-context-ready: exact paths, full impl, verify cmds.

### Task 1 — add `agents: Vec<String>` to `TypeDef` (+ serde default, fix all ctors)
**ACs:** 1, 2, AC2-shape (and foundation for 3-6).
**Files:** `src/engine/config.rs`.
**Do:**
- In `pub struct TypeDef` (~line 134), after `parent_type` field, add:
  ```rust
  #[serde(default)]
  pub agents: Vec<String>,
  ```
  `Vec<String>` w/ `#[serde(default)]` -> absent key deserializes to empty vec (AC1); explicit `[]` deserializes to empty vec (AC2); strict-load still rejects non-list / non-string-elem (serde type error, AC2-shape) because the field type is `Vec<String>`. No custom default fn needed (`Vec::default()` = empty).
- Fix every `TypeDef` literal so the crate compiles:
  - `starter_types()` (~line 365): in the `simple` closure constructor add `agents: Vec::new(),`; in BOTH explicit `TypeDef { ... }` literals (convention ~line 390, dictum ~line 402) add `agents: Vec::new(),`.
  - `TypeDef::test_fixture` (`#[cfg(test)]`, ~line 696): add `agents: Vec::new(),`.
- Field placement after `parent_type` keeps `to_toml` output stable-ish and serde `RawConfig.types: Option<Vec<TypeDef>>` (line 346) picks it up automatically (TypeDef deserializes via RawConfig).
**Verify:**
- `cargo build` (all `TypeDef` ctors updated or it won't compile — that's the compile-time guard).
- `cargo clippy --all-targets`.
- Add inline tests `type_without_agents_key_parses_and_is_empty` (AC1), `type_with_empty_agents_list_parses_empty` (AC2), `type_with_malformed_agents_shape_is_error` (AC2-shape) per Test Plan; reuse the `TYPES`/`RELATIONSHIPS` consts pattern already in the test module.
- `cargo test -p lazyspec config`.

### Task 2 — resolution fn: type's `agents` ∩ loaded templates -> action set + missing report
**ACs:** 3, 4, 5, 6.
**Files:** `src/engine/agent.rs`.
**Do:**
- Add result type carrying resolved set + missing-name report (structured, NOT a stderr warning — see Notes):
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ResolvedAgents {
      /// Template stems from the type's `agents` list that DID load. Order
      /// follows the type's `agents` list (config order, stable).
      pub actions: Vec<String>,
      /// Stems named in the type's `agents` list with no matching loaded
      /// template (user named an action they did not author).
      pub missing: Vec<String>,
  }
  ```
- Add resolution fn. Takes the type's `agents` list + the loaded template NAME set (consumes slice-2 `AgentPrompt` identity, not bodies — see Notes for why a `&[String]` of names, not `&[AgentPrompt]`):
  ```rust
  /// Resolve a document type's opt-in `agents` list against the set of
  /// templates that actually loaded. Returns the action set (the intersection,
  /// in the type's declared order) plus the names declared but not loaded.
  /// An empty `type_agents` yields an empty action set with no missing
  /// entries (agent mode off for the type) — this is not an error.
  pub fn resolve_agent_actions(type_agents: &[String], loaded: &[String]) -> ResolvedAgents {
      let mut actions = Vec::new();
      let mut missing = Vec::new();
      for name in type_agents {
          if loaded.iter().any(|l| l == name) {
              actions.push(name.clone());
          } else {
              missing.push(name.clone());
          }
      }
      ResolvedAgents { actions, missing }
  }
  ```
  - Intersection in type-declared order (AC3): iterate `type_agents`, keep those present in `loaded`.
  - Named-but-missing (AC4): not in `loaded` -> push to `missing`, NOT to `actions`.
  - Unreferenced loaded template (AC5): a name in `loaded` but not in any `type_agents` is never visited -> never appears in `actions` OR `missing`. Silent, by construction.
  - Per-type (AC6): fn is pure over its two args; calling for type A vs type B shares no state. Caller passes `config.type_by_name(doc_type).map(|t| &t.agents)`; absent/empty list -> empty `actions`, empty `missing`, no error.
- Caller-facing convenience (optional, only if a second concrete consumer exists — DICTUM principle 6, don't add indirection for one use). The TUI dialog (slice 4) is the consumer; it holds a `&Config` and the loaded `AgentPrompt` set. If slice 4 needs `(config, doc_type, loaded) -> ResolvedAgents`, add it there or as a thin `Config`/engine wrapper THEN. For THIS slice ship the pure fn over `&[String]` + `&[String]`; the dialog maps its `AgentPrompt`s to their `name`s. Do NOT pre-build the wrapper.
**Verify:**
- `cargo build`.
- `cargo clippy --all-targets`.
- Add inline `#[cfg(test)] mod tests` cases in `src/engine/agent.rs`: `resolve_intersects_type_agents_with_loaded` (AC3), `resolve_reports_named_but_missing` (AC4), `resolve_ignores_unreferenced_loaded_template` (AC5), `resolve_is_per_type_independent` (AC6) per Test Plan. Build the `type_agents` and `loaded` slices inline as `vec!["expand".to_string(), ...]`; no FS, no `AgentPrompt` construction needed (consume names only).
- `cargo test -p lazyspec agent`.

### Task 3 — wire missing-name reporting (surface the report; no new code path if dialog absent)
**ACs:** 4 (the "reported" half).
**Files:** `src/engine/agent.rs` (the `ResolvedAgents.missing` field is the surface).
**Do:**
- "Reporting" = the structured `ResolvedAgents.missing` field returned by `resolve_agent_actions`. No stderr warning emitted from the engine (engine carries no I/O assumptions — DICTUM principle 3; surfacing to the user is the caller's job). The slice-4 dialog (and any CLI `--json` consumer) reads `.missing` and presents it; that presentation is OUT OF SCOPE here.
- This task is the explicit confirmation that `missing` is part of the public return contract (not logged-and-dropped), so slice 4 can render it. No additional implementation beyond Task 2's struct + fn.
- Doc-comment `ResolvedAgents.missing` to state it is the named-but-missing report the dialog surfaces (already in Task 2 snippet).
**Verify:**
- AC4 test `resolve_reports_named_but_missing` (Task 2) already asserts `missing == ["nonexistent"]` -> proves the report is returned, not discarded.
- `cargo test -p lazyspec agent`.

### README
Per CLAUDE.md: if `README.md` documents the `[[types]]` config schema, add the optional `agents = ["stem", ...]` key (template stems under `.lazyspec/agents/`; absent/empty => agent mode off for that type). No CLI-interface change in this slice (no new command/flag), so README change is schema-doc only — skip if README does not enumerate `[[types]]` keys.

## Notes

**Verified paths (file:symbol):**
- `src/engine/config.rs:134` `pub struct TypeDef` — field added here, `#[serde(default)] pub agents: Vec<String>`.
- `src/engine/config.rs:365` `starter_types()` — `simple` closure + 2 explicit `TypeDef` literals (convention, dictum) need `agents: Vec::new()` or no compile.
- `src/engine/config.rs:696` `TypeDef::test_fixture` (`#[cfg(test)]`) — needs `agents: Vec::new()` or test build breaks.
- `src/engine/config.rs:346` `RawConfig.types: Option<Vec<TypeDef>>` — types deserialize via RawConfig (NOT the derive on `DocumentConfig`, which `#[serde(skip_deserializing)]`s `types`); the new field flows through automatically.
- `src/engine/config.rs:639` `Config::type_by_name(&str) -> Option<&TypeDef>` — caller's entry: `type_by_name(doc_type).map(|t| t.agents.as_slice())`.
- `src/engine/agent.rs` — resolution fn + `ResolvedAgents` land here (engine layer, pure, no I/O; existing module already holds agent-id resolution).
- `src/engine/document.rs:54` `DocType(String)` — a document's type identity; resolution keys off the type NAME string (`doc_type: &str` via `type_by_name`).

**Decisions:**
- **Resolution fn placement:** `src/engine/agent.rs` (engine). Pure fn, no FS/process. CLI/TUI depend inward on engine (DICTUM principle 3); slice-4 dialog (TUI) calls it. Keeps the per-type gating logic engine-owned and unit-testable without a terminal.
- **Signature — `&[String]` of names, not `&[AgentPrompt]`:** resolution needs only the loaded template IDENTITY (the stem/`name`), never the body, mode, or `allowed_tools`. Taking `loaded: &[String]` (the `AgentPrompt.name`s) decouples this slice from slice-2's still-stub `AgentPrompt` shape: ITERATION-182 can change `AgentPrompt`'s fields freely without breaking resolution or its tests. Caller (slice 4) maps `&[AgentPrompt]` -> `Vec<String>` of names. Signature: `resolve_agent_actions(type_agents: &[String], loaded: &[String]) -> ResolvedAgents`.
- **"Missing template" surfaced as STRUCTURED RESULT, not warning:** `ResolvedAgents.missing: Vec<String>`. Engine returns it; caller decides presentation (TUI dialog line, CLI `--json` field). No engine-side `eprintln!`/log — engine makes no I/O/terminal assumption (DICTUM principle 3). This also makes AC4 trivially behavioral (assert on returned `missing`, no log capture).
- **Order:** `actions` follows the type's declared `agents` order (config order, stable, deterministic per DICTUM-004). `missing` likewise in declared order.
- **Empty/absent = off, by construction:** empty `type_agents` -> the fn's loop runs zero times -> empty `actions`, empty `missing`. No special-case branch, no error. Matches ADR-011/ADR-015/ADR-016.
- **Relation to slice-2 discovery output:** slice 2 produces the loaded `AgentPrompt` set (discovery of `.lazyspec/agents/*.md`). This slice consumes only their `name`s as the `loaded` arg. If slice 2 lands a typed "discovery result" wrapper, slice 4 unwraps it to `Vec<String>` of names before calling resolution — resolution stays agnostic.
- **`#[serde(default)]` chosen over a custom default fn:** `Vec<String>`'s `Default` is empty; matches how `subdirectory: bool`, `numbering`, `store`, `parent_type: Option<String>` already use bare `#[serde(default)]` on `TypeDef` (config.rs:140-149). Idiomatic (DICTUM principle 5), no new helper (principle 6).
- **No global toggle (ADR-016):** gating is the per-type `[[types]].agents` list only. The global `[agents]` block (interactive command) is slice 5 and is unrelated to this resolution.
