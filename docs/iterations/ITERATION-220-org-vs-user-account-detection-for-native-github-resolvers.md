---
title: "Org-vs-user account detection for native github resolvers"
type: iteration
status: review
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-165
---
## Changes

### 1. Shared org-then-user owner-resolution helper (`src/engine/gh_schema.rs`)
- TODAY two queries hardcode `organization(login: $owner)`: `ISSUE_TYPES_QUERY` (`:104`), `PROJECT_FIELDS_QUERY` (`:106`). On a user owner GitHub returns top-level `errors` "Could not resolve to an Organization with the login of '<owner>'" + `data.organization = null` -> `pointer("/data/organization/...")` yields `None` -> parse fns return empty Vec. Snapshot populates EMPTY, never the user's real fields. The `user(login:)` half is never tried.
- The correct pattern is `resolve_project_id_live` (`store_dispatch.rs:663`): org query -> fall back to user query -> read whichever resolved. PORT that shape into `gh_schema`.
- ADD parameterised query consts (no `organization` hardcode):
  - `ISSUE_TYPES_ORG_QUERY` = current `:104` text (org issueTypes).
  - `PROJECT_FIELDS_ORG_QUERY` = current `:106` text.
  - `PROJECT_FIELDS_USER_QUERY` = `:106` with `organization(login: $owner)` -> `user(login: $owner)` (user accounts have Projects v2; only the root selector changes). No user issue-types query exists -> issueTypes is org-only (see #3).
- ADD `enum OwnerKind { Org, User }` (derive `Copy,Clone,PartialEq`). ADD shared helper `try_org_then_user<T>(gh, org_query, user_query, vars, org_ptr, user_ptr) -> Result<(OwnerKind, &Value)>`: fire org query; if `org_resp.pointer(org_ptr).is_some()` -> `(Org, org_node)`; else fire user query, `user_resp.pointer(user_ptr).is_some()` -> `(User, user_node)`; neither -> `Err`. This is the "single owner-resolution helper" the story's In Scope calls for, and `resolve_project_id_live` (`store_dispatch.rs:663`) is REFACTORED onto it (org/user `projectV2/id` pointers) -> one resolution shape across schema-snapshot + project-id live paths, dropping the inline duplication at `:664`-`:688`.
  - Issue-types stays org-only and degrades to empty (#3) -> it does NOT route through the helper (no user issueTypes query exists); only `fetch_project_fields` and `resolve_project_id_live` do.

### 2. Route project-fields fetch through org-then-user, parse the resolved root (`src/engine/gh_schema.rs`)
- `fetch_project_fields` (`:129`): TODAY single `gh.graphql(PROJECT_FIELDS_QUERY, ...)` then `parse_project_fields(&resp, project_number)`. CHANGE: call `try_org_then_user(gh, PROJECT_FIELDS_ORG_QUERY, PROJECT_FIELDS_USER_QUERY, &[owner, number], "/data/organization/projectV2/fields/nodes", "/data/user/projectV2/fields/nodes")` -> `(kind, nodes)`; pass `nodes` straight to `parse_project_fields`. Org owner -> one round-trip; user owner -> two (org-null then user). On neither -> `Err` propagates (snapshot keeps prior, becomes a `RefreshWarning` via `refresh_schema_snapshot`).
- `parse_project_fields` (`:168`): TODAY hardwires `/data/organization/projectV2/fields/nodes` (`:177`). CHANGE signature to take the resolved nodes pointer root, OR add a `root: &str` param ("organization"|"user") and build `format!("/data/{root}/projectV2/fields/nodes")`. Cleanest: caller extracts `&Value` nodes and passes them in -> `parse_project_fields(nodes: &Value, project_number)`. Body below `:183` (`for node in nodes`) is unchanged -> field/option/iteration extraction identical regardless of root.

### 3. Issue-types: org-only, graceful empty on user accounts (`src/engine/gh_schema.rs`)
- `fetch_snapshot` (`:111`): TODAY always fires `ISSUE_TYPES_QUERY` against `organization`. On a user owner -> `data.organization=null` + top-level `errors`. `gh.graphql` returns `Ok(value)` (it does NOT bail on a GraphQL `errors` payload — `GhCli::graphql` `gh.rs:987` parses stdout JSON), so `parse_issue_types` (`:151`) reads null pointer -> empty Vec TODAY ALREADY. CONFIRM + make explicit: keep firing `ISSUE_TYPES_ORG_QUERY`; if `/data/organization/issueTypes/nodes` is absent (user account) -> `snapshot.issue_types` stays `vec![]`. NO error, NO warning. This is "graceful absence" per AC3.
- Rename `parse_issue_types` pointer + const to the `_ORG_` names for clarity; behaviour unchanged. Snapshot's empty `issue_types` -> `issue_type_id(name)` (`:75`) returns `None` for every name on a user account -> `issue_type` attribute never resolvable.

### 4. `update --attr issue_type=...` fails org-only BEFORE mutation (`src/engine/store_dispatch.rs`)
- Interception already offline-validates: `:847` `issue_type_change` match. `Some(name)` -> `GhSchemaSnapshot::load(&self.root)` (`:850`) -> `issue_type_id(name)` -> on user account snapshot this is ALWAYS `None` -> current error "invalid issue_type '{}': not a known GitHub issue type" (`:851`). This rejects before `issue_edit` (`:863`) / `push_issue_type` (`:879`) -> mutation-free reject already holds (regression-tested at `:3301` `github_update_issue_type_invalid_rejected_offline`).
- IMPROVE the message to name the org-only constraint (AC4). DISTINGUISH empty-set (user account: zero issue types at all) from unknown-name: when `snapshot.issue_types.is_empty()` -> error "native issue types require an organization-owned repository; '<owner>' has none"; else keep the existing "invalid issue_type '{name}'..." message. Owner from `split_owner_repo(&self.repo)` (`:639`). Both branches return `Err` at the same `:851` site -> still pre-mutation (no `issue_edit`, no `push_issue_type`).
- `push_issue_type` (`:240`) / `UPDATE_ISSUE_TYPE_MUTATION` (`:635`) / `CLEAR_ISSUE_TYPE_MUTATION` (`:637`) UNTOUCHED: never reached on user accounts because resolution fails first.

## Test Plan

- AC1 (user-account project fields resolve via fallback, snapshot populates, no org warning): `MockGhClient::with_graphql_responses(vec![org_null_resp, user_fields_resp])` where `org_null_resp` = `{"data":{"organization":null},"errors":[{"message":"Could not resolve to an Organization..."}]}` and `user_fields_resp` = `project_fields_response()` rerooted under `/data/user`. `fetch_project_fields(&gh, "jkaloger/lazyspec", 7)` -> fields/options/iterations non-empty, equal to the org-path expectations. Assert `graphql_calls.len() == 2` (org probe then user fallback). No warning is data here -> `refresh_schema_snapshot` returns `None` (snapshot saved Ok).
- AC2 (org account unchanged, regression): `with_graphql_responses(vec![org_fields_resp])` (existing `project_fields_response()`). `fetch_project_fields(&gh, "octo-org/repo", 7)` -> identical assertions to current `fetch_project_fields_captures_field_option_iteration_ids` (`gh_schema.rs:415`); assert `graphql_calls.len() == 1` (org path satisfied, no user fallback fired).
- AC3 (user account, no custom issue types -> empty set, attr absent, no failure): `fetch_snapshot(&gh, "jkaloger/lazyspec")` with `org_null_resp` for the issue-types call -> `snapshot.issue_types.is_empty()`; `snapshot.issue_type_id("Bug") == None`. Drive `refresh_schema_snapshot` -> returns `None` (empty set is not a failure -> no `RefreshWarning`). Read path: doc serialization with empty snapshot -> `issue_type` attribute absent (no resolvable id).
- AC4 (user account `update --attr issue_type=Bug` fails org-only, pre-mutation): seed snapshot with `issue_types: vec![]` (mirror `issue_type_store` helper `store_dispatch.rs:3190` but empty). `update(&issue_type_attr_td(), "RFC-001", &[("issue_type","Bug")])` -> `Err`; assert message contains "require an organization". Assert `graphql_calls` has ZERO `updateIssue` mutations and `issue_edit_calls`/writer untouched (mirror `:3301` assertions). 
- Regression: existing `fetch_snapshot_captures_issue_type_ids` (`:397`), `fetch_project_fields_captures_field_option_iteration_ids` (`:415`), `github_update_issue_type_sets_native_field_only` (`:3225`), `github_update_issue_type_invalid_rejected_offline` (`:3301`) still pass; resolvers-offline test (`:359`) unaffected (pure load).

## Notes

- One shared `try_org_then_user` helper backs BOTH `fetch_project_fields` AND `resolve_project_id_live` (`store_dispatch.rs:663`, refactored onto it) -> the story's "single owner-resolution helper, reused by the schema-snapshot queries and by `resolve_project_id_live`" In Scope item. No separate probe round-trip (org query doubles as the org-vs-user discriminator). Re-resolve per refresh, no cross-run cache (story Out of Scope).
- `GhCli::graphql` (`gh.rs:987`) returns `Ok` on a GraphQL `errors` payload (it deserialises stdout, does not inspect `errors`) -> org-null on a user account is `Ok(value)` with `data.organization=null`, NOT an `Err`. The null-pointer check (`is_some()`) is the org-vs-user discriminator -> do NOT switch on `errors`.
- Issue types are org-only at GitHub -> NO `user { issueTypes }` query exists; the only correct user-account behaviour is empty set + graceful absence. Empty `issue_types` must never become a `RefreshWarning` (AC3) -> `refresh_schema_snapshot` (`issue_cache.rs:218`) returns `None` on an `Ok` empty snapshot today; keep it.
- AC4 reject already lands pre-mutation via the offline `issue_type_id` check (`store_dispatch.rs:851`); this slice only upgrades the message to name the org constraint when the snapshot's issue-type set is empty. Do NOT move the check past `issue_edit` (`:863`).
- Seam is `GhGraphql` (`gh.rs:409`); fakes via `MockGhClient::with_graphql_responses` (`gh.rs:1197`) which pops canned responses FIFO and records `graphql_calls` -> two-element vec exercises org-then-user; one-element vec asserts the org path short-circuits (no fallback). Build the user-rooted fixtures by rerooting the existing `project_fields_response()` JSON from `/data/organization` to `/data/user`.
- Scope guard: board-creation (STORY-164) and TUI warning routing (STORY-163) are out; this slice only fixes resolution + graceful issue-type absence. No new deps.
