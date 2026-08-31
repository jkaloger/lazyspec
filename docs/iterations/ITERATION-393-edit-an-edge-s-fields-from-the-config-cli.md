---
title: Edit an edge's fields from the config CLI
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-261
- blocks: ITERATION-394
---

## Objective

`config set-edge <name>` changes any field on an existing row -- `from`, the whole `to` set, `via`, `traversal`, `required` -- and every section, comment and block ordering the row does not own survives it.

## Satisfies

STORY-261 AC2. AC1, AC4 landed in ITERATION-392; AC3 lands in ITERATION-394, AC5 in ITERATION-395, AC6 in ITERATION-396, AC7 in ITERATION-397.

## Context

- Story + ACs: STORY-261
- `required` on a set means "any one member", so replacing the set does not multiply the constraint: RFC-067 §Design
- Touch:
  - `src/cli/config.rs:729-754` `run_set_lifecycle` -- the naming and semantics precedent: it *replaces* the whole lifecycle rather than merging into it, and `README.md:508` states that contract out loud ("`set-lifecycle` replaces the whole lifecycle (it is a set, not a merge)")
  - `src/engine/config_write.rs:631-656` `reconcile_array_of_tables` -- edits a surviving source table in place, which is what makes a comment on an untouched key survive an edit
  - `src/engine/config_write.rs:573-630` `update_rule_table` -- the shape `write_edges`' table updater follows, including the clear-then-set discipline when a key stops applying
  - `src/engine/config.rs:55-65` `EdgeDef`, and `:63-64`'s `skip_serializing_if` on `required`
  - `README.md:508` (the mutator-constraints sentence), `:650-711` §"Edges"
- **The decision this slice has to make: partial edit or whole-row replace.** `set-lifecycle` replaces. An edge has six fields and AC2 says "any field", so a replace spelling makes changing `required` alone require re-passing `from`, `to` and `via` -- and getting one of them wrong silently rewrites the DAG. Take the partial spelling: an omitted flag means "leave it". Then unsetting an optional needs a spelling of its own, because omitting `--required` cannot mean both "leave it" and "remove it". Add explicit `--no-required` / `--no-traversal` rather than overloading an empty string, and say so in the help text. This diverges from the command beside it, so the divergence and its reason belong in `run_set_edge`'s doc comment (convention §Governance).
- **The target set replaces; it does not accumulate.** Repeated `--to` on `set-edge` must mean the new set, not additions to the old one, or there is no way to shrink a set from the CLI. The TUI's answer to the same question is a picker that adds and removes members individually (ITERATION-391); the CLI's answer is replacement. Two surfaces, two affordances, one resulting shape -- that is fine, but only if `--to` is documented as replacement in both `--help` and the README.
- Renaming a row is a rename in `reconcile_array_of_tables`' terms -- "remove-old + append-new" (`config_write.rs:545-550`) -- so a `--name` edit loses the block's comments and moves it to the end of the array. Either refuse `--name` here and say why, or accept the decor loss and state it in the help text. Do not let it happen silently: ADR-032 §Consequences accepts decor loss for a *translated* block and ITERATION-388 §Context is explicit that an *edited* block is a different case.
- An unknown edge name is a plain `bail!`, matching `run_set_lifecycle`'s "unknown type" (`:747`) and `run_add_type`'s duplicate refusal (`:143`). This is a CLI-argument error, not a config-validity error, so it is not what AC5 is about.
- `EdgeDef.traversal` arrives in ITERATION-372; `--traversal` / `--no-traversal` have nothing to set until then.

## Tasks

1. Test-first, against the decorated fixture: setting `required` alone on a row leaves `from`, `to`, `via` and every comment in the file untouched; assert on the file text, since a reparse cannot tell you a comment was dropped.
2. Add the `SetEdge` variant, its dispatch arm and `run_set_edge` with the partial-edit semantics and the two explicit unset flags, reusing ITERATION-392's `TypeSelector` constructor for `--from` / `--to`.
3. Test the set replacement in both directions: `["spike","story","bug"]` down to `["story"]` and back up, asserting the emitted spelling each way -- a one-element set re-emits as a bare name (`config.rs:166-179`), so shrinking a set changes the TOML's shape as well as its content.
4. Test unset: `--no-required` removes the key from the file rather than writing a default, which `required`'s `skip_serializing_if` is what makes observable.
5. Resolve the `--name` question from Context and implement whichever answer, with the reason in `run_set_edge`'s doc comment.
6. Emit ITERATION-392's envelope with `"action": "edge-updated"` and the row after the edit, and test that it agrees with `config --json`.
7. README: the `set-edge` row in the command table, and the mutator-constraints sentence at `:508` extended -- `add-type` rejects duplicates, `set-lifecycle` replaces, `set-edge` merges except for `--to`.

## Out of scope

- Removing a row (AC3) -> ITERATION-394.
- Rejecting an edit the loader would refuse (AC5) -> ITERATION-395. This slice will happily set `--via nonsense`; the config then fails to load on the next command with a message pointed at the config rather than at the flag.
- The two holes ITERATION-388 and ITERATION-390 recorded -- no duplicate-`name` check and no non-empty-`to` check at load. `set-edge` can produce an empty target set if `--to` is given zero values; clap makes that unreachable for a `Vec<String>` with `num_args` unset, so confirm it rather than guarding it, and if it *is* reachable, refuse it with the same message the panel uses (ITERATION-387 §Context).
- Editing `[[types]]`, `[[relationships]]` or `[[rules]]` from the CLI. Only `add-type` and `set-lifecycle` exist and this story adds no more; the asymmetry is pre-existing.
- The TUI settings panel -> STORY-260, whose ITERATION-387 covers the same six fields through a different surface.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: reuse `reconcile_array_of_tables` and ITERATION-392's selector constructor; a second table updater for one table is the wrong answer. Convention §"CLI Patterns": the flag set is the contract, so the divergence from `set-lifecycle`'s replace semantics is documented, not inferred.

## Verification

On a scratch copy of this repo's config with a comment above and inside one `[[edges]]` block: `lazyspec config set-edge <name> --required warning` and `git diff .lazyspec.toml` shows exactly one changed line, both comments intact. Then `--no-required` and the key is gone, not defaulted.
