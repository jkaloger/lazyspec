---
title: "github-projects board creation"
type: iteration
status: complete
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-164
---
## Changes

### 1. Owner node id resolver, org-or-user (`src/engine/store_dispatch.rs`)
- `createProjectV2(input: { ownerId, title })` needs `ownerId` = owner's GraphQL node id, NOT `owner/repo` login. NO resolver today -> ADD `fn resolve_owner_node_id<G: GhGraphql>(client: &G, owner: &str) -> Result<String>`.
- Mirror `resolve_project_id_live` (`:663`) org-then-user shape: query `organization(login: $owner) { id }`, fall back to `user(login: $owner) { id }`. ADD two consts beside `PROJECT_NODE_ID_ORG_QUERY` (`:1180`)/`PROJECT_NODE_ID_USER_QUERY` (`:1182`): `OWNER_NODE_ID_ORG_QUERY = "query($owner: String!) { organization(login: $owner) { id } }"`, `OWNER_NODE_ID_USER_QUERY = "query($owner: String!) { user(login: $owner) { id } }"`.
- Pointers `/data/organization/id` then `/data/user/id`; both null -> `bail!("owner '{}' not found as org or user", owner)`.
- STORY-165 (org-vs-user detection) extracts this org-then-user fallback into ONE helper -> on merge, route `resolve_owner_node_id` + `resolve_board` + `resolve_project_id_live` through that shared seam. This slice ships its own resolver if 165 not yet landed; same query pair, same pointers -> trivial dedup later. NO org-only assumption (AC4).

### 2. `create` authors the board (`src/engine/store_dispatch.rs:1289`)
- TODAY `create` (`:1289`) is a pure `bail!(...)` (`:1296`). REPLACE body.
- `let owner = owner_of(&self.repo)?` (`:1186`). `let owner_id = resolve_owner_node_id(&self.client, owner)?` (Change 1).
- Issue mutation over `GhGraphql` seam: `self.client.graphql(CREATE_PROJECT_V2_MUTATION, &[("ownerId", GqlVar::Str(owner_id)), ("title", GqlVar::Str(title.to_string()))])?`. ADD const `CREATE_PROJECT_V2_MUTATION = "mutation($ownerId: ID!, $title: String!) { createProjectV2(input: { ownerId: $ownerId, title: $title }) { projectV2 { id number } } }"`.
- BEFORE parsing success: scope-missing detection (Change 3) on the response.
- Parse `/data/createProjectV2/projectV2/number` (u64) and `/.../id` (str). Missing -> `bail!("createProjectV2 returned no board number")`. `_body`/`_author` unused (boards carry no body) -> keep `_`-prefix on those params.
- Doc id = board number as `PROJECT-{number}` (round-trips through `board_number` `:1195`). Persist node id binding: `self.issue_map.insert(&doc_id, number, "", node_id.clone())` (`issue_map.rs:49`), `self.issue_map.save(&self.root)?`. Materialize cache via the SAME `DocMeta` + `write_cache_file` block `bind_board` uses (`:1267`-`:1282`) -> so `PROJECT-n` membership/field writes resolve offline (AC3).
- Return `CreatedDoc { path, id: doc_id }` (`:39`). `path` = cache-relative path of the written file (match `bind_board`'s `write_cache_file` target; github-issues `create` builds its relative path at `:96` -> follow that shape).

### 3. Scope-missing -> actionable error (`src/engine/store_dispatch.rs`, in `create`)
- Board creation needs `project` token scope; `repo` does NOT grant it -> GraphQL returns top-level `errors[]` with a permission/scope message, NOT a populated `data`.
- After the `graphql` call in Change 2, inspect response: if `/data/createProjectV2` absent OR `errors` present with a scope/permission signal -> `bail!("Projects v2 board creation needs the `project` token scope; run `gh auth refresh -s project`")`. NAME the remedy, mirror the schema-snapshot wording at `issue_cache.rs:234` ("projects need `gh auth refresh -s project`").
- ADD `fn missing_project_scope(resp: &serde_json::Value) -> bool`: true when `resp.pointer("/errors")` array contains a message matching `INSUFFICIENT_SCOPES`/`project` scope / "resource not accessible" / "does not have permission". Bail BEFORE any persist -> no doc, no issue-map write on the scope path (AC2).

### 4. CLI create dispatch already wired (`src/cli/create.rs:156`)
- `run_with_body` ALREADY routes `StoreBackend::GithubProjects` (`:156`-`:178`): builds `GithubProjectsStore { client: GhCli::new(), root, repo, config, issue_map }` and calls `store.create(type_def, title, author, body)` (`:176`). NO dispatch change needed -> `create` going from bail to real impl is picked up here unchanged.
- `run_json`/`run_json_with_body` (`:228`,`:250`) delegate to the same path -> `--json` on `create` preserved automatically; returned `CreatedDoc.path`/`id` flow into existing JSON emit. NO signature change.

## Test Plan

- AC1 (create issues `createProjectV2`, id = returned number): `projects_store(&root, vec![owner_org_response("OWN_org"), create_project_response(42, "PVT_42")])` -> `store.create(&projects_type_def(), "My Board", "", "")`. Assert returned `CreatedDoc.id == "PROJECT-42"`; assert a `graphql_calls` entry query `.contains("createProjectV2")` with vars `ownerId=OWN_org`, `title="My Board"`; assert `store.issue_map.get("PROJECT-42")` has `issue_number==42`, `node_id=="PVT_42"`. Owner resolved via `organization(login:)` first call.
- AC2 (missing `project` scope -> actionable error, no persist): canned response = scope error `json!({"errors":[{"type":"INSUFFICIENT_SCOPES","message":"... `project` scope ..."}]})` after owner resolve -> `create` returns `Err`; assert message contains `gh auth refresh -s project`; assert `IssueMap::load(&root)` has NO `PROJECT-*` entry and no cache file written.
- AC3 (membership resolves freshly created board): after AC1-style create, `store.issue_map.get("PROJECT-42").node_id == "PVT_42"` -> a subsequent github-issues field/membership write keying off `PROJECT-42` resolves the board offline with NO further graphql (no UI step). Assert membership path reads the cached binding (extend the existing membership test fixture with the post-create map).
- AC4 (owner is user account): `projects_store` with owner queries returning org-null then `user_owner_response("OWN_usr")`, then create-success -> assert `createProjectV2` `ownerId==OWN_usr`; assert resolution fell through `organization` (null) to `user(login:)`. Symmetric org-account test (owner resolves on first `organization` query) for regression.
- Fakes/seams: all at `GhGraphql` via `MockGhClient::with_graphql_responses` (`gh.rs:1197`); `graphql_calls` (`:1578`) records query+vars for assertions. ADD helpers beside `org_board_response` (`:3596`): `owner_org_response(id)`/`user_owner_response(id)` = `{"data":{"organization":{"id":..}}}` / `{"data":{"user":{"id":..}}}`; `create_project_response(number,id)` = `{"data":{"createProjectV2":{"projectV2":{"id":id,"number":number}}}}`. `--json` create test asserts emitted JSON carries the new `PROJECT-n` id.

## Notes

- `delete` stays a `bail!` (`:1311`) -> destroying a board is out of scope; unbinding a doc without destroying the board is a separate concern. Untouched.
- `ownerId` is a NODE id (`PVT_`/`O_`/`U_`-style), NOT the login -> Change 1 mandatory; passing `owner/repo` login to `createProjectV2` fails. This is the one new resolver this slice needs over STORY-161's read path.
- Owner resolution depends on STORY-165's org-or-user helper. If 165 lands first, DROP Change 1's bespoke resolver and call its shared seam; if not, this slice's `resolve_owner_node_id` + the two OWNER_NODE_ID consts are the bridge -> identical org-then-user pattern, dedup is mechanical.
- Scope detection lives on the response, NOT a pre-flight `gh auth status` -> avoids an extra shell-out; the GraphQL error IS the signal. Error wording matches the schema-snapshot precedent (`issue_cache.rs:234`) so users see ONE consistent remedy string.
- Board carries NO body/author -> `create`'s `_body`/`_author` stay unused; a board doc's identity is purely its `PROJECT-n` number (the same invariant `board_number`/`resolve_board` already enforce).
- Persist mirror: `create` reuses `bind_board`'s `DocMeta` + `write_cache_file` block (`:1267`) so the freshly created board is cached identically to a bound one -> AC3 (membership offline) holds with no special-casing.
- No new deps; no GraphQL beyond `createProjectV2` + the two owner-id queries. `--json` on create/show/status untouched.
