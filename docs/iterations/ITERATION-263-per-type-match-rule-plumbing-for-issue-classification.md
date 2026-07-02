---
title: Per-type match-rule plumbing for issue classification
type: iteration
status: in-progress
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-192
---

## Objective

`extract_type_and_tags` answers "does issue satisfy type T's rule" per-type, independent of others -- no first-hit short circuit. Thread each type's full match rule (not bare name) through the fetch/read chain.

## Context

- Story: STORY-192 (blocked by STORY-191, landed as ITERATION-262 -- `github_issue_tag`/`github_issue_type` now on `TypeDef`)
- Prior: ITERATION-261 (STORY-190) already threads a narrower per-type label through the same chain. Per STORY-192 body "Relation to STORY-190": reconcile into ONE `TypeMatchRule`-shaped pass -- extend/replace 261's plumbing, don't build a second one. `TypeMatchRule.label` is exactly what 261's plumbing resolves; if 261's `github_label()` resolver exists, reuse it for `label`'s default.
- Design (struct shape, match semantics, exact call sites/line numbers, all signature changes): STORY-192 body verbatim -- don't re-derive.
- Touch: src/engine/issue_body.rs (`TypeMatchRule` new, `IssueContext`:13-22, `deserialize`:83-119, `extract_type_and_tags`:169-190, `default_known_types()`:255 test fixture), src/engine/issue_cache.rs (`IssueContext.issue_type` source, `parse_issue`:553-568, `refresh_stale`:131-142/200-208, `fetch_all`:302-311/351-359, `insert_issue_type`:641-646 -- native type already read here, source `issue.issue_type`, test fixtures building `known_types`), src/engine/store_dispatch.rs (three `IssueContext` builders: `merge_relation_to_remote`:268-282, `update`:955-969, `set_provenance`:1092-1106), src/cli/setup.rs:47-52, src/cli/fetch.rs:117-122.

## Satisfies

STORY-192 AC1-AC6 (all -- single `TypeMatchRule` plumbing pass; independent-check semantics and the multi-match no-short-circuit guarantee are two facets of the same call-chain change, not separable slices).

## Tasks

1. Define `TypeMatchRule { name: String, label: String, tag: Option<String>, issue_type: Option<String> }` in issue_body.rs, plus a constructor from `&TypeDef` (reuse `github_label()` from ITERATION-261 if present; else `gh::type_label(&name)`).
2. Add `issue_type: Option<String>` to `IssueContext`, sourced from `GhIssue.issue_type` at the point `parse_issue` builds context.
3. Rewrite `extract_type_and_tags`: `known_types: &[TypeMatchRule]` + `issue_native_type: Option<&str>` param. Per-type independent check: neither tag/issue_type set -> label exact-match (case-insensitive); only tag -> tag match, skip label; only issue_type -> native-type match, skip label; both -> AND. No break on first match -- every candidate checked.
4. Update `deserialize`, `parse_issue`, both `IssueContext` builders in issue_cache.rs (`refresh_stale`, `fetch_all`), the three in store_dispatch.rs, and the two `all_type_names` builders (cli/setup.rs, cli/fetch.rs) to build/pass `Vec<TypeMatchRule>` instead of `Vec<String>`/`&[&str]`.
5. Populate `issue_type` on the three store_dispatch.rs `IssueContext` literals from `remote_issue.issue_type`.
6. Update existing `known_types` test fixtures (issue_body.rs `default_known_types()`, issue_cache.rs `vec!["story".to_string()]`-style literals) to `TypeMatchRule` construction.
7. New unit test directly on `extract_type_and_tags`: two types both matching one issue -> both checks run, neither short-circuits (per AC5).

## Out of scope

- Discovery/GraphQL query construction -- STORY-193.
- Writing >1 cache doc on multi-match -- STORY-194.
- `create` pushing `github_issue_type` -- STORY-195.
- README -- STORY-196.

## Principles/conventions

Engine-only change (CONVENTION.md L3) -- no CLI/TUI surface change. Reuses ITERATION-261's plumbing shape rather than duplicating it, per story's explicit "one pass, not two" instruction.

## Verification

- Unconfigured type (no tag/issue_type): issue with matching label classifies exactly as before -- regression-free.
- Tag-only type: issue with that label but no `lazyspec:{name}` label at all still matches.
- Issue-type-only type: issue with matching native type but no label at all still matches.
- Both set: issue satisfying only one of the two does NOT match (AND, not OR).
- Two types both matching one issue: unit test on `extract_type_and_tags` shows both checks execute (no short-circuit).
- All listed call sites compile against `Vec<TypeMatchRule>`; existing fetch/update/set_provenance/merge_relation_to_remote tests for unconfigured types pass unchanged.

