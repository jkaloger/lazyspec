---
title: Config-driven context traversal
type: iteration
status: accepted
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-169
---

## Goal

Context traversal declared at relationship, not validation rules. Decouple context from validation.

## Decisions (locked)

- Hard cutover. Single code path. No legacy rules-derivation fallback. No version bump.
- Manual migration. Starters + init + repo `.lazyspec.toml` + README. Hand-edit.
- Empty chain = legal silent state. No warning.
- Direction implicit in declaring doc. Inverses never persisted.
- `Related` = symmetric (fwd+rev) depth-bounded BFS. Byte-for-byte match current `related-to`.
- Drop `link` from `ParentChild`. Optional re-add deferred till 2nd chain relation needs disambig.

## Data model -- `src/engine/config.rs`

- Add enum: `#[serde(rename_all="lowercase")] pub enum Traversal { Chain, Related }`.
- `RelationshipDef`: `#[serde(default)] pub traversal: Option<Traversal>`. Absence = None = neither walk.
- `ValidationRule::ParentChild`: remove `link`. Becomes `{name, child, parent, severity, require_parent_status}`.
- `starter_relationships()`: `implements` -> Chain, `related-to` -> Related. Others None (set `targets`/`member-of` Chain if those edges should surface).

## Engine -- `src/engine/store.rs`

- Replace rules-derived `chain_relationships` (~L100-118) with derivation from `config.relationships` where `traversal == Some(Chain)`.
- Add `related_relationships: Vec<String>` from `traversal == Some(Related)`.
- `parent_of` re-sources from new `chain_relationships`. `propagate_parent_links` logic untouched, only input set changes.

## Engine -- `src/engine/context.rs`

- `nodes` walk (L54-72) + `forward` set (L82-103): no logic change, already filter by `chain_relationships`.
- `related` BFS (L119-160): replace two hardcoded `== "related-to"` (L126, L133) with membership in `related_relationships`.
- L147: emit real `rel_type` of edge traversed, not hardcoded `related-to`.
- `resolve_forest`/`chain_parents` (L190): re-source, no logic change.

## Validation -- `src/engine/validation.rs` (ParentLinkRule ~L456-539)

- ParentChild check drops `link`. New: child of type `child` must have some Chain relation resolving to `parent`-type doc. Consult `chain_relationships`.
- `require_parent_status` resolves parent via satisfying Chain edge.
- `RelationExistence` (adrs-need-relations) unaffected.
- Rules <-> traversal fully independent. No cross-lint.

## Serialization / CLI

- TOML write-back (config edit path, config-write): emit `traversal`, stop emitting `link`.
- `config --json` surface: `traversal` on relationships, no `link` on rules.

## GitHub fetch

- No special wiring. `fetch.rs` emits relations by `rel_type`. Chain-ness from config `traversal` only.

## Migration

- `starter_relationships()` + init template: add `traversal`.
- Repo `.lazyspec.toml`: `implements` -> chain (+ `targets`/`member-of` if desired), `related-to` -> related; strip `link` from `stories-need-rfcs`, `iterations-need-stories`.
- README: document `traversal`, relationship-drives-context model, breaking `link` removal.

## Verify

- Unit: `Traversal` serde round-trip (lowercase, absent -> None).
- Engine: `resolve_chain` reproduces current chain + neighbourhood with implements->chain, related-to->related; no markers -> target-only, no panic.
- Engine: Chain relation w/ no rule still chains; rule w/o `link` validates via any Chain edge.
- Validation: existing parent-child fixtures pass w/o `link`; `iterations-need-stories` still fires.
- E2E: `cargo run -- context <id> --json` matches pre-change after migrate; `validate --json` clean.
- `cargo test` incl `tests/integration/fetch_milestone_relation_test.rs`.
