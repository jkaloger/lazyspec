---
title: GitHub issue-type as a classification signal for the github-issues store
type: rfc
status: accepted
author: jkaloger
date: 2026-07-02
tags: []
related:
- related-to: RFC-050
- related-to: RFC-037
---## Summary

Let a `github-issues`-store type declare its native GitHub Issue Type (`github_issue_type`) and/or an arbitrary tag (`github_issue_tag`) as classification signals, alongside or instead of the existing `lazyspec:{type}` label. When set, these fields opt a type out of the default label check entirely; when both are set they combine with AND. Two lazyspec types are allowed to both match the same underlying GitHub issue — `fetch` then materializes one independent doc per matching type, all pointing at the same issue, kept in sync last-write-wins.

## Motivation

Type classification for `github-issues`-store types is label-only today: `lazyspec:{type}` by default (RFC-037), or a custom string via `github_label` (STORY-190, accepted but unbuilt). RFC-050 added the native GitHub Issue Type field to the document model, but deliberately kept it out of classification — its Goals section states plainly: "Native issue-type readable and writable as an orthogonal document attribute (lazyspec type stays the `lazyspec:{type}` label)." Issue type today is metadata you can read and set on a doc; it plays no part in deciding what doc a GitHub issue becomes.

That leaves a gap for teams who already classify their GitHub issues with native Issue Types instead of (or in addition to) labels: "all issues in GitHub with issue type = Feature become stories in lazyspec" is not expressible today. This RFC reverses RFC-050's specific stance and makes native issue type a first-class classification signal.

## Goals

- `github_issue_type: Option<String>` on `TypeDef` — the native GitHub Issue Type name this lazyspec type matches.
- `github_issue_tag: Option<String>` on `TypeDef` — an arbitrary GitHub label this lazyspec type matches, independent of the `lazyspec:` prefix scheme. Absorbs the field STORY-190 proposed as `github_label`.
- Matching semantics per type, evaluated at fetch/classify time:
  - Neither field set: today's behavior, unchanged (`lazyspec:{type}` label match).
  - Only `github_issue_type` set: matches every issue with that native issue type; no label/tag check at all.
  - Only `github_issue_tag` set: matches every issue carrying that tag; no native-type check at all.
  - Both set: AND — an issue must carry both signals to match.
- Setting either field opts a type fully out of the default `lazyspec:{type}` fallback — no dual-checking against the old default once either is configured.
- Cross-type overlap is a supported outcome: two types may both be configured to match the same issue (e.g. both set `github_issue_type = "Feature"`, or one via tag and the other via type). No precedence or ownership mechanism arbitrates this.
- `fetch` materializes one independent cache doc per matching type from a single GitHub issue (dual materialization) — same issue number, different type dir/prefix/numbering, N docs when N types match.
- `create` for a type with `github_issue_type` configured also pushes that native issue type onto the newly created GitHub issue, reusing the existing `push_issue_type` mutation (built for manual `update --issue_type` under RFC-050).
- Discovery: label/tag-based matching keeps using the existing server-side `gh issue list --label` filter; issue-type-based matching adds a GraphQL search query (no REST filter exists for native issue type), reusing the GraphQL plumbing RFC-050 already established for issue-type resolution. For a type with only one signal configured, that signal's result set is used directly; for a type with both `github_issue_tag` and `github_issue_type` set, the two result sets intersect (AND semantics, matching the Goals above) rather than merge — a union would wrongly surface issues satisfying only one of the two configured signals.

## Non-goals

- **No precedence or ownership mechanism across overlapping type matches.** Overlap is an accepted, intended outcome, not an error state to arbitrate.
- **No conflict detection or optimistic locking on dual-materialized writes.** Two docs backed by the same issue both remain independently `update`-able; whichever writes last to the issue's single `<!-- lazyspec ... -->` frontmatter comment block wins. This matches RFC-050's existing house policy ("last-write-wins + refresh; optimistic locking for native fields is out of scope") — not a new risk class introduced here.
- **No change to `filesystem` or `github-milestones` store classification.** Scoped to `github-issues` only.
- **No resolution of STORY-190's fate here.** STORY-190 (accepted, unbuilt) proposed `github_label: Option<String>` for the same purpose `github_issue_tag` now serves. This RFC does not retire or rewrite that story; it flags the overlap as a follow-up decision once this RFC is accepted (see Decisions).

## Design

### Config schema

Two new optional `TypeDef` fields, both `#[serde(default)]`:

```rust
pub struct TypeDef {
    // ...
    pub github_issue_tag: Option<String>,
    pub github_issue_type: Option<String>,
}
```

Both are inert on non-`github-issues` types (unused by any store), following the same silent-ignore precedent STORY-190 set for `github_label` on non-`github-issues` types.

### Classification (read side)

`extract_type_and_tags` (issue_body.rs) currently walks an issue's labels once, matching the first `lazyspec:`-prefixed suffix against `known_types`, falling back to a `default_type`. That function's contract changes from "which label matches" to "does this issue satisfy this type's configured match rule", evaluated independently per candidate type rather than short-circuiting on the first hit:

- Type has neither field set → existing label-prefix check (or custom `github_label` per STORY-190, pending its reconciliation).
- Type has `github_issue_type` and/or `github_issue_tag` set → check only those signals (AND if both), skip the label-prefix check entirely.

Because a type boundary is no longer "first match wins" but "does this type's rule hold", an issue can satisfy zero, one, or several types. Zero → existing `default_type` fallback (per the type currently being fetched, since fetch is invoked per-type). Several → dual materialization (below).

The plumbing gap flagged in STORY-190 — that only bare type-name strings reach `extract_type_and_tags`, not each type's resolved label/tag/issue-type rule — applies here too and in the same call chain (`issue_cache.rs` `parse_issue`/`refresh_stale`/`fetch_all`, `store_dispatch.rs`'s three `IssueContext` builders). Threading the full per-type match rule (not just a name list) through this chain is shared groundwork for both STORY-190 and this RFC.

### Dual materialization

`fetch` is invoked per lazyspec type today and already writes into that type's own `.lazyspec/cache/<type>/` directory with its own numbering. Dual materialization requires no new storage concept: when type A's fetch and type B's fetch each independently determine a given issue matches their own rule, each writes its own doc (`TICKET-42`, `STORY-17`) into its own cache directory, unaware of the other. No cross-type coordination is introduced; the "duplication" is simply that fetch no longer assumes each issue belongs to exactly one type's cache.

Both docs remain live: subsequent `fetch` runs for either type continue reclassifying and refreshing their own copy, and `update` on either writes through to the same underlying issue.

### Discovery: label vs. issue-type search

Label/tag discovery is unchanged: `gh issue list --label <value>`, server-side filtered (gh.rs:719, 796).

Issue-type discovery has no REST filter equivalent. It uses a GraphQL search query scoped to the repo, e.g.:

```graphql
search(query: "repo:OWNER/REPO is:issue type:\"Feature\"", type: ISSUE, first: 100) {
  nodes { ... on Issue { number } }
}
```

reusing the `GhGraphql` trait RFC-050 introduced. When a type's config implies checking both label and issue-type (i.e. `github_issue_tag` and `github_issue_type` both set), both queries run and their issue-number results intersect (AND semantics — Goals); when only `github_issue_type` is set, only the GraphQL search result set is used.

### Write side: create

`create` for a type with `github_issue_type` configured pushes that value via the existing `push_issue_type` mutation (store_dispatch.rs) immediately after issue creation, the same call `update --issue_type` already uses. `github_issue_tag`, if also configured, is applied as a label at creation the same way the default `lazyspec:{type}` / custom `github_label` is today.

### Write side: dual-materialized updates

No new write path. Each materialized doc's `update` goes through the store's existing single-issue update flow unmodified. The shared-frontmatter last-write-wins behavior is a consequence of that flow being unaware another type's doc also targets the same issue — not a new code path requiring its own conflict handling.

## Relation to prior RFCs

- **Amends RFC-050.** RFC-050's Goals section states native issue-type stays "orthogonal" and that "lazyspec type stays the `lazyspec:{type}` label." This RFC replaces that stance for types that opt in via `github_issue_type`/`github_issue_tag`; types that set neither are unaffected and keep RFC-050's original orthogonal behavior.
- **Revises RFC-037.** RFC-037 states the store "uses a single `lazyspec:{type}` label per issue for type filtering" and that there is "one [label] per issue" (RFC-037:34, 78), implicitly one doc per issue. This RFC changes that invariant to "one doc per matching type" — one issue may back zero, one, or several lazyspec docs depending on how many types' rules it satisfies.
- **Overlaps STORY-190.** STORY-190 (accepted, unbuilt) proposes `github_label: Option<String>` to override the label string a type filters/creates/tags with. `github_issue_tag` in this RFC serves the same purpose (a type-configurable tag independent of the `lazyspec:` prefix). Reconciling the two — folding STORY-190 into this RFC's field, or keeping both — is an open decision for after this RFC lands (see Decisions).

## Interfaces

- `TypeDef.github_issue_tag: Option<String>` @draft, `#[serde(default)]`.
- `TypeDef.github_issue_type: Option<String>` @draft, `#[serde(default)]`.
- `extract_type_and_tags` (issue_body.rs) signature changes from "known type names + label list" to "per-type match rule (label-or-tag-or-issue-type, AND/fallback semantics) + label list" @draft.
- Fetch call chain (`issue_cache.rs` `parse_issue`, `refresh_stale`, `fetch_all`; `store_dispatch.rs`'s three `IssueContext` builders) threads full per-type match rules instead of bare `known_types: &[String]` @draft — shared with STORY-190's plumbing gap.
- New GraphQL search call for issue-type discovery, via the existing `GhGraphql` trait (RFC-050) @draft.
- `push_issue_type` (store_dispatch.rs) gains a call site from `create`, not just `update` @draft.

## Decisions (ADRs to emit)

- **Native issue type becomes a classification signal, reversing RFC-050's orthogonality stance for opted-in types.** Records why the "type stays the label" invariant no longer holds universally.
- **Overlap across types is accepted, not arbitrated.** No precedence/ownership mechanism; two types may legitimately both match one issue.
- **One doc per matching type, not one doc per issue.** Revises RFC-037's original per-issue invariant.
- **Last-write-wins extends to dual-materialized docs sharing one issue.** Consistent with RFC-050's existing native-write policy, applied to a new case (two *different* doc identities, not just concurrent edits to one).

## Stories

1. **Config schema** — `github_issue_tag` / `github_issue_type` fields on `TypeDef`, validation (inert on non-`github-issues` types, per STORY-190 precedent).
2. **Per-type match-rule plumbing** — thread label/tag/issue-type rules through the fetch call chain in place of bare `known_types`, replacing `extract_type_and_tags`'s first-match logic with independent per-type evaluation. Shares groundwork with STORY-190.
3. **Issue-type GraphQL discovery** — search-based fetch path, combined with existing label-filtered `gh issue list` results per type's configured signals (intersected when both are set, per Design).
4. **Dual materialization** — fetch writes independent cache docs per matching type from one issue; round-trip test (issue matching two types produces two docs, both refresh correctly).
5. **`create` pushes native issue type** — bidirectional write-side symmetry with `github_issue_type`.
6. **README documentation** — new fields under the `github-issues` store auth section, matching sibling field documentation conventions.

## Risks and tradeoffs

- **Silent data loss on dual-materialized writes.** Two docs, one issue, one frontmatter comment block: updating one can silently drop the other's attributes. Accepted per RFC-050 precedent; revisit if this proves surprising in practice.
- **Overlap has no guardrail against misconfiguration.** Two types both targeting `github_issue_type = "Feature"` is indistinguishable, from lazyspec's view, between "intentional dual classification" and "accidental typo/copy-paste." No validation catches the latter, by design (Non-goals) — a `validate`/`fetch` diagnostic surfacing detected overlaps (informational only, not blocking) is worth considering as follow-up, not required here.
- **GraphQL search adds a second discovery query per fetch for issue-type-enabled types**, beyond the existing label-filtered REST call. Cost scales with the number of types configuring `github_issue_type`, not with total repo issue count (search is itself server-side filtered).
- **STORY-190 overlap left unresolved.** Shipping this RFC without reconciling STORY-190's `github_label` field risks two near-identical mechanisms coexisting in config. Flagged as a Decision, not resolved here.
