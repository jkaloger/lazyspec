---
title: Pick-from-list prompts and multi_select in the wizard Prompter
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-229
- blocks: ITERATION-331
---

# Iteration: pick-from-list prompts and `multi_select` in the wizard `Prompter`

## Objective

Wizard single-choice prompts become a real numbered chooser (number OR exact option text, out-of-list rejected + re-asked in real impl); add `multi_select` for choosing several (lifecycle states). CLI-only; engine untouched.

## Satisfies

STORY-229 AC group "Multi-select / pick-from-list prompts" (all three) + cross-cutting layering AC. Blank-default first screen -> Slice 2; colours -> Slice 3 (see Out of scope).

## Context

- Parent story + AC text: `docs/stories/STORY-229-interactive-init-wizard-ux-polish.md` (§Scope bullet 1, §"Multi-select / pick-from-list prompts"). Do NOT restate.
- Convention principles 2/3/4: cited inline below.
- Touch:
  - `src/cli/wizard.rs` — `Prompter` trait, `StdinPrompter<R,W>`, `ScriptedPrompter`.
  - `src/cli/config.rs` — `collect_type_interactive`, `collect_parent_child_rule` (select callsites + lifecycle-states loop).
  - `src/cli/init.rs` — `run_init_interactive` "Start from" select; test scripts `full_scratch_answers`, `scratch_lifecycle_and_gate_reject_unknown_states`.
  - `README.md` — interactive add-type flow prose (~L537-544) IF it documents prompt rendering.

## Callsite inventory (ground truth — read before touching)

Single-choice `select` callsites:
1. `config.rs:283` — `select("Store", &[filesystem, github-issues, github-milestones, github-projects, git-ref, clickup-tasks], "filesystem")`. Static list. Return later parsed by `parse_store` (bails on unknown).
2. `config.rs:295` — `select("Numbering", &[incremental, sqids, reserved], "incremental")`. Static. `parse_numbering`.
3. `config.rs:301` — `select("Authorship", &[human, assisted, generated], default_authorship)`. Static. `parse_authorship`.
4. `config.rs:345` — `select("Parent", &type_names, type_names[0])` inside outer re-ask loop (L344-350). Dynamic list.
5. `config.rs:414` — `select("Rule", &rule_names, rule_names[0])` inside outer re-ask loop (L413-419). Dynamic.
6. `config.rs:515` — `select("Severity", &[warning, error], "warning")`. Static. `parse_severity`.
7. `config.rs:502` — `pick` closure `select(label, &type_names, type_names[0])` inside outer re-ask loop (L501-508); called for "Child type" (L510) and "Parent type" (L511). Dynamic.
8. `init.rs:102` — `select("Start from", &[starter, scratch], "starter")`. Static.

Lifecycle-states collection (the "multiple verbatim values" to migrate -> `multi_select`):
- `config.rs:359-370` in `collect_type_interactive`: `loop { ask("Lifecycle state (blank to finish)", None) ... }`, blank-to-finish, guard "at least one state is required" when empty. Shared by BOTH `design_config_interactive` and `design_config_from_scratch` (both call `collect_type_interactive`).
- Edges loop `config.rs:371-391` (`ask("Edge FROM:TO ...")` + `parse_edge`) is FROM:TO, NOT a list-pick -> LEAVE AS-IS this slice.

## Design decisions (resolve before coding)

- **Validation lives in the real impl only.** Trait doc + `StdinPrompter::select` reject out-of-list and re-prompt. `ScriptedPrompter::select` stays verbatim (principle 4 — fake returns queued answers, no validation). Consequence -> the existing OUTER re-ask loops for dynamic lists (callsites 4,5,7 at L344-350, L413-419, L501-508) MUST REMAIN: they are the seam the `ScriptedPrompter` tests drive (feeding `bogus`/`ghost` then a valid name). Removing them breaks `interactive_add_type_parent_only_from_existing`, `scratch_parent_child_rule_defined_types_and_severity`, etc. Real usage double-checks (select re-asks, outer loop re-checks) — harmless.
- **Static-list callsites (1,2,3,6,8) have no outer loop today** — verbatim text is parsed later by `parse_*` which bails. Migration = real `select` now rejects out-of-list up front (friendlier, earlier). Scripted tests feed blanks (-> default) or exact option strings (`filesystem`/`incremental`/`generated`/`error`/`scratch`) — all still accepted, no answer changes.
- **`multi_select` signature:** `fn multi_select(&mut self, label: &str, options: &[&str], defaults: &[&str]) -> Result<Vec<String>>`. When `options` is EMPTY (the lifecycle-states case — states are user-invented, not from a fixed set) it is freeform: split the one input line on `,`, trim, drop empties. When `options` is non-empty: accept comma-separated numbers and/or exact names (reject unknown in real impl). Blank input -> `defaults.to_vec()`.
- **Lifecycle-states migration:** replace the L359-370 blank-to-finish loop with a single `multi_select("Lifecycle states", &[], &[])?` call, KEEPING the "at least one state is required" re-ask guard at the callsite (re-ask if returned Vec is empty).

## Tasks

1. **Trait (`wizard.rs`).** Update `Prompter::select` doc to specify numbered-chooser + out-of-list rejection (real impl). Add `fn multi_select(&mut self, label: &str, options: &[&str], defaults: &[&str]) -> Result<Vec<String>>` with doc.
2. **`StdinPrompter` real impl (`wizard.rs`).** Rewrite `select` (L78-86): render numbered list (`1) opt` ... with `[default]` cue), read line, empty -> default, else resolve a 1-based number into `options` OR match exact option text; unknown -> print a rejection line and re-read (loop). Implement `multi_select`: if `options` empty, split line on `,` (trim, drop empties), blank -> `defaults`; else render numbered list, parse comma-separated tokens as numbers-or-names, reject any unknown token (re-read), blank -> `defaults`; return `Vec<String>` in input order, de-duplicated.
3. **`ScriptedPrompter` fake (`wizard.rs`).** Keep `select` verbatim (blank -> default). Add `multi_select`: pop one queued answer, blank -> `defaults.to_vec()`, else split on `,` (trim, drop empties) -> `Vec<String>`. Update the struct doc comment (L89-91) to describe `multi_select` grammar.
4. **Migrate lifecycle states (`config.rs`).** Replace `collect_type_interactive` L359-370 states loop with `multi_select("Lifecycle states", &[], &[])` + empty-guard re-ask. Leave edges loop untouched. (Static/dynamic `select` callsites need no code change beyond benefiting from the new real-impl validation; outer re-ask loops stay.)
5. **Update scripted test grammar (states only).** Change every script that fed states as `"state", "state", ""` (blank-to-finish) to a single comma-joined answer:
   - `config.rs::interactive_add_type_custom_lifecycle_reprompts_bad_edge`: `"draft","done",""` -> `"draft,done"` (edge lines unchanged).
   - `config.rs::interactive_full_flow_matches_flag_chain`: `"draft","done","", "draft:done",""` -> `"draft,done","draft:done",""`.
   - `init.rs::full_scratch_answers`: `"draft","accepted","","draft:accepted",""` -> `"draft,accepted","draft:accepted",""`.
   - `init.rs::scratch_lifecycle_and_gate_reject_unknown_states`: `"draft","done",""` -> `"draft,done"` (edge re-ask lines unchanged).
   Verify `design_all_defaults_equals_starter` / `interactive_add_type_matches_flag_call` still pass unchanged (blanks -> defaults; exact option strings still accepted).
6. **New tests (`wizard.rs` `#[cfg(test)]` mod — construct `StdinPrompter { reader: Cursor<..>, writer: Vec<u8> }` directly; fields are private but same-module).**
   - real `select`: input `"bogus\nfilesystem\n"` over options `[filesystem, git-ref]` -> returns `"filesystem"`; assert writer contains a re-prompt/rejection cue.
   - real `select`: input `"2\n"` -> returns second option (number resolves to list).
   - real `multi_select` (options non-empty): input `"1,3\n"` -> returns first+third; unknown token `"9\n"`/`"nope\n"` re-asks.
   - real `multi_select` (options empty): input `"draft, accepted\n"` -> `vec!["draft","accepted"]` (trimmed).
   - `ScriptedPrompter::multi_select`: queued `"a,b,c"` -> `vec!["a","b","c"]`; blank -> `defaults`.
7. **Migration integration test (`config.rs`).** Add a test that `collect_type_interactive` with a `multi_select` states answer `"draft,review,done"` yields a lifecycle with all three states (drives via `ScriptedPrompter`), and that a blank states answer re-asks (empty-guard) before accepting a valid one.
8. **README.** If the interactive add-type flow prose (~L537-544) narrates the prompt sequence, update the lifecycle-states step to "enter comma-separated states" and note single-choice prompts are now pick-from-list. No flag/CLI-surface change this slice, so the command tables (~L302, L329) are untouched.

## Out of scope

- Blank-by-default first screen + `--template starter` flag (STORY-229 "Blank-by-default" AC) -> Slice 2. `run_init_interactive`'s `select("Start from", ...)` keeps today's `starter`/`scratch` labels and `starter` default here.
- Wizard colours / `src/cli/style.rs` wiring (STORY-229 "Wizard colours" AC) -> Slice 3.
- Edges loop redesign (`config.rs:371-391`) — FROM:TO stays an `ask`+`parse_edge` loop.
- Non-interactive `run()` and `--json` paths (`init.rs:65-68`) — untouched (principle 2).
- Engine writers (`config_write`, `Config`) — untouched (principle 3).

## Principles / conventions

- **Principle 2 (parity):** non-interactive `run()` + `--json` byte-for-byte unchanged; scriptability preserved through `multi_select` (AC "remains fully scriptable via queued answers").
- **Principle 3 (layering):** every change under `src/cli/`; engine untouched.
- **Principle 4 (test seam):** `Prompter` trait is the only fake seam; validation added to `StdinPrompter` only, `ScriptedPrompter` stays verbatim; no other `Prompter` impls exist (only two, both in `wizard.rs`; `main.rs:38,674` construct `StdinPrompter::new()`).

## Verification

- `cargo test` green — all pre-existing `ScriptedPrompter`-driven tests in `config.rs` and `init.rs` pass after the states-grammar update; new wizard/config tests pass.
- Boundary: real `select` fed an out-of-list value then a valid one returns the valid one and emitted a rejection line (proves re-prompt, not silent-accept). Real `multi_select` fed `"1,3"` returns exactly two selections (proves multi-capture).
- `cargo run -- init` on a TTY (manual, if runnable): store/numbering/authorship/severity render numbered; lifecycle states accept one comma-separated line.

