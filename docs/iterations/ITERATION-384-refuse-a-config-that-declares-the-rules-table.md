---
title: Refuse a config that declares the rules table
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-259
- blocks: ITERATION-385
- blocks: ITERATION-389
- blocks: ITERATION-395
---

## Objective

Strict load fails on a config carrying `[[rules]]`, naming `fix --config` as the remedy; a config carrying only `[[edges]]` loads clean; `fix --config` no longer appends rules it would then refuse to read; and `ParentLinkRule`, whose declarations no longer arrive, goes with them.

## Satisfies

STORY-259 AC1, AC2. AC2 is the regression guard on AC1, not a separable slice -- the rejection is a predicate over one config shape and the clean load is the other half of the same test table. Also clears `ParentLinkRule` and the two findings it produced, part of AC3, closed in ITERATION-385. AC4 landed in ITERATION-383; AC5 lands in ITERATION-385.

## Context

- Story + ACs: STORY-259
- The strict-load pattern this follows -- hard error, no fallback, error names the remedy: ADR-011 §Decision. Deletion not deprecation: STORY-259 §Notes
- Why the lenient read must keep parsing `[[rules]]`, and why the remedy named is `fix --config` rather than `init`: ADR-012 §Decision, §Context
- The migration this rejection makes mandatory landed in ITERATION-376 and ITERATION-377, and was proved finding-preserving in ITERATION-379. This is the slice that depends on all three
- Touch:
  - `src/engine/config.rs:1273-1300` -- `parse_inner(toml_str, lenient)`. The bail belongs in the `!lenient` branch beside the `[[relationships]]` one at `:1284-1290`, so `Config::parse_lenient` (used only by `fix --config`, `:1465-1481`) keeps reading rules and can still translate them
  - `src/engine/config.rs:1305` -- `let rules = raw.rules.unwrap_or_default();`. Under strict load this becomes unreachable-with-content; the raw field itself stays until ITERATION-385
  - `src/engine/validation.rs:526-606` -- `ParentLinkRule`, its registration at `:1406`-adjacent, and `ValidationIssue::MissingParentLink` (`:16-21`) / `MissingRelation` (`:22-26`) with their `Display` arms (`:150-176`). Neither variant is matched outside `validation.rs`, so `Display` is the whole downstream contract -- re-confirm that before assuming it
  - `src/engine/validation.rs` tests: `:1846`, `:1855-1875`, `:2396-2425` (`rules_and_edges_both_report_from_the_same_config`, which exists to assert the coexistence this slice ends)
  - `src/engine/store.rs:41`, `:114-135` -- `Store.chain_relationships`, if ITERATION-380 left it alive for `:565`. This is its last reader
  - `src/engine/ops/fix/config.rs` -- `existing_rule_names` / `missing_rules` / `rules_added` (`:62-76`), the `nothing_missing` term (`:86`), `append_blocks`'s `rules` parameter (`:118-140`) and `RulesDoc` (`:21-24`). Appending `default_rules()` to a config that strict load then refuses is the collision ITERATION-377 §Context flagged and deferred; this is where it resolves
  - `src/engine/ops/fix.rs:64-69` -- `ConfigFixResult.rules_added` is a `--json` field. Removing it is a contract change (dictum 2); decide whether it goes or reports zero forever, and say which
  - Fixtures declaring `[[rules]]`: `tests/integration/cli_init_test.rs` (2), `cli_fix_config_test.rs` (1), `config_schema_validation_test.rs` (1), `config_test.rs` (4), `cli_expanded_validate_test.rs`, `cli_transition_gate_test.rs`, plus the inline TOML in `src/cli/config.rs` (3), `src/engine/config_write.rs` (2), `src/engine/config.rs` (4), `src/engine/validation.rs` (5)
  - `README.md:305`, `:440`, `:469-476`, `:694` -- `fix --config`'s description ("inject missing standard relationships/rules"), the sources-of-truth sentence, the migration section, and §"Edges"' closing claim that the two tables "are enforced independently; a project may declare either or both"
- **What the message has to say.** ADR-011's precedent is one sentence naming the remedy. `[[rules]]` differs from a *missing* section in one way that matters: the user's config is not incomplete, it is obsolete, and running `fix --config` rewrites it destructively (ADR-032 §Consequences). Name `fix --config --dry-run` alongside `fix --config`, since the plan ITERATION-378 built is the thing that tells them what they lose.
- The `[[rules]]`-carrying fixtures are not busywork: each one is a test that will pass for the wrong reason if the block is simply deleted. Every fixture that declared a rule declared it because the assertion needed a constraint; translate it to the equivalent edge and keep the assertion, or delete the test and say why.

## Tasks

1. Test-first, the pair: a config with one `[[rules]]` block fails `Config::parse` with a message carrying both `[[rules]]` and `fix --config`; the same config with `[[edges]]` in its place parses and its `edges` are populated (AC1, AC2). Add a third case asserting `Config::parse_lenient` still accepts the rules-carrying config -- that is what keeps `fix --config` able to repair it.
2. Add the bail to `parse_inner`'s strict branch, alongside the `[[relationships]]` bail it mirrors.
3. Remove the rules-append path from `fix --config`: `missing_rules`, `rules_added`, the `append_blocks` parameter, and `RulesDoc`. Resolve `ConfigFixResult.rules_added` explicitly and amend the `collect_config_fixes` doc comment, which ITERATION-377 already rewrote once (convention §Governance).
4. Delete `ParentLinkRule` and the `MissingParentLink` / `MissingRelation` variants and their `Display` arms. `rules_and_edges_both_report_from_the_same_config` asserts a coexistence that no longer exists -- delete it and let the new rejection test stand in its place.
5. Delete `Store.chain_relationships` and the `load_with_fs` filter that builds it, if ITERATION-380 left it for `:565`.
6. Translate every `[[rules]]`-carrying fixture to `[[edges]]` and keep its assertion. Work file by file; a fixture whose test no longer has anything to assert gets deleted with a note in the commit, not silently.
7. README: `fix --config` migrates rules to edges rather than injecting rules; the sources-of-truth sentence names `[[edges]]`; the migration section states that a config declaring `[[rules]]` is rejected on every command; §"Edges" loses the "either or both" sentence.

## Out of scope

- `ValidationRule`, `Config.rules`, `RawConfig.rules`, `default_rules()`, `write_rules` and the JSON schema `$defs` -> ITERATION-385. After this slice `config.rules` is provably always empty under strict load, which is what makes that deletion mechanical.
- The migration itself -- translation, rewrite, plan, proof -> STORY-258. This slice runs it, it does not build it.
- `require_parent_status` and `config add-gate` -> ITERATION-381, which must land first: a command that writes a field into a table strict load now refuses would be a command that breaks the project it repairs.
- The TUI settings panel and `init` -> ITERATION-382, ITERATION-383, both of which must land first, for the same reason.
- `AGENTS.md` and the shipped skills, which instruct agents to read `rules` and `require_parent_status` from `config --json` -> STORY-262. Nothing in STORY-259's ACs covers them; after this slice that instruction names an empty list.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: the rejection message and `fix --config --json`'s result shape are both agent-facing. Dictum 5: this is the same strict-load idiom `[[relationships]]` already uses; do not invent a second error style for it.

## Verification

`cargo run -- validate` on this repo works (its config moved to `[[edges]]` in ITERATION-380), and on a scratch copy with the three `[[rules]]` blocks restored, every command fails with the same message naming `fix --config`. `cargo run -- fix --config --dry-run` on that scratch copy still reads it and reports the migration.
