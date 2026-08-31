---
title: Add an edge from the config CLI
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-261
- blocks: ITERATION-393
- blocks: ITERATION-397
---

## Objective

`config add-edge` appends an `[[edges]]` row from flags, writing through `write_edges`, and answers `--json` with a result object -- the envelope the other two edge mutators then reuse.

## Satisfies

STORY-261 AC1, AC4. AC4 rides with AC1 because no `config` mutator emits a JSON result today (see Context): the envelope has to be designed with the first command, and `set-edge` / `remove-edge` fill it rather than inventing their own. AC2 lands in ITERATION-393, AC3 in ITERATION-394, AC5 in ITERATION-395, AC6 in ITERATION-396, AC7 in ITERATION-397.

## Context

- Story + ACs: STORY-261
- What each position on a row means, and that a `to` set is one row rather than one row per member: ADR-030 §Decision; RFC-067 §"Interface sketch"
- `"*"` on any of the three positions: ADR-031 §Decision
- Touch:
  - `src/cli/config.rs:16-97` -- `ConfigCommand`. `AddGate` (`:90-96`) is gone by ITERATION-381, so this slice adds a variant to a four-variant enum, not a five
  - `src/cli/config.rs:99-110` `run_show_json` -- its doc comment already promises `edges` is an always-present array in JSON so an agent "should never have to branch on null". That is the read half of AC1; this slice supplies the write half
  - `src/cli/config.rs:729-754` `run_set_lifecycle` -- the shape every flag-driven mutator follows: read, `Config::parse`, mutate the typed buffer, `write_config_in_place`, write. Follow it
  - `src/engine/config_write.rs:10-27` `write_config_in_place` -- `write_edges` joins its call list in ITERATION-377
  - `src/engine/config.rs:55-65` `EdgeDef`; `:98-121` `TypeSelector` with `names()`; `:124-135` `RelSelector`; `:181-192` and `:206-...` the two `Deserialize` impls that already decide how a scalar, a list and `"*"` map onto a selector
  - `src/main.rs:654-778` -- the `Commands::Config` dispatch
  - `README.md:311-315` (the `config` command table), `:483-513` (the examples and the mutator-constraints paragraph), `:650-711` §"Edges"
- **No `config` mutator answers `--json` at all.** `json` is a flag on `Commands::Config` (`src/main.rs:654`) and the three mutator arms pass it only to `spinner::op_spinner` to suppress the animation (`:753`, `:768`); none of them prints anything machine-readable, so dictum 2 is currently satisfied for `config show` and quietly unmet for `add-type` and `set-lifecycle`. The precedent to copy is `link --json`'s object at `src/main.rs:365-372` (`{"action": "linked", ...}`). Emit `{"action": "edge-added", "name": ..., "edge": <the row as `config --json` serialises it>}` so the caller does not have to re-read the config to see what landed. Retrofitting `add-type` and `set-lifecycle` is out of scope and recorded below.
- **The word "edge" is already taken in this command.** `config set-lifecycle --edge FROM:TO` (`src/cli/config.rs:85-87`, parsed by `parse_edge` at `:790-801`) means a lifecycle *transition*. `config add-edge` means a DAG edge. Two unrelated meanings, one word, adjacent subcommands -- the same collision ITERATION-383 hit inside `render_dag_summary`. Do not rename `set-lifecycle --edge`; disambiguate in the help text of both, and make `add-edge`'s help name the table it writes (`[[edges]]`).
- **The CLI is the third producer of a `TypeSelector`, and the first two disagree with what a naive parser would do.** `Deserialize` (`config.rs:181-192`) folds a scalar `"*"` to `Any` and a one-element list to `Types(vec![name])`, and `Serialize` (`:166-179`) folds a one-element `Types` back to a bare name. A repeated `--to` flag arrives as a `Vec<String>`, so `--to '*'` must become `Any` and `--to a --to '*'` must be refused rather than becoming `Types(vec!["a","*"])` -- the shape ITERATION-391 §Context establishes only the panel could produce. Put that mapping in one engine-side constructor on `TypeSelector` and have the CLI call it, so there is one rule and not a third copy of it.
- Flag spelling follows the two mutators beside it: repeated `--to` (as `set-lifecycle` repeats `--state`/`--edge` and `add-type` repeats `--attribute`), not a comma list. `--traversal` and `--required` stay optional and absent means absent -- `required`'s `skip_serializing_if` (`config.rs:63-64`) is what makes the difference visible in the written file.
- `EdgeDef.traversal` arrives in ITERATION-372; until then there is no field for `--traversal` to set. That is the blocking edge.

## Tasks

1. Test-first in `src/cli/config.rs`'s test module, against a fixture config with comments and a non-default section order (`:917-...`): `run_add_edge` on a config with no `[[edges]]` writes one row with every flag reflected, and `Config::parse` on the result yields the row; a second call with a different name appends beside it rather than replacing it.
2. Add the `AddEdge` variant, its dispatch arm, and `run_add_edge` following `run_set_lifecycle`'s read-parse-mutate-write shape.
3. Add the single `TypeSelector` constructor from Context -- one `Vec<String>` in, `Any` / `Types` / an error out -- and test the three cases (`["*"]`, `["story"]`, `["story","*"]`) at the engine level, where the other two selector rules already live.
4. Refuse a duplicate `name` up front, the way `run_add_type` refuses a duplicate type (`:142-144`). Nothing at load rejects two rows sharing a name (`src/engine/config.rs:1307-1345`) and `write_edges` reconciles by `name`, so a second `add-edge` with the same name would silently rewrite the first. ITERATION-388 recorded the missing load check; this is the second surface that has to work around it.
5. Add the JSON envelope from Context, and a test asserting the printed object parses and its `edge` matches the same row in `config --json`. Two spellings of one row is how the CLI's answer and the config's answer drift.
6. README: an `add-edge` row in the command table, an example in the mutators block, and the `[[edges]]` schema section gaining the mutator sentence. Full coverage of the surface is AC7's job in ITERATION-397; this slice documents only the command it adds.

## Out of scope

- Editing an existing row (AC2) -> ITERATION-393; removing one (AC3) -> ITERATION-394. Until then the only way to change a row from the CLI is to hand-edit the TOML, which is the thing this story exists to stop.
- Validating the mutation (AC5) -> ITERATION-395. This slice writes whatever the flags say: `--to nonsense` produces a config that strict load then rejects on every subsequent command. That is a real footgun for three slices and it is chosen, because the guard belongs in one place across all three commands rather than being grown per command.
- `write_edges` itself -> ITERATION-377, which must land first. Do not write a second edge writer here; the whole point of that iteration's §Out of scope note is that this story reuses it.
- `--json` for `config add-type` and `config set-lifecycle`. Both are silent today and stay silent; AC4 covers "any of these commands", meaning the edge mutators. Recorded because a reviewer reading dictum 2 will ask.
- The `init` starter set (AC6) and the wizard -> ITERATION-396, ITERATION-397.
- Editing edges in the TUI settings panel -> STORY-260.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: the result object is the agent's answer, so it carries the row, not a sentence about the row. Dictum 3: the selector constructor is engine, the flag parsing is CLI. Dictum 6: one selector rule with three callers, not one per caller. The project instruction that a CLI change updates the README applies to the new subcommand.

## Verification

On a scratch copy of this repo's config: `lazyspec config add-edge iterations-implement-work --from iteration --to story --to bug --via implements --required error --json` prints the row, `git diff .lazyspec.toml` shows exactly one added block with `to = ["story", "bug"]`, and `lazyspec config --json | jq '.edges[-1]'` matches what the command printed. Then `--to '*'` on a second row writes `to = "*"`, not `to = ["*"]`.
