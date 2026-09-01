---
title: Translate rules and relationship traversal into edge rows
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-258
- blocks: ITERATION-377
---

## Objective

`fix --config` gains a total, pure translation from `[[rules]]` plus `RelationshipDef.traversal` to `Vec<EdgeDef>`. Nothing is written to disk yet.

## Satisfies

STORY-258 AC2, AC3, AC4. The three shapes are one match arm apiece over the same source config and produce the same output vector, so they are not separable into three slices. AC1, AC5, AC6, AC7, AC8 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-258
- The three translations, term by term: ADR-032 §Decision
- Why `via = "*"` rather than `implements`, and that tightening is a later human edit: ADR-032 §Decision (final paragraph), §Consequences
- Touch:
  - `src/engine/ops/fix/config.rs` -- a private function beside `collect_config_fixes`. Nothing outside migration translates rules into edges, so this is not a `Config` method (dictum 6)
  - `src/engine/config.rs` -- read-only here: `ValidationRule`'s two variants, `RelationshipDef.traversal`, `EdgeDef`
- `EdgeDef` has no `traversal` field until ITERATION-372 lands it. AC4 and the `traversal = "chain"` half of AC2 cannot be spelled before then -- this slice is blocked on that iteration, not merely sequenced after it.
- Two source fields have no destination. `require_parent_status` is abandoned outright (ADR-033), and `relation-existence`'s `require` is read by nobody -- `validation.rs:586-591` destructures it with `..`. Dropping both is correct; doing it silently is the part the plan iteration has to surface.
- `via = "*"` matches any relationship, while `parent-child` is satisfied only by a relationship marked `traversal = "chain"` (`validation.rs:559-573`). That is a widening, not a preservation, and ADR-032 §Decision claims otherwise. Make the choice ADR-032 specifies and leave the discrepancy for the finding-set iteration to make visible; do not narrow it here on a hunch.

## Tasks

1. Test-first table, one case per AC, asserting exact field values per ADR-032 §Decision: a `parent-child` rule, a `relation-existence` rule, and a relationship carrying each `Traversal` variant. Assert `via` is the wildcard on both rule shapes -- a translation that produces `RelSelector::Named("implements")` passes a looser test and fails AC5.
2. Implement the translation as one function over `&Config` returning `Vec<EdgeDef>`, ordered rules-then-relationships so the emitted TOML is deterministic (the writer in the next slice appends in buffer order).
3. Name the rows. A rule-derived row keeps the rule's own `name`, which is what lets the finding text after migration still name what it named before. A traversal-derived row has no name to inherit: derive one from the relationship name, and make a collision with a translated rule name a hard error rather than a silent overwrite -- the writer reconciles `[[edges]]` by `name`, so a duplicate would drop a row without a word.
4. Test-first: a relationship whose `traversal` is absent contributes no row, and a config with neither `[[rules]]` nor any `traversal` key translates to an empty vector. That empty vector is the "nothing to migrate" signal the next slice's idempotence check reads.
5. Test-first: a `parent-child` rule carrying `require_parent_status` translates to exactly the same edge as one without it, with a comment at the drop site naming ADR-033. The abandonment then lives in the suite rather than in prose.

## Out of scope

- Writing anything at all: emitting `[[edges]]`, deleting the source blocks and keys, preserving unknown sections, idempotence (AC1, AC6, AC8) -> next iteration.
- Reporting what the rewrite destroys (AC7) and proving the finding set is unchanged (AC5) -> the two iterations after that.
- Removing `ValidationRule` and `RelationshipDef.traversal` from the codebase, and migrating this repo's own `.lazyspec.toml` -> STORY-259.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: a free function in the migration module, not a trait or a `Migration` type for one caller. Dictum 3: translation is engine-side; the CLI only formats what comes back.

## Verification

`cargo run -- fix --config --dry-run` on this repo is unchanged -- the translation exists but nothing calls it from `collect_config_fixes` yet, and `git diff .lazyspec.toml` after the run is empty.
