---
title: Per-type match-rule plumbing for issue classification
type: story
status: complete
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-055
---## Problem

`extract_type_and_tags` (src/engine/issue_body.rs:169-190) decides a fetched GitHub issue's lazyspec type by walking its labels once and matching the first `lazyspec:`-prefixed suffix against `known_types: &[&str]` — bare type names, first hit wins, loop then breaks in spirit (`doc_type.is_none()` guard). That contract cannot express RFC-055's per-type match rules: a type may instead (or additionally) match on an arbitrary `github_issue_tag` label or a native `github_issue_type`, AND-combined when both are set on one type, with the label-prefix check skipped entirely once either is configured (RFC-055 Design/Goals). "First match in label order" and "does type T's own rule hold" are different questions, and the current call chain cannot ask the second one because only bare type-name strings — not each type's full label/tag/issue-type rule — ever reach `extract_type_and_tags`.

This is the same plumbing gap STORY-190 identified for its own purpose (custom `github_label` per type): `Config` (the full `TypeDef` list) is available one level above every call site, but only bare names get extracted before being passed down through `parse_issue` → `IssueContext` → `deserialize` → `extract_type_and_tags`. STORY-190 and this story need the identical fix — thread each type's resolved match data through this chain instead of `Vec<String>`/`&[&str]` of names — and should be built as one plumbing pass, not two competing ones. See "Relation to STORY-190" below.

## Goal

Change `extract_type_and_tags`'s contract from "which label matches first" to "does this issue satisfy type T's configured match rule", evaluated independently for every candidate type (no short-circuit on first hit). Thread each type's full match rule — not just its name — through the fetch/read call chain so this evaluation is possible.

This story is plumbing only. It does not change what `fetch` requests from GitHub (STORY-193, not yet built), and it does not implement writing more than one cache doc when an issue satisfies more than one type's rule (STORY-194, not yet built). Within this story, an issue that satisfies zero or one types behaves as today (falls back to `default_type` on zero); the multi-match case is made *classifiable* here so STORY-194 has something to consume, but *consuming* it (dual materialization) is explicitly out of scope.

## Depends on

Blocked by STORY-191 (config schema): this story consumes `TypeDef.github_issue_tag: Option<String>` and `TypeDef.github_issue_type: Option<String>`, which do not exist yet. No match rule can be built from a `TypeDef` until STORY-191 lands.

## Design

### Match rule shape

Introduce a small struct in src/engine/issue_body.rs, alongside `IssueContext`, capturing one type's resolved classification rule:

- `name: String` — the lazyspec type name (`DocType` value on match).
- `label: String` — the label checked when neither of the below is set. Defaults to `gh::type_label(&type_def.name)` (today's `lazyspec:{name}`) until/unless STORY-190's `github_label()` resolver exists, at which point only this one line of the constructor changes — see "Relation to STORY-190".
- `tag: Option<String>` — from `TypeDef.github_issue_tag` (STORY-191).
- `issue_type: Option<String>` — from `TypeDef.github_issue_type` (STORY-191).

Built once per type from its `TypeDef`, e.g. a `From<&TypeDef>`/constructor function callers use wherever `known_types` is assembled today.

### Match semantics (independent per type, per RFC-055 Design/Goals)

For a given type's rule and a given issue (its labels + native issue type):

- `tag` and `issue_type` both `None` → default: issue carries the `lazyspec:{label}`-style label (today's exact-prefix check, case-insensitive).
- Only `tag` set → issue carries that label; label-prefix check skipped entirely.
- Only `issue_type` set → issue's native issue type equals it; label-prefix check skipped entirely.
- Both set → AND: issue must satisfy both.

Every candidate type's rule is checked against the issue — no `break`/first-hit short circuit. Zero types satisfied → `default_type` fallback (the type currently being fetched/parsed), unchanged from today. One type satisfied → that type. More than one satisfied → out of this story's scope to *act* on (STORY-194); this story's job is only that the check does not silently stop at the first match and mask a second one, since STORY-194's dual materialization has nothing to build on otherwise.

### Native issue type must reach the check

`IssueContext` (src/engine/issue_body.rs:13-22) currently carries `labels: Vec<String>` but nothing about the issue's native GitHub issue type — that's only read later, by `insert_issue_type` (src/engine/issue_cache.rs:641-646), *after* `deserialize`/`extract_type_and_tags` have already run. A `github_issue_type` rule cannot be evaluated without it. Add `issue_type: Option<String>` to `IssueContext`, sourced from `GhIssue.issue_type` (src/engine/gh.rs:51) at the same point `parse_issue` builds the rest of the context.

### Signature changes (all verified against current source)

- `IssueContext` (src/engine/issue_body.rs:13-22): `known_types: Vec<String>` → `known_types: Vec<TypeMatchRule>`; add `issue_type: Option<String>`.
- `deserialize` (src/engine/issue_body.rs:83-119, specifically the `known_type_refs`/`extract_type_and_tags` call at lines 95-96): drop the `&str`-collecting intermediate, pass `&ctx.known_types` and `ctx.issue_type.as_deref()` straight through.
- `extract_type_and_tags` (src/engine/issue_body.rs:169-190): `known_types: &[&str]` → `known_types: &[TypeMatchRule]`; add an `issue_native_type: Option<&str>` parameter; replace the label loop's first-match logic with the independent per-type check above.
- `parse_issue` (src/engine/issue_cache.rs:553-568): `known_types: &[String]` → `known_types: &[TypeMatchRule]`; populate the new `IssueContext.issue_type` from `issue.issue_type.clone()`.
- Its two call sites: `refresh_stale` (src/engine/issue_cache.rs:131-142 signature, call at 200-208) and `fetch_all` (src/engine/issue_cache.rs:302-311 signature, call at 351-359) both take `known_types: &[String]` as a parameter and pass it straight through — change both signatures to `&[TypeMatchRule]`.
- Two upstream builders that currently construct the bare `Vec<String>` fed into `fetch_all`'s `known_types` parameter: src/cli/setup.rs:47-52 (`all_type_names`) and src/cli/fetch.rs:117-122 (`all_type_names`) — both build `config.documents.types.iter().map(|t| t.name.clone()).collect()` and must build `Vec<TypeMatchRule>` instead.
- Three direct `IssueContext` builders in src/engine/store_dispatch.rs — `merge_relation_to_remote` (268-282), `update` (955-969), `set_provenance` (1092-1106) — all build `known_types` as `self.config.documents.types.iter().map(|t| t.name.clone()).collect()` today; all three switch to building `Vec<TypeMatchRule>` the same way, and all three also need an `issue_type` field added to the `IssueContext` literal, sourced from the remote issue already in scope (`remote_issue.issue_type` — same field `parse_issue` reads).
- Existing unit tests in src/engine/issue_body.rs and src/engine/issue_cache.rs that construct `known_types` as `Vec<String>`/`&[&str]` literals (e.g. `default_known_types()` at issue_body.rs:255, and the many `let known_types = vec!["story".to_string()]` fixtures in issue_cache.rs) need mechanical updates to the new type; not enumerated individually here.

### Relation to STORY-190

STORY-190 needs the exact same plumbing change (bare type names → richer per-type data reaching `extract_type_and_tags`) to resolve custom `github_label` values instead of assuming `lazyspec:{name}`. This is not a coincidence: both stories hit the identical call chain (`parse_issue` → the three `IssueContext` builders → `deserialize` → `extract_type_and_tags`) for the same reason — only names cross that boundary today. The `TypeMatchRule.label` field this story introduces is exactly the field STORY-190 needs to populate from `github_label`; if STORY-190 is implemented after this story, its change is confined to how `label` is computed (swap the `gh::type_label(&type_def.name)` default for `type_def.github_label()`), not the plumbing shape. If STORY-190 lands first, this story's `TypeMatchRule` should reuse its resolver rather than reintroducing a second one. Either order, one `TypeMatchRule`-shaped plumbing pass should satisfy both stories — implementers should not build this twice.

## Non-goals

- Discovery changes (server-side `gh issue list --label`/GraphQL search query construction per type's configured signals) — STORY-193.
- Writing more than one cache doc per issue when several types' rules match — STORY-194.
- Pushing `github_issue_type` to GitHub on `create` — STORY-195.
- README documentation of the new fields — STORY-196.
- Resolving STORY-190's `github_label` vs. this RFC's `github_issue_tag` overlap — RFC-055 flags that as an open decision (RFC-055 Decisions), not this story's concern beyond sharing plumbing cleanly.

## Acceptance criteria

- **Given** a type with neither `github_issue_tag` nor `github_issue_type` set, **when** an issue carrying its `lazyspec:{name}` (or STORY-190 custom) label is classified, **then** it matches exactly as before (regression-free default).
- **Given** a type with only `github_issue_tag` set, **when** an issue carries that tag as a plain label but has no `lazyspec:{name}` label at all, **then** it still matches that type (label-prefix check is skipped, not required in addition).
- **Given** a type with only `github_issue_type` set, **when** an issue's native issue type equals it but the issue carries no matching label of any kind, **then** it still matches that type.
- **Given** a type with both `github_issue_tag` and `github_issue_type` set, **when** an issue satisfies only one of the two, **then** it does not match that type (AND, not OR).
- **Given** two types whose rules both independently match one issue, **when** classification runs, **then** the check for the second type is still performed and not skipped because the first already matched (no first-hit short circuit) — verified by a unit test on `extract_type_and_tags` directly, independent of how a caller currently chooses to act on more than one match.
- **Given** the existing `known_types: &[&str]`/`Vec<String>`-based call chain, **when** this story lands, **then** every call site listed in Design (issue_body.rs, issue_cache.rs, store_dispatch.rs, cli/setup.rs, cli/fetch.rs) compiles against `Vec<TypeMatchRule>` and existing fetch/update/set_provenance/merge_relation_to_remote behavior for unconfigured types (STORY-191 fields absent) is unchanged.
