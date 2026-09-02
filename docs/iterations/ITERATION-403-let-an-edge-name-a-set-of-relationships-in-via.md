---
title: Let an edge name a set of relationships in via
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-02
tags: []
related:
- implements: STORY-258
- blocks: ITERATION-404
---

## Objective

`via` on an `[[edges]]` row takes a set of relationship names, matching a member of the set satisfies the row, and every surface that reads or writes a `via` follows.

## Satisfies

Groundwork for STORY-258 AC5. No AC lands until ITERATION-404 uses the set; this slice is the schema change alone.

## Context

- Story + ACs: STORY-258
- Decision, second amendment: ADR-032 §Decision -- "`via` takes a set of relationship names, exactly as `to` takes a set of type names"
- `to` is the model to mirror, position for position: `TypeSelector` at `src/engine/config.rs:218-260` has `names`, `matches`, `is_concrete`, `intersects`, a `Serialize` that re-emits a one-element set as a bare string, and a `Deserialize` that accepts `"*"`, a bare name, or a list. `RelSelector` (`:264-303`) has the same five plus `name() -> Option<&str>`.
- Touch:
  - `src/engine/config.rs` -- `RelSelector::Named(String)` becomes a set; `name()` becomes `names() -> &[String]`; `RelSelectorRepr` (`:320`) gains the untagged one-or-many shape `TypeSelectorRepr` already has; the declared-relationship check at `:1508-1513` loops the set; the "declares no `via`" message at `:1467-1470` is unchanged in meaning
  - `src/engine/validation.rs:147` -- `via_phrase` reads one relationship. A set is a disjunction, so it reads like the `to` disjunction the finding already prints
  - `src/engine/config_write.rs:636,666` -- `rel_selector_str` returns `&str` and cannot spell a set; writing `via` follows however `to` is written
  - `src/engine/ops/fix/config.rs:158` -- `edges_from_rule` still emits a row per chain relationship. Leave that shape alone; ITERATION-404 changes it
  - `README.md:669` (the `via` row of the field table), `:704`, `:729` (specificity -- a named position counts once whether it names one relationship or six, exactly as for a type)
  - `src/engine/config.rs:1915` -- the emitted JSON schema is asserted on

## Tasks

1. Test-first, in `config.rs`: `via = "implements"`, `via = ["implements", "targets"]` and `via = "*"` all parse; a one-element set re-emits as a bare string; an unknown name anywhere in the set is refused naming that member.
2. Change `RelSelector` and its serde and schema impls to the set shape. Follow `TypeSelector` rather than inventing a second spelling -- if the two end up structurally identical, say so in the report and do not pre-emptively merge them (dictum 6 is about two concrete uses, and there are now two).
3. Test-first, in `validation.rs`: a row whose `via` names two relationships is satisfied by a document carrying either, and reported once -- not once per member -- when it carries neither. Update `matches_target`, `specificity` and `intersects` callers as the compiler finds them.
4. `via_phrase`: an unsatisfied set-valued row names every relationship that would satisfy it. The `to` disjunction in the same message is the wording to match.
5. Write the set through `config_write`, with a round-trip test: a two-name `via` written and re-read is the same selector.
6. README `[[edges]]` section: the `via` field row, the one-or-many spelling alongside `to`'s, and the specificity sentence.

## Out of scope

- The migration's translation shape -- `edges_from_rule` keeps emitting a row per chain relationship until ITERATION-404.
- Un-ignoring `the_finding_set_survives_the_migration`. It stays ignored through this slice; ITERATION-404 is what makes it pass.
- The TUI settings panel and the config CLI edge commands, which do not exist yet (STORY-260, STORY-261). Their iterations must account for a set-valued `via`.

## Principles/conventions

`cargo run --quiet -- convention`. Dictum 2 (`--json` carries the same facts as the human text), dictum 5 (mirror the Rust-idiomatic shape already in the file), dictum 6.

## Verification

`cargo run --quiet -- config schema --json | jq '.["$defs"].RelSelectorRepr'` shows the one-or-many shape, not a bare string.
