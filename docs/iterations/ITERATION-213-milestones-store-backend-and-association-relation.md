---
title: Milestones store backend and association relation
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-158
---## Changes

### REST milestone seam (`src/engine/gh.rs`)
- New `GhMilestone` struct: `number: u64`, `title`, `description`, `due_on: Option<String>` (ISO8601), `state` (open/closed), `open_issues: u64`, `closed_issues: u64`, `url`. `#[derive(Deserialize)]`, serde-rename `due_on`/`open_issues`/`closed_issues` -> REST field names.
- New `trait GhMilestoneApi` (own seam, fakeable like `GhIssueReader`/`GhIssueWriter`):
  - `fn milestone_list(&self, repo) -> Vec<GhMilestone>` -> `gh api repos/{repo}/milestones?state=all`.
  - `fn milestone_view(&self, repo, number) -> GhMilestone` -> `gh api repos/{repo}/milestones/{n}`.
  - `fn milestone_create(&self, repo, title, description, due_on, state) -> GhMilestone` -> `gh api -X POST repos/{repo}/milestones -f title=.. -f description=.. -f due_on=.. -f state=..`.
  - `fn milestone_edit(&self, repo, number, title?, description?, due_on?, state?) -> GhMilestone` -> `gh api -X PATCH repos/{repo}/milestones/{n} -f ..` only-changed fields.
  - `fn issue_set_milestone(&self, repo, issue_number, milestone: Option<u64>)` -> `gh api -X PATCH repos/{repo}/issues/{n} -F milestone=<num|null>` (assoc relation write).
- impl on `GhCli` via `run_gh_checked` (REST not GraphQL; reuses existing arg-vec + `classify_gh_error` path). Add JSON parsers `parse_milestone_json`/`parse_milestone_list_json` mirroring `parse_issue_json`.
- `#[cfg(test)] mod test_support`: extend `MockGhClient` (or new `MockGhMilestoneClient`) impl `GhMilestoneApi`: in-mem `RefCell<Vec<GhMilestone>>`, `next_number: Cell<u64>`, record `last_set_milestone: RefCell<Option<(u64, Option<u64>)>>`. create/edit mutate vec -> re-read returns updated (round-trip). No network.

### Store backend variant (`src/engine/config.rs`)
- `enum StoreBackend` += `#[serde(rename = "github-milestones")] GithubMilestones`. `Display` arm -> `"github-milestones"`.
- `StoreBackend::from_str`-equivalent maps in `src/cli/config.rs` `parse_store` (~211-214) + `src/tui/state/app.rs:150` (`"github-milestones" => GithubMilestones`).

### Milestone store impl (`src/engine/store_dispatch.rs`)
- New `pub struct GithubMilestonesStore<M: GhMilestoneApi> { client, root, repo, config, issue_map }` (reuse `IssueMap` keyed doc_id -> milestone number; same `.lazyspec/cache/<type>/` materialization as `GithubIssuesStore`).
- `impl DocumentStore for GithubMilestonesStore`:
  - `create`: doc title->milestone title, body->description, `status`->state via `status_to_milestone_state` (below) -> `milestone_create` -> milestone number becomes doc id (`type_def.make_id(number)`, mirrors github-issues id derivation: issue.number -> filename -> id, store_dispatch.rs ~250-275). `issue_map.insert(id, number, "")` + `write_cache_file`.
  - `update`: resolve number from `issue_map`; map updates `title`/`body`(->description)/`due_on` -> `milestone_edit` (changed fields only); `status` update -> `state` open/closed via `milestone_edit`. Re-read via `milestone_view` -> `write_cache_file` (round-trip). Last-write-wins: push unconditionally then refresh cache (no `check_lock`).
  - `delete`: `gh api -X DELETE repos/{repo}/milestones/{n}` (add `milestone_delete` to trait) + cache/map cleanup.
  - `set_provenance`: store in cache frontmatter only (milestone REST has no provenance field).
- Lifecycle<->state mapping helpers (module-level):
  - `status_to_milestone_state(&Status) -> "open"|"closed"`: closed-equiv statuses (`complete`,`rejected`,`superseded`) -> `"closed"`, else `"open"` (mirrors open-set logic at store_dispatch.rs:331-334).
  - `milestone_state_to_status(state) -> Status`: `"closed"` -> `complete` (closed-equiv), `"open"` -> `in-progress` (open-equiv). Applied when materializing cache from `milestone_view`.
- `%complete` computed, NOT stored: helper `percent_complete(open, closed) -> Option<u8>` = `closed*100/(open+closed)` when total>0 else None. Surfaced only at read/show time (see show below); never written to milestone, never a `--body`/frontmatter writable field.

### Dispatch wiring (`src/engine/store_dispatch.rs::dispatch_for_type`)
- Add param `milestone_store: Option<&mut GithubMilestonesStore<M>>` (new generic `M: GhMilestoneApi`); `StoreBackend::GithubMilestones => milestone_store.ok_or(...)` arm (mirror GithubIssues arm at :487). Thread option through callers (`create`/`update`/`delete` cli builders that call `dispatch_for_type`).

### Association relation (issue-doc -> milestone)
- `struct RelationshipDef` (`src/engine/config.rs:256`) += `#[serde(default, skip_serializing_if=Option::is_none)] github_native: Option<String>` (per RFC-050 Interfaces; value `"milestone"` here). Carried through `to_toml` writer + `relationship_by_name`.
- Issue-store link path (`src/cli/link.rs`): when linking with a rel whose `github_native == "milestone"` and target resolves to a `github-milestones` doc -> resolve target milestone number via milestone `issue_map`, resolve source issue number via issue `issue_map`, call `issue_set_milestone(repo, issue_num, Some(milestone_num))`. unlink -> `issue_set_milestone(.., None)`. Relation also recorded in source issue cache `related` (so it surfaces same as comment-backed rels via `src/cli/json.rs:15` `"related"` map). `PATCH issues/{n}` is the GitHub edge of record.

### Fetch / materialization (`src/cli/fetch.rs`, `src/engine/issue_cache.rs`)
- `fetch::run`: add `github-milestones` types list (filter `StoreBackend::GithubMilestones`, mirror gh_types at fetch.rs:21-27); for each, fetch `milestone_list` -> write cache files under `.lazyspec/cache/<type>/` with `milestone_state_to_status` status; reverse-derive issue->milestone relations into issue caches.

## Test Plan

- AC1 (create -> REST milestone, id=number): `GithubMilestonesStore::create` with `MockGhMilestoneClient` -> assert `milestone_create` called with title/description, returned id == `type_def.make_id(number)`, `issue_map` maps doc_id->number, cache file written.
- AC2 (update title/description/due_on round-trips PATCH): create then `update` with `[("title",..),("body",..),("due_on",..)]` -> assert mock recorded `milestone_edit` changed fields; re-read via `milestone_view` returns updated values; cache file reflects them.
- AC3 (state -> lifecycle): unit `milestone_state_to_status("closed")==complete`-class, `("open")==in-progress`-class; load milestone with `state="closed"` -> cache materialized with closed-equiv status. Reverse `status_to_milestone_state(complete)=="closed"`, `(draft)=="open"`.
- AC4 (issue->milestone assoc as relation): link issue-doc to milestone-doc via `github_native="milestone"` rel -> assert mock `issue_set_milestone` recorded `(issue_num, Some(milestone_num))`; assert source issue cache/`--json` `related` surfaces relation -> milestone doc. unlink -> `(issue_num, None)`.
- AC5 (%complete computed, read-only): unit `percent_complete(7,3)==Some(30)`, `(0,0)==None`; `show <milestone> --json` includes computed `percent_complete`; assert no writable path accepts it (update of `percent_complete` key -> ignored/err, never PATCHed).
- AC6 (trait fake, no network): all above run against `MockGhMilestoneClient` impl `GhMilestoneApi` (in-mem vec); assert zero real `gh` invocation (mock-only seam, mirrors existing `MockGhClient` tests).
- AC4 real-client clear (gh `-F` null edge case): unit on `issue_set_milestone` argv builder (factor argv into pure fn) -> `milestone: None` MUST emit `-F milestone=null` producing JSON null, NOT string `"null"`; `Some(n)` -> `-F milestone=<n>` (int). (Mock-seam unlink test already covers `(issue_num, None)`; this guards the real GhCli flag.)

## Notes

- REST not GraphQL: milestones use `gh api repos/.../milestones` (POST/PATCH/DELETE/GET); depends on STORY-155 only for shared `gh`-access plumbing, NOT the `GhGraphql` trait.
- `open_issues`/`closed_issues` read-only counts from GitHub -> `%complete` computed at read time, never stored, never writable.
- Milestones ARE authorable (create/update/delete); project boards are NOT (boards read/associate only, separate story) -- do not generalize this store to boards.
- `due_on` ISO8601 string passed through verbatim; PATCH only sends changed fields.
- Write policy last-write-wins + refresh (RFC-050): push unconditionally then re-read `milestone_view` into cache; NO optimistic `check_lock` (unlike GithubIssuesStore). Concurrent edits silently overwritten.
- Issue->milestone edge of record is `PATCH issues/{n}` `milestone` field; lazyspec mirrors it in the issue cache `related` so it shows as a normal relation. Many issues -> one milestone (GitHub native cardinality).
- Closed-equiv status set reused from existing open/closed mapping (store_dispatch.rs:331); a closed milestone always materializes to a single closed-equiv status (`complete`).
- gh `-F milestone=null` clears assoc -> must serialize to JSON null not string `"null"` (known gh `-F` edge case); `Some(n)` -> typed int. Real-client concern; guarded by AC4 argv test.
- Provenance asymmetry: milestone store keeps provenance in cache frontmatter only (REST milestone object has no provenance field) -> diverges from github-issues' HTML-comment round-trip. Acceptable this slice; provenance is local-cache metadata, not pushed to GitHub for milestones.