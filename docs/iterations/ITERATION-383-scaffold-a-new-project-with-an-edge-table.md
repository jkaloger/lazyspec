---
title: Scaffold a new project with an edge table
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-259
- blocks: ITERATION-384
- blocks: ITERATION-396
- blocks: ITERATION-397
---

## Objective

`init` writes `[[edges]]` and no `[[rules]]`: `starter_config()` carries the three starter constraints as edge rows, the from-scratch wizard stops offering a parent-child rule loop, and the DAG summary reports edges.

## Satisfies

STORY-259 AC4. Also clears the `ValidationRule` sites in `src/cli/init.rs` -- part of AC3, closed in ITERATION-385. AC1, AC2 land in ITERATION-384; AC5 in ITERATION-385.

## Context

- Story + ACs: STORY-259
- That `init`'s config is the single sanctioned home for defaults, and the engine load path carries none: ADR-011 §Decision; `src/cli/init.rs:21` already says so
- The edge shapes the three starter constraints translate to: STORY-258 AC2, AC3; the reference rows in `README.md` §"Edges"
- Touch:
  - `src/cli/init.rs:22-41` -- `starter_config()`, currently `rules: default_rules(), edges: Vec::new()`
  - `src/cli/init.rs:194-206` -- the blank base, whose doc comment reasons at length about rules starting empty ("NO rules ... so the from-scratch DAG never inherits a rule"). The reasoning transfers to edges verbatim; the comment does not
  - `src/cli/init.rs:243-251` -- the wizard's `while prompter.confirm("Add a parent-child rule", ...)` loop and `collect_parent_child_rule` (`src/cli/config.rs:550-604`)
  - `src/cli/init.rs:263-310` -- `render_dag_summary`. It already prints `edge:` lines for *lifecycle* edges (`:283-289`); a second, unrelated meaning of "edge" now appears in the same summary, so the DAG-edge section needs a heading that distinguishes them
  - `src/cli/init.rs` tests: `:690-700` (blank base parity), `:815-835` and `:838-870` (the two scripted rule tests), `:975-995` (the scaffolded-project-validates-clean assertion, which walks `loaded.rules`), and every scripted answer list that answers the rule prompt
  - `tests/integration/cli_init_test.rs` -- two `[[rules]]` assertions on the written config
  - `README.md:438` (the wizard description promises "prompting for each type, its lifecycle, and parent-child rules") and `:440` (names `[[rules]]` as one of the three sources of truth)
- **The decision this slice has to make.** STORY-258 translates a `parent-child` rule to `via = "*"` deliberately, to avoid tightening an existing project's config and turning valid documents into findings (STORY-258 AC2). A fresh `init` has no existing documents, so that reasoning does not apply, and `via = "*"` would scaffold exactly the imprecision the edge table exists to escape (STORY-258 §Notes). Scaffold `via = "implements"` for the two chain constraints, and take the consequence: `init`'s output is no longer the migration's output, so any test asserting the two are identical must be re-pointed at behaviour rather than bytes.
- `adrs-need-relations` translates to `to = "*"`, `via = "*"`, `required = "error"` -- the shape RFC-067 §Design names for `relation-existence`. There is no more precise spelling available; the wildcard here is the intended one.

## Tasks

1. Test-first, integration: `init --non-interactive` writes a `.lazyspec.toml` with three `[[edges]]` and no `rules` key, and the scaffolded project passes strict load and validates clean (extend the existing `:975-995` assertion rather than adding a fixture).
2. Add a `starter_edges()` beside `starter_relationships()` / `starter_types()` in `src/engine/config.rs`, and point `starter_config()` at it. Whether `default_rules()` stays for `fix --config`'s append path is ITERATION-384's problem, not this slice's -- do not delete it here.
3. Remove the wizard's parent-child rule loop and `collect_parent_child_rule`. The `blank` template already scaffolds no rules, so nothing is lost there; for `starter`, the loop only ever *added* rows on top of the starter set.
4. Rewrite `render_dag_summary`'s rules section as an edges section: `from`, `to`, `via`, `required`, and `traversal` if set. Rename the lifecycle-edge lines or the DAG-edge heading so the two senses of "edge" are distinguishable in one screen of output.
5. Re-base every scripted answer list in `init.rs`'s test module: the removed prompt shifts each positional list, and the shift is silent -- a wrong-length script feeds a `"y"` into the wrong question and still passes. Assert on the resulting config, not just on `is_ok()`.
6. README: the wizard no longer prompts for parent-child rules; `[[edges]]` replaces `[[rules]]` in the sentence naming the sources of truth at `:440`.

## Out of scope

- An interactive edge-authoring loop in the wizard, and `config add-edge` -> STORY-261. This slice removes a prompt and does not replace it: between here and STORY-261 the wizard designs types and lifecycles but not the DAG. That is a real regression in the wizard's coverage and STORY-259's ACs do not acknowledge it -- it is recorded here so the gap is chosen rather than discovered.
- Refusing `[[rules]]` at load, and `fix --config`'s append of `default_rules()` -> ITERATION-384.
- `ValidationRule`, `Config.rules` and `default_rules()` themselves -> ITERATION-385.
- Editing edges in the TUI settings panel -> STORY-260.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: the scaffolded config is what agents read back through `config --json`, so the edge rows must be the spelling a human would have written. The project instruction that a CLI change updates the README applies to the wizard's prompt set.

## Verification

`lazyspec init` in an empty directory, then `lazyspec validate` in it: clean, with `[[edges]]` naming `stories-need-rfcs`, `iterations-need-stories` and `adrs-need-relations`, and no `rules` key anywhere in the written file.
