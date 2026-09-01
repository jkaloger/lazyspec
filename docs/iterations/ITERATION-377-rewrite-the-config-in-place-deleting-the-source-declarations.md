---
title: Rewrite the config in place, deleting the source declarations
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-258
- blocks: ITERATION-378
- blocks: ITERATION-379
- blocks: ITERATION-380
- blocks: ITERATION-388
- blocks: ITERATION-392
---

## Objective

`fix --config` applies the translation: `[[edges]]` is written into `.lazyspec.toml`, the source `[[rules]]` blocks and `relationships.traversal` keys are removed, every section the migration does not understand survives, and a config already carrying `[[edges]]` and no `[[rules]]` is left untouched.

## Satisfies

STORY-258 AC1, AC6, AC8. AC6 and AC8 are properties of the one rewrite path AC1 introduces -- an idempotence or preservation slice with no rewrite to be idempotent about is not a slice. AC2, AC3, AC4 landed in the preceding iteration; AC5, AC7 deferred.

## Context

- Story + ACs: STORY-258
- Why this cannot be append-only, and that byte-for-byte preservation is lost for the replaced blocks: ADR-032 §Context, §Consequences; STORY-258 §Notes
- ADR-012's lenient read is already in place at `src/engine/ops/fix/config.rs:49-50`; this slice adds no new load path
- Touch:
  - `src/engine/config_write.rs` -- there is no `write_edges`. Add one beside `write_rules` (`:553-564`), reconciling by `name` through the existing `reconcile_array_of_tables` (`:631-656`). Two traps: deleting all `[[rules]]` is not "reconcile against an empty buffer", because `retain` leaves an empty array-of-tables behind, so the `rules` key must come off the `DocumentMut` outright; and `update_relationship_table` (`:282-286`) writes `name`/`inverse`/`github_native` and never mentions `traversal`, so a source `traversal` key survives in-place editing today and needs an explicit removal
  - `src/engine/ops/fix/config.rs` -- `collect_config_fixes` calls the rewrite. Its doc comment at `:34-41` asserts append-only "by design", which is now false for the module (convention §Governance; STORY-258 §Notes)
  - `src/engine/ops/fix.rs:64-69` -- `ConfigFixResult` has three `*_added` vectors and nothing that can say "migrated", so `fix --config` would rewrite the file while reporting nothing
  - `tests/integration/cli_fix_config_test.rs` -- `fix_config_migration_preserves_user_content` (`:378`) and `fix_config_preserves_user_defined_extras` (`:447`) are AC8's existing guards; extend them rather than write a third fixture
- The three append-only fixes keep working until STORY-259, and one of them now collides with the rewrite: on a config with no `[[rules]]`, `collect_config_fixes` appends the standard rules from `default_rules()` (`:63-66`). Appending them and translating them away in the same run is a wasted round trip and an incoherent plan. Pick an order, and record which one in the doc comment.

## Tasks

1. Test-first, integration: a config with two `[[rules]]` and `traversal` on two relationships comes back with `[[edges]]`, no `rules` key, and no `traversal` on any relationship -- and passes strict load afterwards, following `fix_config_result_passes_strict_load` (`:198`).
2. Add `write_edges` plus an edge table writer covering all six keys. It must emit the spelling a human would write: `"*"` re-emitted as `["*"]` is the bug ITERATION-368 Task 3 guards against in `to_toml`, and the in-place writer is a second code path that needs its own assertion.
3. Remove the source: drop the `rules` key from the document, and remove `traversal` from each relationship table. Both are deletions of things the writer otherwise preserves, so both need a test that reads the resulting text, not just the reparsed `Config`.
4. Wire the rewrite into `collect_config_fixes`, resolve the `default_rules()` interaction from Context, and give `ConfigFixResult` enough shape that the next slice has something to format -- at minimum the edges written and the rules and traversal keys removed.
5. Test-first idempotence (AC6): a second run over a migrated config leaves the bytes identical and `written` false. Cover a hand-written edges-only config as well -- the AC is about a config this tool has never touched.
6. Test-first preservation (AC8): `[github]`, `[tui]`, an unrecognised top-level section, and comments not attached to a translated block all survive. Assert on the file text; a reparse cannot tell you a comment was dropped.
7. Amend the `collect_config_fixes` doc comment: append-only for relationships, rules and lifecycles; translating rewrite for the edge migration; and what is not preserved byte-for-byte.

## Out of scope

- The plan naming what the rewrite destroys (AC7) -> next iteration. Until then the rewrite drops comments on translated blocks with no warning. Do not soften that by preserving them: ADR-032 §Consequences accepts the loss, and preserving them would make the warning unnecessary and the ADR wrong.
- The finding-set proof (AC5) -> the last iteration on this story.
- Editing edges from the `config` CLI, `init` or the TUI settings panel (STORY-260, STORY-261). `write_edges` is the writer those stories reuse, so keep it a general buffer-to-document writer, not a migration-shaped one.
- Retiring `[[rules]]` and `RelationshipDef.traversal`, and migrating this repo's own config -> STORY-259.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: `write_edges` follows `write_rules`' shape and reuses `reconcile_array_of_tables`; a second reconciliation helper is the wrong answer. Dictum 2: the result type is what `fix --config --json` serialises, so it carries facts, not sentences.

## Verification

On a scratch copy of this repo's `.lazyspec.toml`, apply the migration and confirm the result strict-loads and that `[[edges]]` names `stories-need-rfcs`, `iterations-need-stories` and `adrs-need-relations`. This repo's own config stays on `[[rules]]` -- `git diff .lazyspec.toml` must be empty at the end of the slice.
