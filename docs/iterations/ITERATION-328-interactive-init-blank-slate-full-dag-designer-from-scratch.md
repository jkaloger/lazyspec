---
title: Interactive init blank-slate full DAG designer from scratch
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-228
---
<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

Implements STORY-228 (RFC-062 bootstrap wizard, "blank slate / full DAG designer" path). BUILDS ON STORY-227 (ITERATION-327). ONE slice: first-screen "starter vs scratch" branch -> from-scratch design (types -> per-type lifecycle -> parent-child rules+severity -> gates -> relation vocab default -> DAG summary -> confirm) -> existing `write_project`. Reuse `collect_type_interactive`/`apply_collected_type`/`Prompter`/`write_project`. NO second config writer.

## Objective

`lazyspec init` on TTY offers "blank slate" beside STORY-227's "start from starter"; blank slate designs the WHOLE type DAG from nothing (types, lifecycles, parent-child rules with severity, gates), renders a DAG summary, confirms, then scaffolds a `Config` that owes nothing to `starter_config()` and passes `validate`.

## Satisfies

STORY-228 all 6 AC (5 CLI-behaviour + non-func non-TTY suppression).

## Context (file:line)

- STORY-227 delivered the reusable spine — READ IT FIRST:
  - `run_init_interactive(root, prompter)` (`src/cli/init.rs:95-99`): existence bail -> `design_config_interactive(starter_config(), prompter)` -> `write_project`. Blank-slate branch goes HERE.
  - `design_config_interactive(base, prompter) -> Result<Config>` (`src/cli/init.rs:125-155`): PURE, no disk. Author(discard)/naming/keep-drop/add-type loop/write-confirm-else-reloop. MIRROR its purity + confirm-loop shape for the from-scratch twin.
  - `write_project(root, &Config)` (`src/cli/init.rs:72-91`): THE single scaffold+writer path (mkdir per type, templates dir+`template.md`, `scaffold_skeleton_files`, write toml, gh labels, gitignore). REUSE unchanged.
  - `init_is_interactive(non_interactive, json, stdin_tty, stdout_tty)` (`src/cli/init.rs:47-54`): gate already suppresses wizard for `--json`/`--non-interactive`/non-TTY. Blank slate lives INSIDE the interactive branch => inherits suppression free (STORY-228 non-func AC).
- Reusable type collector (`src/cli/config.rs`):
  - `collect_type_interactive(config, prompter) -> Result<CollectedType>` (`config.rs:252-456`): PURE prompt-only; validates name/prefix dup (270-277), parent from defined types (337-353), custom lifecycle states+edges with edge-target re-prompt (358-395), gate on EXISTING parent-child rule with status-in-parent-lifecycle re-prompt (399-439). REUSE for the types loop.
  - `apply_collected_type(config, collected) -> Result<()>` (`config.rs:461-515`): pushes `TypeDef`, applies lifecycle, attaches gate to existing rule. REUSE.
  - `CollectedType` (`config.rs:232-246`); `type_def_from_parts` (`config.rs:168-206`).
- Rule/vocab primitives (`src/engine/config.rs`):
  - `ValidationRule::ParentChild{name,child,parent,severity,require_parent_status}` (`config.rs:31-38`); `RelationExistence` (`40-46`). `Severity{Error,Warning}` `#[serde lowercase]` (`config.rs:10-13`).
  - `default_rules()` (`config.rs:938-961`): references `story/rfc/iteration/adr` — DO NOT seed blank slate with this (dangling-rule trap; see Notes).
  - `starter_relationships()` (`config.rs:414-432`): type-agnostic vocab (implements/supersedes/blocks/related-to). SAFE default for blank slate.
  - `starter_config()` (`src/cli/init.rs:17-43`): naming `{type}-{n:03}-{title}.md` (the `.md` suffix parity default — 327 Notes), `FilesystemConfig`, `UiConfig::default`, `ref_count_ceiling:15`, `CertificationConfig::default`. Blank base copies the NON-type/NON-rule scaffolding from here.
- `run_add_gate` (`config.rs:601-626`) = existing gate-attach reference (attaches `require_parent_status` to a parent-child rule). No CLI command creates parent-child rules today => rule authoring is the NEW surface.
- `Prompter` trait + `ScriptedPrompter` (`src/cli/wizard.rs:7-15,92-139`). REUSE, add nothing.

## Approach

Add a from-scratch PURE designer twin beside `design_config_interactive`, branched by a first-screen select. One writer, one gate function, reuse the type collector.

1. **First-screen branch** — in `run_init_interactive` (`init.rs:95`), after existence bail, `prompter.select("Start from", &["starter","scratch"], "starter")`. `starter` -> existing `design_config_interactive(starter_config(), p)`; `scratch` -> new `design_config_from_scratch(p)`. Both -> same `write_project`. Keeps designers PURE/testable (branch is one select).
2. **Blank base** — helper `blank_config() -> Config`: clone `starter_config()` scaffolding but `documents.types = vec![]` and `rules = vec![]`, KEEP `relationships = starter_relationships()`, naming `{type}-{n:03}-{title}.md`, filesystem/ui/ceiling/certification. Rules EMPTY is the dangling-rule guard.
3. **`design_config_from_scratch(prompter) -> Result<Config>`** — PURE, no disk, confirm-loop like `design_config_interactive`:
   - author (prompt+discard, prefault git user.name — mirror 327), naming (default blank base pattern).
   - **types loop**: `while confirm("Add a type", true-first/false-after)` -> `collect_type_interactive(&config, p)` -> `apply_collected_type`. Require >=1 type before leaving (re-prompt if none). (Per-type gate section inside collector stays inert here: no rules yet.)
   - **parent-child rules loop**: only when >=2 types. `while confirm("Add a parent-child rule", false)`: select child + parent from DEFINED type names (re-prompt unknown), auto-name `"{child_plural?}-need-{parent}"` or `"{child}s-need-{parent}s"` (stable, dedup-guarded), `select("Severity",&["warning","error"],"warning")`, then optional `confirm("Gate on a parent status")` -> pick status from parent's `effective_lifecycle().states` (re-prompt unknown) -> `require_parent_status`. Push `ValidationRule::ParentChild`.
   - relation vocab: default `starter_relationships()` (already on base); no authoring UI this slice (see Out of scope).
   - **DAG summary**: `render_dag_summary(&config)` -> println types (name/plural/dir/prefix/store), each type's effective lifecycle states+edges, parent-child rules (child->parent, severity, gate status), relation vocab names. Then `confirm("Write this config", true)`; `n` -> discard clean + reloop (no IO).
4. **Severity parse** — small `parse_severity(&str)->Result<Severity>` (warning/error) in `config.rs`, reused by the rule loop.
5. **README** — document blank-slate DAG designer beside 327's starter path; `--non-interactive`/`--json`/non-TTY still write `starter_config()` (project rule: CLI change -> README).

## Task breakdown

1. `src/cli/init.rs`: add `blank_config()` helper (starter scaffolding, empty types+rules, starter relationships).
2. `src/cli/init.rs`: add `design_config_from_scratch(prompter)` — author/naming/types-loop(>=1)/rules-loop/summary/confirm-reloop; PURE.
3. `src/cli/init.rs`: `render_dag_summary(&Config) -> String` (or println helper) listing types+lifecycles+rules+gates+relations.
4. `src/cli/init.rs`: branch `run_init_interactive` on `select("Start from",["starter","scratch"],"starter")`.
5. `src/cli/config.rs`: add `parse_severity`; expose a small pure rule-collect helper if it keeps `design_config_from_scratch` readable (else inline). Reuse `collect_type_interactive`/`apply_collected_type` as-is.
6. `README.md`: blank-slate designer section; note suppression unchanged.

## Acceptance criteria (each test-backed)

- **AC1 lifecycle/gate ref only defined states** (STORY-228 AC1): **Given** `ScriptedPrompter` adding a type with a custom lifecycle whose edge, then a gate status, name an UNDEFINED state first, **When** `design_config_from_scratch`, **Then** each invalid ref re-prompts and only defined states/statuses are accepted. -> `test scratch_lifecycle_and_gate_reject_unknown_states` (drives collector re-prompt reuse).
- **AC2 parent-child rule from defined types + severity** (AC2): **Given** two defined types, `ScriptedPrompter` adds a rule picking child+parent from them and severity=error, **When** design, **Then** a `ValidationRule::ParentChild{child,parent,severity:Error}` is present; an unknown child/parent re-prompts. -> `test scratch_parent_child_rule_defined_types_and_severity`.
- **AC3 DAG summary + confirm** (AC3): **Given** a designed DAG, **When** the wizard renders the summary, **Then** output names every type, its lifecycle, every rule+gate, and the relation vocab, and a `confirm("Write this config")` is asked before returning. -> `test scratch_summary_lists_dag` (assert rendered string contains type/lifecycle/rule/relation tokens).
- **AC4 confirm -> valid scaffold** (AC4): **Given** a full from-scratch design (>=2 types, >=1 rule, custom + inherited lifecycles), **When** `write_project` scaffolds it into a temp dir, **Then** per-type dirs + `template.md` exist, config LOADS, and `validate_full` returns NO errors (no dangling rules/relationships). -> `test scratch_scaffold_validates_clean` (temp-dir round-trip, mirror `init.rs:474-508`).
- **AC5 decline at summary -> nothing written** (AC5): **Given** `ScriptedPrompter` answering write-confirm=`n` then aborting, **When** `run_init_interactive` on a temp root, **Then** no `.lazyspec.toml` and no per-type dirs are created (design is pure; `write_project` never runs). -> `test scratch_decline_writes_nothing`.
- **AC6 non-TTY/non-interactive suppressed** (AC6 non-func): **Given** `--json`/`--non-interactive`/non-TTY, **When** init dispatches, **Then** blank slate is unreachable and `starter_config()` is written. -> reuse `test json_suppresses_interactive` (`init.rs:385`) + `test init_noninteractive_writes_starter` (branch select never constructed when non-interactive).

## Test plan

- All unit, `ScriptedPrompter`-driven, NO real TTY (mirror `init.rs:307+` / `config.rs` scripted tests).
- From-scratch happy path: >=2 types (one custom lifecycle, one inherited), 1 parent-child rule + gate, accept relation-vocab default, write=y -> assert types/rules/relations on returned `Config`.
- Re-prompt coverage: unknown lifecycle edge state, unknown gate status, unknown rule child/parent, dup type name/prefix (collector reuse) each re-ask not abort.
- Blank base parity: `blank_config()` has empty types+rules, `starter_relationships()`, starter naming pattern.
- Scaffold round-trip: temp dir `write_project(tmp,&designed)` -> per-type dirs + template + skeletons -> `Config::load` -> `validate_full` errors empty (explicit dangling-rule assertion: no rule references an absent type).
- Decline path: write=n -> reloop then abort -> no files/dirs on disk.
- Suppression: `init_is_interactive` false for json/non-interactive/non-TTY (existing tests hold).

## Out of scope

- **Relation-vocabulary authoring UI** (RFC-062 §5 "advanced users add named relations w/ inverses"): this slice DEFAULTS to `starter_relationships()` and does not prompt to add/edit relations. Deferred — a follow-up may add a relation-authoring loop. (Keeps slice bounded; blank slate is still valid because default vocab is type-agnostic.)
- **Editing** an existing type/rule/lifecycle interactively (RFC-062 non-goal: add not edit).
- **Persisting default author** — no `Config.default_author` field (`config.rs:477`); prompt+discard as in 327.
- **STORY-227 starter path internals** — unchanged; blank slate only adds the first-screen branch + twin designer.
- **Remote-store auth** (`setup github-issues`/`clickup`) — `write_project` may already emit labels via `ensure_github_labels`; no new auth flow.
- **TUI/web** — CLI-only authoring; produces standard `.lazyspec.toml` all layers already read (RFC-062 §parity). No TUI/web work.

## Notes

- Convention: prompt seam in CLI layer (P3); `Prompter` fake at seam (P4); non-TTY/`--json` -> unchanged non-interactive path (P2, byte parity via `starter_config()`); ONE writer `write_project` + ONE gate-attach shape (P6) — NO second config write path.
- **Dangling-rule trap (STORY-228 core validity concern)**: `blank_config().rules` MUST start EMPTY. Seeding `default_rules()` would reference `story/rfc/adr` that a from-scratch DAG need not contain -> `validate` errors on rules pointing at absent types. Rules are BUILT only from the user's authored parent-child steps, so every rule endpoint is a defined type by construction. Relation vocab is type-agnostic so `starter_relationships()` is safe.
- **Naming parity trap (from 327)**: default the naming prompt to `{type}-{n:03}-{title}.md` (WITH `.md`), i.e. `starter_config().documents.naming.pattern`, not the story's suffix-less form.
- **Ordering** (RFC-062 §"Ordering matters"): types -> (per-type lifecycle inside collector) -> parent-child rules -> gates. Gates can only attach after rules exist, and rules only reference already-defined types; the prompt sequence enforces this, not post-hoc validation. The collector's own per-type gate section stays inert during the types loop (no rules yet); real gating happens in the rules loop.
- Require >=1 type before finishing (a zero-type config is degenerate); parent-child rule loop only offered when >=2 types exist.
- Blank slate is opt-in: default the first-screen select to `starter` so the STORY-227 path stays the zero-effort default (RFC-062 step 7 "start from the starter DAG and tweak" as first choice).
