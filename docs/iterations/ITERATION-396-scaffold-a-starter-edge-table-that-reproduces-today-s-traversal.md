---
title: Scaffold a starter edge table that reproduces today's traversal
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-261
- blocks: ITERATION-397
---

## Objective

A freshly scaffolded project walks its chain and its related neighbourhood from `[[edges]]`: `starter_edges()` gains the two wildcard traversal rows, `starter_relationships()` stops carrying `traversal`, and `context` on a scaffolded project produces what it produces today.

## Satisfies

STORY-261 AC6. ITERATION-383 (STORY-259 AC4) already writes `[[edges]]` and no `[[rules]]`; what it does not write is traversal, so "reproduces today's default behaviour" is unmet after it and met here. AC1, AC4 landed in ITERATION-392, AC2 in ITERATION-393, AC3 in ITERATION-394, AC5 in ITERATION-395; AC7 lands in ITERATION-397.

## Context

- Story + ACs: STORY-261, and §Notes: "the starter set is where wildcards earn themselves -- `init` should emit a handful of readable rows, not one row per type pair"
- One config table owns the document DAG: RFC-067 §Intent
- The all-wildcard row buys no precision and exists to keep the config short: RFC-067 §"The traversal cost, stated plainly"; ADR-031 §Context, §Consequences
- That a wildcard `implements` / `chain` row reproduces today's whole-store forest exactly, and a wildcard `related-to` / `related` row today's neighbourhood: ITERATION-373 Task 6, ITERATION-374 Task 1 -- both already assert it, so this slice is scaffolding a shape the traversal slices proved
- Touch:
  - `src/engine/config.rs` -- `starter_edges()`, added by ITERATION-383 with the three constraint rows; `:596-614` `starter_relationships()`, which sets `traversal: Some(Chain)` on `implements` and `Some(Related)` on `related-to`
  - `src/cli/init.rs:22-49` `starter_config()`; `:78-100` `write_project`, which serialises with `config.to_toml()` rather than the in-place writer, so `to_toml`'s edge emission is the whole write path here
  - `src/cli/init.rs:198-207` `blank_config()` -- `edges` empty, `relationships: starter_relationships()`
  - `src/cli/init.rs:266-330` `render_dag_summary`, rewritten as an edges section by ITERATION-383; `traversal` now has a value to print
  - `src/cli/init.rs` tests `:975-995` (scaffolded project validates clean) and `tests/integration/cli_init_test.rs`
- **What ITERATION-383 left behind.** It translated the three starter *constraints* -- `stories-need-rfcs`, `iterations-need-stories`, `adrs-need-relations` -- which are `required` rows. Traversal is a separate declaration and it lives on `[[relationships]]` in `starter_relationships()`. So after ITERATION-383 a scaffolded project's DAG is declared in two tables again, which is the defect RFC-067 §Problem exists to end, and after `RelationshipDef.traversal` retires it is declared in neither and `context` walks nothing. Two wildcard rows close it: `from = "*"`, `to = "*"`, `via = "implements"`, `traversal = "chain"`, and the same for `related-to` / `related`. Five rows total in the starter set.
- **The decision this slice has to make: does `starter_relationships()` keep `traversal`.** ITERATION-373's rule is that an edge row declaring a role for relationship X suppresses `RelationshipDef.traversal` for X entirely, so keeping it changes no behaviour. Drop it anyway: `init`'s config is the one config every new project reads as the worked example (ADR-011 §Decision; `src/cli/init.rs:20-21`), and a worked example that declares traversal twice teaches the thing the table replaced. Dropping it means `starter_relationships()` no longer needs its `traversal` parameter -- check whether `fix --config`'s append path still calls it with one before removing the argument.
- **Wildcard traversal rows overlap the three constraint rows, and that must not be a load error.** `iterations-need-stories` matches `iteration --implements--> story`, and so does the wildcard `implements` row. ADR-031 §Decision composes overlapping rows for traversal and resolves requiredness by specificity, and ITERATION-372 rejects only rows that *disagree* on a role -- absence is not a disagreement. Assert that explicitly against the resolution ITERATION-371 and ITERATION-372 shipped, because the starter config is the first place in the repo where a wildcard row and a specific row cover the same triple, and if the loader reads that as a contradiction then `init` scaffolds a config that will not load.
- **What a blank-designed project loses.** `blank_config()` declares no edges, so once `RelationshipDef.traversal` is gone a from-scratch project has no traversal at all and `context` returns a chain of one. Today it inherits chain traversal from `implements` for free. That is a real regression for the `blank` path and AC6 does not reach it -- AC6 is about the starter set. ITERATION-397 gives the from-scratch wizard a way to declare edges; until then, `blank` scaffolds a DAG that constrains and walks nothing. Recorded so the gap is chosen rather than discovered, in the same spirit as ITERATION-383 §Out of scope.

## Tasks

1. Test-first, integration: `init --non-interactive`, then `create rfc`, `create story`, `link` the story to the rfc, and `context <story> --json` puts the rfc in the chain. This is AC6's real claim -- a scaffolded project's `context` works -- and no existing `init` test exercises traversal at all.
2. Add the two wildcard traversal rows to `starter_edges()`, with a comment stating what RFC-067 §"The traversal cost" says about them: they are blanket by design, and the precision the table makes available is spent by the project, not by the scaffold.
3. Test the overlap from Context: the five-row starter set strict-loads, and the wildcard `implements` row plus `iterations-need-stories` do not read as a traversal contradiction.
4. Drop `traversal` from `starter_relationships()` and check its remaining callers -- `fix --config`'s relationship append and every fixture that expects the field.
5. Extend `render_dag_summary`'s edges section to print `traversal`, and re-run the whole `init` test module: ITERATION-383 already re-based every scripted answer list once, and a summary format change breaks any test asserting on the rendered text.
6. Test the `related` half: `link ... related-to ...` on a scaffolded project surfaces in `context --json`'s `related`, matching ITERATION-374 Task 1's assertion one layer up.
7. Extend `:975-995` so the scaffolded project's edge set is asserted by content -- five rows, named -- not by count. A count assertion passes when a row is silently replaced.

## Out of scope

- The from-scratch wizard's edge loop, and the `blank` path's missing traversal -> ITERATION-397. This slice fixes the starter designer only.
- `[[rules]]`, `default_rules()` and the three constraint rows -> ITERATION-383 and ITERATION-385, all of which must land first.
- Retiring `RelationshipDef.traversal` the field. This slice stops `init` writing it; the field, its parsing and the legacy fallback ITERATION-373 built are STORY-259's to remove, and no AC on either story names them. Third slice to say so.
- Emitting one precise row per type pair instead of two wildcard rows. STORY-261 §Notes rules it out and RFC-067 §"The traversal cost" explains why: without wildcards one line becomes N-squared lines.
- `fix --config`'s migrated output. It writes traversal rows from the source relationships (ITERATION-377); `init` now writes them from the starter set. The two are no longer byte-identical -- ITERATION-383 §Context already took that consequence and re-pointed the affected test at behaviour.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 1: the scaffolded config exists to produce a project whose `validate` and `context` work; a starter set that constrains without walking is half a config. Dictum 2: the scaffolded rows are what an agent reads back through `config --json`, so they must be the spelling a human would have written.

## Verification

`lazyspec init` in an empty directory, then `create rfc "a"`, `create story "b"`, `link STORY-001 implements RFC-001`, `context STORY-001 --json`: the rfc is in the chain. `grep -c '\[\[edges\]\]' .lazyspec.toml` is 5 and `grep traversal .lazyspec.toml` finds it only inside `[[edges]]` blocks. `lazyspec validate` is clean.
