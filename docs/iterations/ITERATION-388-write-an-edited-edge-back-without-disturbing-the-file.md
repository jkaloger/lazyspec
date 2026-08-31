---
title: Write an edited edge back without disturbing the file
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-260
- blocks: ITERATION-389
- blocks: ITERATION-390
- blocks: ITERATION-391
---

## Objective

A settings save carries edge edits into `.lazyspec.toml` through `write_edges`, and every unrelated section, comment and block ordering survives it.

## Satisfies

STORY-260 AC5. AC1, AC2 landed in ITERATION-386 and ITERATION-387; AC4 lands in ITERATION-389, AC6 in ITERATION-390, AC3 in ITERATION-391.

## Context

- Story + ACs: STORY-260
- Touch:
  - `src/engine/config_write.rs:10-27` `write_config_in_place` -- thirteen writers, none of them for edges. This slice adds the call
  - `src/engine/config_write.rs:631-656` `reconcile_array_of_tables` -- the by-`name` reconciliation `write_edges` uses
  - `src/tui/state/app.rs:1594-1637` `settings_commit_write` -- unchanged. It already renders in memory, validates the exact bytes with `Config::parse`, and touches disk only on success
  - `tests/integration/cli_fix_config_test.rs:378` `fix_config_migration_preserves_user_content` and `:447` `fix_config_preserves_user_defined_extras` -- ITERATION-377's preservation guards, which cover the same writer from the migration side
- **This slice does not write the writer.** `write_edges` is built by ITERATION-377 for `fix --config`. That iteration's §Out of scope names this story as its second consumer and asks that it stay "a general buffer-to-document writer, not a migration-shaped one". If it did not come out that way, fix it there rather than adding a second writer here -- two writers for one table is how the emitted spelling drifts, which is the bug ITERATION-368 Task 3 and ITERATION-377 Task 2 both already guard against for `to_toml`.
- **What preservation means here, as against the migration.** ADR-032 §Consequences accepts that a *translated* block loses its comments. An *edited* block is a different case: `reconcile_array_of_tables` edits tables in place, so a comment on an untouched key survives. Assert that explicitly, because the migration's accepted loss will otherwise be read as licence for the editor.
- **Reconciliation is by `name`, and nothing enforces that `name` is unique.** The strict-load edge loop (`src/engine/config.rs:1307-1345`) checks `via`'s presence, unknown types and unknown relationships, and never checks for duplicate names -- even though ADR-031 §Consequences keeps `name` required precisely so error messages can identify a row by it. Two rows sharing a name are a config the loader accepts and a by-name reconciliation cannot address. Do not paper over it in the writer; record where the missing check would go, and note that ITERATION-390's seed has to work around its absence.
- The save protocol is unchanged by this slice: what changes is what the rendered bytes contain, not when they are written or validated.

## Tasks

1. Test-first in `config_write.rs`: a source carrying a comment above `[[edges]]`, a comment on an untouched key inside an edge block, a `[github]` section after the edge blocks and an unrecognised top-level section round-trips an edited `to` with everything else byte-identical. Assert on the text; a reparse cannot tell you a comment was dropped.
2. Add `write_edges` to `write_config_in_place`'s call list, positioned so an appended block lands where a human would have put it rather than at the end of the file.
3. Test the insert case: a config with no `[[edges]]` at all gains the block without disturbing what precedes it, and the result strict-loads.
4. Test through the App: a drilled edge edit followed by `settings_save` writes the edit, clears `settings_dirty`, clears `settings_footer_error`, and raises `config_reload_request` (`app.rs:1633-1636`).
5. Test the no-op case: a save on a clean buffer whose config declares edges leaves the file byte-identical. Edges joining the writer set is the moment a previously untouched table starts being rewritten on every save, and that is where an unnoticed reformat would show up.
6. `README.md:480` claims the config mutators "reconcile the TOML in place, preserving comments, formatting, and block order exactly as `fix --config` and the TUI settings screen do". Confirm the claim holds with edges in the set and extend the sentence if it enumerates tables.

## Out of scope

- `write_edges` itself, and the migration that motivates it -> ITERATION-377, which must land first.
- Rejecting an edit the loader would refuse (AC4) -> ITERATION-389. Until then a save either writes a valid config or fails at `Config::parse` with the message pointed at nothing in particular; making that message land on the right field is the next slice.
- Enforcing unique edge names, wherever that lands. This slice states the hole and does not fill it: a load-time check is an engine change with no AC on this story.
- `write_rules` and `update_rule_table` -> ITERATION-385 deletes them from the same file. Neither slice needs the other; do not delete them here to tidy up.
- `[[edges]]` from the `config` CLI -> STORY-261, which reuses the same writer.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: the writer is engine, the save protocol is TUI, and this slice adds one line to the former. Dictum 6: one edge writer with two callers, not one per caller.

## Verification

On a scratch copy of this repo's `.lazyspec.toml` with a comment above each `[[edges]]` block, change one `to` in the TUI, save, and `git diff` shows exactly that one line. Then save again without editing: no further diff.
