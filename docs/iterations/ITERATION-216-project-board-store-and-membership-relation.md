---
title: Project board store and membership relation
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-161
---## Changes

`StoreBackend` enum @ `src/engine/config.rs:116` -> add variant `#[serde(rename = "github-projects")] GithubProjects`. `Display` impl `:126` -> `GithubProjects => "github-projects"`.

GraphQL access via `GhGraphql` seam -> DEPENDENCY interface from STORY-155/ITERATION-210 (NOT present in `gh.rs` today; only `GhIssueReader`/`GhIssueWriter`/`GhAuth` @ `gh.rs:115`/`:127`/`:155`). 210 is draft -> this iteration CANNOT execute until 210 lands. Once landed: `GhGraphql` shells `gh api graphql` (NO new dep) for ALL Projects v2 calls; fakes via the `MockGhGraphql`/extended `MockGhClient` that 210 adds @ `gh.rs:469` test_support.

New store `GithubProjectsStore<G: GhGraphql>` @ `src/engine/store_dispatch.rs` (mirror `GithubIssuesStore` @ `:143`). Fields: `client`, `root`, `repo`/`owner` (parse owner from `GithubConfig.repo` "owner/repo" @ `config.rs:514`), `config`, cache handle.
  -> impl `DocumentStore` trait (`store_dispatch.rs:38`) READ/ASSOCIATE ONLY:
  -> `create()` -> `bail!("github-projects backend does not author boards; bind to existing Projects v2 board number")`. NO `createProjectV2`. (causal: RFC non-goal -> boards human-authored on GitHub)
  -> `update()`/`set_provenance()` -> resolve board node id only; no board mutation.
  -> `delete()` -> bail (boards not deleted from lazyspec).

Board resolve fn `resolve_board(owner, n) -> project_node_id`:
  -> owner type (org|user) chosen per config/owner -> GraphQL `query{ organization(login:$owner){ projectV2(number:$n){id} } }` OR `user(login:...)`. Resolved id -> bind to doc. NO create mutation issued.
  -> board N absent under owner -> `projectV2` null -> `bail!` not-found; NEVER call create. (AC: not-found, no createProjectV2)

Cache materialize: `.lazyspec/cache/<type>/<id>.md` (cache dir keyed on `type_def.name`, the doc TYPE e.g. `project`, NOT backend name -> `root.join(".lazyspec/cache").join(&type_def.name)`; mirror `write_cache_file` @ `store_dispatch.rs:424` + `issue_cache.rs` materialize @ `:250`) -> store resolved board node id for offline lookup.

`membership` native relation:
  -> `RelationshipDef` @ `config.rs:256` -> add `#[serde(default, skip_serializing_if=Option::is_none)] github_native: Option<String>` (values "sub-issue"|"membership"). `starter_relationships()` @ `:266` adds none; user config declares `[[relationships]] name="membership" github_native="membership"`.
  -> link path @ `src/cli/link.rs:38` `link_inner` + `push_if_github_backed` @ `:124`: after frontmatter write, if rel `github_native=="membership"` -> resolve issue content id (issue node id of `from`) + project node id (of target PROJECT-n via `resolve_board`) -> GraphQL `mutation{ addProjectV2ItemById(input:{projectId:$pid, contentId:$cid}){item{id}} }`. (AC: add member)
  -> many-to-many: each membership relation = one board; N relations -> N independent `addProjectV2ItemById` calls. (AC: two boards = two calls)
  -> unlink path @ `link.rs:82` `unlink_with_config`: membership removal -> resolve project item id for that board -> `mutation{ deleteProjectV2Item(input:{projectId:$pid, itemId:$iid}) }`; other membership relations untouched. (AC: remove member, others unaffected)

`dispatch_for_type` @ `store_dispatch.rs:479` -> add `StoreBackend::GithubProjects => projects_store branch` (bail if backend unconfigured, mirror GithubIssues `:487`).

`--json` -> all ops serialize result via existing `--json` path (resolve outcome, membership add/remove outcome).

README -> document `github-projects` backend + `github_native="membership"` relation + `gh auth refresh -s project` requirement.

## Test Plan

Fakes at `GhGraphql` seam (the `MockGhGraphql`/extended `MockGhClient` @ `gh.rs:469` that ITERATION-210 introduces), TDD per AC:

- board resolve reads from projectV2 (AC1): given config owner + board N exists -> `resolve_board` issues `organization{projectV2(number:N){id}}` (org root chosen per owner type), binds returned node id, asserts NO create mutation in mock call log.
- board not-found, no create (AC2): given board N absent (mock projectV2 -> null) -> resolve `bail!`s not-found; assert mock recorded zero `createProjectV2`/create calls.
- board not authorable (AC2 corollary): `GithubProjectsStore::create()` -> err "does not author boards"; no mutation.
- issue->board membership via addProjectV2ItemById (AC3): link issue-doc --membership--> PROJECT-n -> assert mock got `addProjectV2ItemById(projectId=<board node id>, contentId=<issue node id>)`.
- many-to-many = many relations (AC4): issue-doc holds membership->N, add membership->M -> both relations persist in frontmatter AND mock recorded two `addProjectV2ItemById` calls (one per board).
- remove membership removes item, others unaffected (AC5): issue member of N and M, unlink membership->N -> mock got `deleteProjectV2Item` for N item; membership->M relation still in frontmatter, no delete for M.
- `--json` (AC6): each op `--json` -> valid serialized result.

## Notes

- READ/ASSOCIATE ONLY: boards never authored/created/deleted from lazyspec (RFC-050 non-goal) -> `create`/`delete` bail. Board owns field schema.
- Membership = many-to-many: multiple `membership` relations = multiple boards; each synced independently (one `addProjectV2ItemById` per relation). One relation per board.
- Node id resolution: project node id via `organization|user(login){projectV2(number:N){id}}` (root chosen per owner type); issue content id = issue node id from issue map / `gh` lookup.
- Auth: Projects mutations need `project` scope -> doc remedy `gh auth refresh -s project` (permission errors otherwise). Reads need `read:project`.
- DEPENDS ON ITERATION-210 (GraphQL `GhGraphql` seam) + soft ITERATION-211 (attr round-trip, only if board doc carries attrs; membership self-contained, no `--attr`). Both 210/211 draft -> cannot execute until they land. `GhGraphql`/`MockGhGraphql` do NOT yet exist in `gh.rs`.
- Board Status NOT lifecycle here (STORY-162); per-board field VALUES (`PROJECT-n.<field>`) out of scope -> STORY-162 depends on this.
- Last-write-wins on native mutations (RFC ADR); no conflict detection.