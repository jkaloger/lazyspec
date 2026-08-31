---
title: Delete ValidationRule
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-259
- blocks: ITERATION-398
---

## Objective

`ValidationRule`, both its variants, `Config.rules`, `RawConfig.rules`, `default_rules()` and the config writers that serialise them are deleted, and neither the README nor the emitted JSON schema documents `[[rules]]` as current.

## Satisfies

STORY-259 AC3, AC5. AC3 is an absence claim, so it is satisfied by the slice in which the grep comes back empty; the four preceding iterations cleared disjoint surfaces (`hierarchy_from_config` and `Store.chain_relationships` in ITERATION-380, `require_parent_status` and `add-gate` in ITERATION-381, the TUI editor in ITERATION-382, `init` in ITERATION-383, `ParentLinkRule` in ITERATION-384) so that this one is the type and its serialisation. AC5 lands here because the schema is derived from the type by `schemars`: deleting `ValidationRule` is what removes the `$defs` entry, and the README section it documents goes in the same commit. AC1, AC2 landed in ITERATION-384; AC4 in ITERATION-383.

## Context

- Story + ACs: STORY-259
- Gone rather than deprecated, and no silent fallback in the load path: STORY-259 §Notes; ADR-011 §Consequences ("no hidden defaults to reconcile against")
- Both `ValidationRule` variants replaced by `EdgeDef`, `[[relationships]]` retaining only `name`/`inverse`/`github_native`: ADR-030 §Decision; RFC-067 §"Interface sketch" (`@ref src/engine/config.rs#ValidationRule`)
- Touch:
  - `src/engine/config.rs` -- the enum (`:27-46`), `Config.rules` (`:820-823`) and its `to_toml` serialisation comment, `RawConfig.rules` (`:986-988`), `let rules = ...` (`:1305`) and its use at `:1439`, `default_rules()` (`:1134-1159`), the `Default for Config` field (`:1181`), and the schema assertion's `ValidationRule` `oneOf` block (`:1696-1714`). The `EdgeDef` assertion immediately below it stays
  - `src/engine/config_write.rs` -- `write_rules` (`:553-564`), `rule_name` (`:566-571`), `update_rule_table` (`:573-629`), `rule_shape` (`:658-662`), and the tests at `:1029`, `:1075`, `:1093`, `:1329-1350`. `reconcile_array_of_tables` (`:631-656`) stays: `write_edges` (ITERATION-377) uses it
  - `src/cli/config.rs` -- `:460-464` (the `show` path's rule display), `parent_child_rule_name` (`:523-548`), and the `rule_named` test helper (`:986`) plus `show_json_emits_relationships_rules_and_gate` (`:1062`)
  - `src/engine/prompt.rs:6` -- the `ValidationRule` import, if ITERATION-373's `child_types_for` rewrite left it behind
  - `src/engine/context.rs:825` -- `config.rules.clear()` in a test fixture
  - `README.md` §"Validation rules" (`:624-650`) in full, and the §"Edges" paragraph's remaining comparison against `parent-child` (`:652`) -- once rules are gone, "where a `parent-child` rule is satisfied by any chain relationship" is a comparison to nothing. Also `:305` and `:469-476` if ITERATION-384 left any rules wording behind
- **The one thing that is not deletion.** `Config` derives `Serialize` and `to_toml` round-trips it, so `Config.rules` disappearing changes the shape of `config --json` and of every config the TUI and `fix` write. Nothing should notice -- ITERATION-384 made the field provably always empty under strict load -- but the `rules` key vanishing from `config --json` is a contract change dictum 2 makes visible. Assert the emitted config re-parses, and assert the schema no longer defines `ValidationRule`, rather than only that it no longer requires it.
- Deleting a `pub` engine item is the moment to check the API surface claim in convention §"API Surface". `default_rules()` is `pub` and reached from `src/cli/init.rs` and `src/engine/ops/fix/config.rs`; both callers were removed in ITERATION-383 and ITERATION-384, so confirm rather than assume.

## Tasks

1. Test-first on the schema: extend the existing assertion at `src/engine/config.rs:1690-1714` to require that `$defs` has no `ValidationRule` and that `properties` has no `rules`, replacing the two `shape` const assertions. This is AC5's machine-checkable half.
2. Delete the enum, `Config.rules`, `RawConfig.rules`, `default_rules()` and the `Default for Config` field. Compile; the remaining errors are the call-site inventory.
3. Delete `write_rules`, `update_rule_table`, `rule_shape` and `config_write.rs`'s `rule_name`, and their tests. Leave `reconcile_array_of_tables` alone.
4. Clear the `cli/config.rs` display and helper sites, and rewrite `show_json_emits_relationships_rules_and_gate` as a relationships-and-edges assertion -- the name no longer describes anything it can check.
5. Run the ast-grep AC3 asks for: `ast-grep -p 'ValidationRule' -l rust src/` returns nothing, and `grep -rn '\[\[rules\]\]' src/ tests/ README.md` returns nothing. Anything left is either a missed site or a deliberate historical mention in a doc, and the second kind does not live in those paths.
6. README: delete §"Validation rules" whole. Rewrite §"Edges"' opening sentence so it defines an edge on its own terms instead of by contrast with a rule shape that no longer exists, and confirm `fix --config`'s description and the migration section read correctly after ITERATION-384's edits.

## Out of scope

- `docs/` -- RFC-042, SPEC-002, SPEC-004, and the RFCs and stories that describe `[[rules]]` as it was. They are the record of decisions taken, and AC5 is scoped to the README and the schema. Do not rewrite history to make a grep clean.
- `AGENTS.md` and the shipped skills (`skills/lazy`, `skills/advance`, `skills/configure-type`), which tell agents to derive type boundaries from "the union of `parent_type` edges and parent-child `rules`" -> STORY-262. That instruction is wrong from ITERATION-384 onward and this story has no AC for it.
- `TypeDef.parent_type` and `Store.parent_of`. They mean containment and are explicitly untouched: ADR-030 §Decision.
- `RelationshipDef.traversal` -> STORY-257. `[[relationships]]` shrinking to `name`/`inverse`/`github_native` finishes there, not here.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 1: the codebase exists to produce and validate structured markdown, and after this slice there is exactly one config table describing the DAG. Convention §"API Surface": every `pub` item removed here narrows the engine contract, which is the point.

## Verification

`cargo run -- config --json | jq 'has("rules")'` is `false`, `cargo run -- validate` on this repo is clean, and `cargo run -- init` in an empty directory followed by `cargo run -- validate` in it is clean. `ast-grep -p 'ValidationRule' -l rust src/` prints nothing.
