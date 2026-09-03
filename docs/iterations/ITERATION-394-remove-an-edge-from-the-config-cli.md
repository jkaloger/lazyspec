---
title: Remove an edge from the config CLI
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-261
- blocks: ITERATION-395
---

## Objective

`config remove-edge <name>` drops one `[[edges]]` row, leaves every other block and its decor byte-identical, and removes the `edges` key outright when the last row goes.

## Satisfies

STORY-261 AC3. AC1, AC4 landed in ITERATION-392, AC2 in ITERATION-393; AC5 lands in ITERATION-395, AC6 in ITERATION-396, AC7 in ITERATION-397.

## Context

- Story + ACs: STORY-261
- Touch:
  - `src/engine/config_write.rs:631-656` `reconcile_array_of_tables` -- deletion is expressed by the buffer no longer carrying the name: `tables.retain` drops it and, per the doc comment at `:545-550`, "a middle delete drops only that one table, others keep their comments". That is the whole mechanism; this slice adds no writer code
  - `src/engine/config_write.rs:553-564` `write_rules` -- shows the failure mode to avoid: it returns early when the key is absent *and* the buffer is empty, but on a buffer emptied to zero it reconciles against an empty slice and leaves `[[rules]]` behind as an empty array-of-tables. ITERATION-377 §Context names the same trap for the migration and takes the key off the `DocumentMut` outright; `write_edges` must behave the same way, and this is the slice where an emptied table is reachable from a user command
  - `src/cli/config.rs:99-110` `run_show_json` -- forces `edges` to an always-present array in JSON, so removing the last row must leave `config --json` reporting `[]`, not a missing key. The TOML loses the table and the JSON keeps the field; assert both
  - `README.md:311-315`, `:508`
- **A config with zero edges is legal.** Strict load has no minimum (`src/engine/config.rs:1307-1345` iterates whatever is there) and ITERATION-390 §Context says the same for the panel's delete path: unlike `[[relationships]]`, where ADR-011's refusal to delete the last entry lives at `app.rs:2812-2815`, there is no analogue here. A zero-edge project validates clean and constrains nothing. Do not invent a refusal.
- **Deleting a row can turn valid documents into findings, or stop turning them into findings.** Removing a `required` row silences findings; removing a `traversal` row shortens every chain that walked it. Neither is a refusal (RFC-067 §Design: "No edge condition refuses a command"), and neither is reported today. Say what the command does in the JSON result -- the removed row, in full -- so an agent that wants to warn has the facts, and do not add a confirmation prompt: this is the CLI, and `set-lifecycle` already replaces a whole lifecycle without asking.
- Reconciliation is by `name` and nothing enforces that `name` is unique (`config.rs:1307-1345`; recorded in ITERATION-388 §Context). Two rows sharing a name means `remove-edge` deletes both or neither depending on how the buffer filter is written. Pick "remove every row with that name", state it in the doc comment, and test it -- guessing is worse than either answer.

## Tasks

1. Test-first, against the decorated fixture: removing the middle of three rows leaves the other two, their comments, and every other section byte-identical.
2. Add the `RemoveEdge` variant, its dispatch arm and `run_remove_edge`: read, parse, drop the row from the typed buffer, `write_config_in_place`, write. An unknown name is a `bail!` in `run_set_lifecycle`'s idiom (`:747`).
3. Test the last-row case: the `edges` key is absent from the resulting text, the result strict-loads, and `config --json`'s `edges` is `[]`. If `write_edges` leaves an empty array-of-tables behind, fix it there rather than special-casing the CLI -- ITERATION-377 owns that writer and the migration hits the same case.
4. Emit ITERATION-392's envelope with `"action": "edge-removed"` and the row as it was before removal.
5. Test the duplicate-name case from Context, asserting whichever answer the doc comment states.
6. Round-trip test: `add-edge` then `remove-edge` on a config that had no `[[edges]]` returns the file to byte-identical. This is the only assertion in the story that catches the writer inserting or leaving whitespace where a human would not have.
7. README: the `remove-edge` row in the command table and its line in the mutators block.

## Out of scope

- Rejecting a mutation the loader would refuse (AC5) -> ITERATION-395. A removal cannot produce an invalid config, so this slice is the one command on the story with nothing for that guard to catch -- confirm that when the guard lands rather than assuming it.
- Warning that a removal changes the finding set or shortens a chain. That is a `validate` / `context` observation and neither has an AC on this story; the JSON result carries the facts and stops there.
- Enforcing unique edge names at load. Third slice to work around its absence (after ITERATION-388 and ITERATION-390); still no AC anywhere.
- Removing a `[[relationship]]` or a `[[type]]` from the CLI. Neither has ever been possible and this story does not change that.
- The panel's delete path -> ITERATION-390.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: deletion is `reconcile_array_of_tables`' existing behaviour, so this slice contributes a caller and no new mechanism. Dictum 2: the removed row is in the JSON result because an agent cannot re-read what is gone.

## Verification

On a scratch copy of this repo's config: `lazyspec config remove-edge general-relatedness --json` prints the row it removed, `lazyspec validate` is clean, and `git diff` shows only that block gone. Remove the remaining rows and `grep -c '\[\[edges\]\]' .lazyspec.toml` is 0 while `lazyspec config --json | jq '.edges'` is `[]`.
