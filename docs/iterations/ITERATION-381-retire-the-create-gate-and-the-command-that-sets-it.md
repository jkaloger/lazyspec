---
title: Retire the create gate and the command that sets it
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-259
- blocks: ITERATION-384
- blocks: ITERATION-392
- blocks: ITERATION-402
---

## Objective

`require_parent_status` comes off `ValidationRule::ParentChild`, `create` stops consulting a parent's status, and `config add-gate` -- the command whose only purpose is to set that field -- is removed rather than repointed.

## Satisfies

STORY-259 AC3, in part: the `require_parent_status` surface -- the field, the `create` gate at `src/engine/ops/create.rs:86-99`, and the three `ValidationRule` sites in `src/cli/config.rs` that exist to write it. This is the largest *behaviour* removal on the story and the only one with no successor, so it is separated from the mechanical deletion in ITERATION-385. AC1, AC2 land in ITERATION-384; AC4 in ITERATION-383; AC5 in ITERATION-385.

## Context

- Story + ACs: STORY-259
- That gating is abandoned outright rather than moved onto the edge table, and that `require_parent_status` dies here with no successor: ADR-033; RFC-067 §Design, final bullet ("No edge condition refuses a command"); ADR-030 §"Amended 2026-08-31"
- Commit `40b91f3` already reverted the `require_to_status` successor. This slice removes the predecessor, so the abandonment is complete in both directions
- Touch:
  - `src/engine/config.rs:37` -- the `require_parent_status: Option<String>` field, and its two occurrences in `default_rules()` (`:1143`, `:1150`)
  - `src/engine/ops/create.rs:86-99` -- the gate loop, including its `validate_status` call against the parent type's lifecycle. `ValidationRule` is imported at `:2` solely for this
  - `src/cli/config.rs` -- the `AddGate` clap variant (`:90`), its dispatch (`:724`), `run_add_gate` (`:756-781`), the `rule_name` helper it needs (`:783-788`), and the wizard's gate collection (`:589-604`, applied at `:764-775`, built at `:594`)
  - `src/cli/init.rs:294-310` -- `render_dag_summary` prints ", gate: parent status = X" per rule
  - `tests/integration/cli_transition_gate_test.rs` -- `config_with_parent_status_gate` (`:126`), `parent_status_gate_blocks_then_allows` (`:165`) and `no_gate_when_require_parent_status_unset` (`:215`). The other five tests in the file are lifecycle-edge transition checks and stay
  - `src/cli/config.rs` tests: `add_gate_sets_require_parent_status` (`:1541`), `add_gate_rejects_unknown_rule` (`:1552`), `add_gate_rejects_relation_existence_rule` (`:1559`), `add_gate_preserves_comments_and_only_changes_one_rule` (`:1638`), `show_json_emits_relationships_rules_and_gate` (`:1062`), and the composed flow at `:2074`/`:2136`
  - `README.md` -- the `config add-gate` row (`:315`), the gate paragraph in §"Validation rules" (`:631`), the `add-gate` example and its constraint sentence (`:504-508`), and `config.rs:3011-3075`'s two parse tests for the field
- This is a removal a user can notice: a project relying on `require_parent_status = "accepted"` loses a refusal at `create`. ADR-033 is the authority for accepting that; ITERATION-378 is where the migration plan says so out loud. Do not soften it with a warning-severity validation finding as a consolation -- RFC-067 forbids the edge table carrying a second policy, and inventing one outside the edge table is worse.

## Tasks

1. Test-first, by deletion: `parent_status_gate_blocks_then_allows` is the assertion that the gate exists. Invert it -- `create` of a child type succeeds with the parent in its earliest state and no gate declarable -- so the suite records that gating is gone rather than merely losing a test.
2. Remove the gate loop from `ops/create.rs` and drop its now-unused `ValidationRule` / `validate_status` imports. Check whether `validate_status` has other callers before deleting it.
3. Remove the field from `ValidationRule::ParentChild` and from `default_rules()`. Every remaining `..` pattern over the variant keeps compiling; the two parse tests in `config.rs` do not, and go.
4. Remove `config add-gate` end to end: the clap variant, the dispatch arm, `run_add_gate`, and its tests. Removing a subcommand is a CLI contract change -- state it in the README table in the same commit, per the project instruction that CLI changes update the README.
5. Remove the wizard's gate prompt from `src/cli/config.rs`, and the gate suffix from `render_dag_summary`. The from-scratch wizard scripts in `init.rs`'s test module feed a positional `"n", // no gate` answer (e.g. inside `scratch_parent_child_rule_defined_types_and_severity`, `:838-870`); every scripted answer list shifts by one, so run the whole `init` test module, not only the tests that mention gates.
6. README: drop the `add-gate` row, the gate paragraph, and the gate example. Leave §"Validation rules" otherwise intact -- ITERATION-385 removes the section.

## Out of scope

- `ValidationRule` itself, `Config.rules`, and the rest of §"Validation rules" in the README -> ITERATION-385.
- Refusing `[[rules]]` at load -> ITERATION-384. Until then a `require_parent_status` key in a config is an unknown field on the rule shape; confirm what serde does with it and, if it is silently ignored, say so in ITERATION-384's rejection message rather than adding a second bail here.
- The migration plan naming the gate as something the rewrite destroys -> ITERATION-378 (STORY-258).
- Status-conditioned gating in any other form. It is abandoned, not relocated (ADR-033).

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 1: a command that exists only to write a field that no longer exists is not justified by anything. Dictum 2: `config --json`'s `rules` entries lose a key, so the schema assertion moves with them.

## Verification

`cargo run -- config add-gate --help` fails as an unknown subcommand, and `cargo run -- create story "x"` on a scratch project whose rfc is still `draft` succeeds. On this repo, `git diff .lazyspec.toml` is empty -- it declares no gate.
