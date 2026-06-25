---
title: Native issue-type as document attribute
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-157
---## Changes

Slice = native GitHub issue-type → orthogonal `issue_type` doc attribute, bidirectional. Builds on 210 (`GhGraphql` trait + `gh-schema.json` snapshot caching issue-type **ids**) + 211 (`--attr` write path + github attribute round-trip). lazyspec type label (`lazyspec:{type}`) untouched throughout.

### Read path (issue → `issue_type` attr)

- `src/engine/gh.rs` `struct GhIssue` (line 22) → add field `pub issue_type: Option<String>` (the native `issueType.name`; `None` when issue has no type). Serde-flatten/map from issue JSON.
- `src/engine/gh.rs` `impl GhIssueReader for GhCli::issue_view` (line 233) — `gh issue view` JSON does not expose `issueType`; fetch native type via the 210 `GhGraphql` seam (`issue(number){ issueType { name id } }`) and populate `GhIssue.issue_type`. GA 2025-03-17 → no `issue_types` preview header. MockGhClient (gh.rs:469) gains a settable `issue_type` so tests drive it.
- `src/engine/issue_cache.rs` `fn parse_issue` (line 302) — after `issue_body::deserialize` returns `meta` → if `issue.issue_type` is `Some(name)` then `meta.attributes.insert("issue_type", AttrValue::Str(name))`; if `None` → insert nothing (absent, not empty/default). Same insertion on the fallback (non-lazyspec body) `DocMeta` branch (line ~328). issue_type is sourced ONLY from native field, never from labels → orthogonality.
- `AttrValue` from `src/engine/document.rs:186`; `meta.attributes: BTreeMap<String,AttrValue>` (document.rs:270). issue_type rides existing attribute serialization → surfaces in `show --json` / `status --json` for free (no extra code; same path as any attr).

### Write path (`--attr issue_type=<v>` → `updateIssue{issueTypeId}`)

- `src/engine/store_dispatch.rs` — `GithubIssuesStore<G>` bound `G: GhIssueReader + GhIssueWriter` (line 216) → widen to `+ GhGraphql`. The 211 `--attr` write lands attrs into the issue-body HTML comment via `issue_body::serialize`; this slice intercepts `issue_type` BEFORE the round-trip so it is NOT stored in the comment (native field is sole home).
- In the attribute-write entrypoint (211's `DocumentStore` set-attr path, store_dispatch ~line 358-442), split incoming attrs: pull out `issue_type` → handle natively; pass remainder to the HTML-comment round-trip.
  - Resolve `name → issueTypeId` against the snapshot: read `.lazyspec/cache/gh-schema.json` (210), look up org issue-type list by name (case-sensitive match on cached `name`), take cached `id`.
  - Non-empty value `issue_type=Bug` → `GhGraphql::graphql("mutation{ updateIssue(input:{id:$issueId, issueTypeId:$typeId}){...} }")` with vars `issueId` (issue node id) + `issueTypeId` (resolved id). Exactly one `updateIssue` mutation. No dedicated set-type mutation exists → ride `updateIssue`. id-keyed (not name-keyed) per RFC-050.
  - Empty value `issue_type=` (clear) → same `updateIssue` with `issueTypeId: null`. Distinct from set; `null` clears (no clear-specific mutation needed for issue-type, unlike project single-select).
  - Validation: value not in snapshot issue-type list → reject OFFLINE before any mutation; error names the invalid value; non-zero exit. Empty value bypasses validation (clear is always valid).
- Issue node id (`issueId`) for the mutation: from the 210 GraphQL read (`issue(number){ id }`) or cached in the issue_map entry; reuse existing `self.client.issue_view` lock path (store_dispatch.rs `check_lock` line 185) to get number → resolve id via GraphQL.
- `src/cli/update.rs` `run_with_config` (line 18) — no change beyond 211's `--attr` plumbing; issue_type is just another `(key,value)` pair routed to the github store.

### Snapshot (210 dependency, asserted here)

- `gh-schema.json` MUST cache org issue-type `{name, id}` pairs (210 STORY-155 deliverable). This iteration consumes ids for name→id resolution and the name list for offline validation. No snapshot-writing code added here; if 210 cached names-only, that is a 210 gap to surface.

## Test Plan

One per AC. Tests use `MockGhClient` (gh.rs:469) extended with `issue_type` getter + recorded GraphQL mutations, and a fixture `gh-schema.json` with `Bug`→id `IT_bug`.

- **AC1 read surfaces issue_type** — `parse_issue` test (issue_cache.rs tests, ~line 940): `GhIssue{ issue_type: Some("Bug"), labels:["lazyspec:story"], .. }` → assert `meta.attributes["issue_type"] == AttrValue::Str("Bug")`. Also assert it appears in `show --json` output (`.attributes.issue_type == "Bug"`).
- **AC2 absent when unset** — `GhIssue{ issue_type: None, .. }` → assert `meta.attributes.get("issue_type").is_none()` (not `""`, not a default).
- **AC3 write maps name→id, one mutation** — snapshot has `Bug→IT_bug`; run set-attr `issue_type=Bug` → assert MockGhClient recorded exactly ONE `updateIssue` mutation with var `issueTypeId == "IT_bug"`; assert NO `issue_type` written to issue-body HTML comment.
- **AC4 clear sends null** — doc with `issue_type=Bug`; run `issue_type=` (empty) → assert recorded `updateIssue` mutation carries `issueTypeId == null` (serde_json::Value::Null).
- **AC5 invalid rejected offline** — snapshot lacks `Nonsense`; run `issue_type=Nonsense` → assert error returned, error string contains `"Nonsense"`, and MockGhClient recorded ZERO mutations (rejected before GraphQL call).
- **AC6 lazyspec type label orthogonal** — issue carries `lazyspec:story` label + `issue_type=Bug`; on read assert `meta.doc_type == story` AND `issue_type == Bug` both present; on write of `issue_type=Task` assert NO label add/remove recorded (`issue_edit` labels_add/labels_remove empty) and `doc_type` unchanged.

## Notes

- Native issue-types GA 2025-03-17 → omit the legacy `issue_types` preview Accept header on all read/mutation calls.
- Orthogonal to lazyspec type: `issue_type` ↔ native `issueType` field only; `lazyspec:{type}` label is the sole lazyspec-type source and is never read/written by this slice. An issue can be lazyspec `story` + GitHub `Bug` simultaneously.
- Depends on 210 (`GhGraphql` trait seam + `gh-schema.json` caching issue-type **ids**) and 211 (`--attr` flag + github attribute HTML-comment round-trip). If either seam is missing/incomplete (esp. snapshot caching names-only), that blocks this iteration → surface as upstream gap, do not re-implement here.
- Writes are id-keyed: every native write resolves name→id via snapshot then keys the mutation off the id (no dedicated set-type mutation → `updateIssue{issueTypeId}`; `null` clears). RFC-050 native-attribute shape 3.
- Last-write-wins (RFC-050): mutation pushes unconditionally; no conflict detection on issue_type. Stale snapshot can pass validation for an option GitHub since removed → GraphQL error is the backstop.