---
title: 'Interactive add-type: attributes, lifecycle, gate and parent relation'
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-226
---
Implements STORY-226. One slice: grow ITERATION-325 add-type wizard past core fields → also prompt attributes, custom lifecycle, parent type, gate. Drive same three writers flag path drive. No new write path. RFC-062 add-type flow (`docs/rfcs/RFC-062-...md` §"Add-type wizard flow", §"Layering", §"Validation & re-prompt").

## Context

Wizard seam + core-field flow already built (ITERATION-325, STORY-225). This slice only extends the collector; every value object + writer exist.

- Prompter trait seam: `src/cli/wizard.rs:7` (`ask`/`confirm`/`select`). Real `StdinPrompter`, test `ScriptedPrompter` (`wizard.rs:92`). CLI layer (Convention P3/P4). Reuse as-is; no new methods needed.
- `run_add_type_interactive` (`src/cli/config.rs:195`) = extension point. Today prompts core fields only, then delegates `run_add_type(... parent=None, ... attributes=&[])` (`config.rs:255-271`). Loop at `config.rs:212` already re-prompts dup name/prefix.
- Writers (all read→mutate→`write_config_in_place`→write, one per pass):
  - `run_add_type` (`config.rs:100`) — takes `parent_type: Option<&str>` + `attributes: &[String]`. Already parses specs via `parse_attr_spec` + rejects dup attr names (`config.rs:125-135`).
  - `run_set_lifecycle` (`config.rs:274`) — replaces states+edges on a type.
  - `run_add_gate` (`config.rs:301`) — sets `require_parent_status` on an existing **parent-child rule by name**; rejects unknown/relation-existence rule. Does NOT validate status membership.
- Validators to reuse: `parse_attr_spec` (`config.rs:351`, module-private) — kind/enum/segment rules. `parse_edge` (`config.rs:335`) — FROM:TO **syntax only**, no state membership.
- Lifecycle model: `Lifecycle{states,edges}` (`config.rs:158`). Decline custom → leave `Lifecycle::default()` (empty) → resolves to preset via `effective_lifecycle()` (`config.rs:1312`); so decline = do NOT call set-lifecycle.
- Gate target lifecycle: rule `parent` type → `effective_lifecycle().states` (`config.rs:1312`). `ParentChild` fields (`config.rs:31`).
- Dispatch already gates TTY + `!--json` (`src/main.rs:658`); no dispatch change.

## Approach

Extend `run_add_type_interactive` body only (after existing core-field loop, before/around the single `run_add_type` call). Four optional sections, each prompt-side pre-validated → re-prompt on fail, never abort session (RFC §"Validation & re-prompt"). Writers stay untouched.

1. **Attributes** — loop: `confirm("Add an attribute", false)`; if yes `ask` spec `NAME:KIND[:required][:VAL,…]` → `parse_attr_spec` → on `Err` print msg + re-ask (same iteration, no dup-add prompt consumed); dup name vs already-collected → same re-ask. Collect `Vec<String>` of raw specs. Pass to `run_add_type` `attributes` arg (drop the `&[]`).
2. **Parent** — build `&[&str]` of existing type names from `config.documents.types`. If empty skip. Else `confirm("Set a parent type")` → `select("Parent", &names, first)`; only listed names selectable (re-ask if answer ∉ names). Pass as `parent_type` to `run_add_type`.
3. **Lifecycle** — `confirm("Design a custom lifecycle", false)`. No → nothing (inherits preset). Yes → collect states (repeat `ask`, blank ends, ≥1 required) then edges (repeat `ask` FROM:TO, blank ends): `parse_edge` for syntax, then membership — `to ∈ states` and (`from ∈ states` or `from=="*"`) else re-ask. After type written, call `run_set_lifecycle(root,fs,&name,&states,&edges)`.
4. **Gate** — collect `ParentChild` rule names from `config.rules`. If none skip. Else `confirm("Gate a parent-child rule")` → `select` rule → `ask` status → resolve rule.parent type `effective_lifecycle().states`; status ∈ that set else re-ask. Call `run_add_gate(root,fs,&rule,&status)`.

Order of writer passes: `run_add_type` (type + attrs + parent) → `run_set_lifecycle` (if custom) → `run_add_gate` (if gated). Same chain as flag callers; each re-reads config so ordering safe.

## Task breakdown

- [ ] Attributes loop in `run_add_type_interactive`; reuse `parse_attr_spec`; pass collected specs to `run_add_type`.
- [ ] Parent select from existing type names; pass to `run_add_type` `parent_type`.
- [ ] Custom-lifecycle collector (states then edges, `parse_edge` + membership re-prompt); call `run_set_lifecycle` only when opted in.
- [ ] Gate collector over existing parent-child rules; status membership check vs parent `effective_lifecycle().states`; call `run_add_gate`.
- [ ] README: extend `config add-type` interactive section — attributes, lifecycle, parent, gate prompts; flags/`--json`/non-TTY path unchanged.

## Acceptance criteria

- **G** wizard running **W** I add attribute `priority:enum:low,medium,high` **T** validated + written; **and** malformed spec (unknown kind, enum w/o values) rejected + re-prompted. (STORY-226 AC1) — test `interactive_add_type_collects_attributes`, `interactive_add_type_reprompts_bad_attr`.
- **G** I opt into custom lifecycle **W** I enter states + edges **T** edge naming a non-existent state rejected + re-prompted; **and** if I decline, type inherits standard preset (no `[lifecycle]` written / empty lifecycle). (STORY-226 AC2) — test `interactive_add_type_custom_lifecycle_reprompts_bad_edge`, `interactive_add_type_declined_lifecycle_inherits_preset`.
- **G** I set a parent **W** offered choices **T** only already-defined types offered; unknown parent not enterable. (STORY-226 AC3) — test `interactive_add_type_parent_only_from_existing`.
- **G** I gate a parent-child rule with a status the parent lifecycle lacks **T** rejected + re-prompted; valid status written. (STORY-226 AC4) — test `interactive_add_type_gate_reprompts_unknown_status`.
- **G** I complete the full flow **T** result is byte-identical to the equivalent non-interactive `add-type`(+attrs+parent) → `set-lifecycle` → `add-gate` flag chain, and reparses clean. (STORY-226 AC5) — test `interactive_full_flow_matches_flag_chain`.

## Test plan

- `ScriptedPrompter`-driven unit tests in `src/cli/config.rs` `mod tests`, no real TTY (extend existing `interactive_*` tests, `config.rs:823+`). Fixture `SRC` (`config.rs:458`) already carries rfc/story types + `stories-need-rfcs` parent-child rule → gate target ready.
- AC5 equivalence: run wizard into fixture A, run flag chain (`run_add_type` + `run_set_lifecycle` + `run_add_gate`) into fixture B, assert file bytes equal (mirrors `interactive_add_type_matches_flag_call`, `config.rs:824`).
- Re-prompt tests assert queue-consume behaviour + no duplicate/abort + final `Config::parse` ok.

## Out of scope / notes

- Creating **new** parent-child rules or **new** named relationship vocabulary: no engine writer exists (only `write_config_in_place`; CLI exposes only add-type/set-lifecycle/add-gate). Gate step operates on rules that already exist. Deferred — not in STORY-226 ACs (AC5 fixes the scriptable equivalent to add-type+set-lifecycle+add-gate). Do NOT invent an add-rule/add-relationship writer.
- Editing an existing type's lifecycle/gates (RFC non-goal). `init` bootstrap wizard (STORY-227/228). Prompt-crate adoption. TUI/web parity — none (CLI-only authoring, produces same `.lazyspec.toml`; RFC §"Impact").
- Conventions: `docs/CONVENTIONS.md` P3 (prompt seam in CLI), P4 (fake at trait seam = `ScriptedPrompter`), P6 (no second config path). `--json`/non-TTY suppression already enforced at `main.rs:658`.
