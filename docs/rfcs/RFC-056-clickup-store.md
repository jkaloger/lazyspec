---
title: "ClickUp store"
type: rfc
status: review
author: "unknown"
date: 2026-07-04
tags: []
related: []
---<!-- intent: propose a design and the decisions it forces, before code -->

## Summary
New store backend for lazyspec: `clickup-tasks`, backing task-level docs in ClickUp. Full read-write, same class as `github-issues`/`ticket`: create/update/advance on a ClickUp-backed doc pushes to the ClickUp API, not just pulls. Doc `status` is the raw ClickUp task status string; the type's lifecycle set is derived from the bound List's status workflow at sync time, not hardcoded in config.

## Motivation
Teams that track work in ClickUp instead of GitHub Issues get no lazyspec integration today — lazyspec only speaks to GitHub (`github-issues`/`github-milestones`/`github-projects`). Work tracked in ClickUp stays outside lazyspec's structured-doc pipeline: no `--json` access, no lifecycle-DAG gating, no relations linking a ClickUp task to an RFC/story/spec. This RFC gives ClickUp-tracked work the same first-class doc treatment `ticket` already gives GitHub issues.

## Goals
- CRUD parity with `ticket`: `create`/`update`/`advance` on a ClickUp-backed doc write through to the ClickUp task, not just read it.
- Read side (`Store`, `.lazyspec/cache/`) behaves identically to `github-issues` docs — same cache file shape, same freshness/staleness handling.
- `priority`/`estimate`/`due` round-trip through ClickUp's *native* task fields, not an opaque comment blob (ClickUp has native equivalents GitHub issues lack).
- One lazyspec type binds to exactly one ClickUp List (`clickup_list_id`); that List's status set becomes the type's effective lifecycle.
- `dispatch_for_type` stops being a closed generic match that every new backend must edit — ClickUp is the backend that forces this, since it would otherwise be the 5th type param bolted onto an already-closed match.

## Non-goals
- No ClickUp List/Folder/Space document type (milestone-analog) — grouping/epic mapping deferred to a future RFC.
- No OAuth app / multi-tenant auth flow. Personal API token only.
- No native ClickUp dependency-graph or linked-task API integration. Relations round-trip through a ClickUp custom field instead.
- No generalized "external tracker" trait layer refactor above `DocumentStore`. ClickUp gets its own parallel implementation, same as GitHub's — this RFC does not touch `gh.rs`/`issue_body.rs`/`issue_cache.rs`/`issue_map.rs`. This is a distinct layer from the dispatch-registry refactor below: auth/transport/field-mapping stay bespoke per backend; only the *mechanism that selects a backend* changes.

## Design

**Store dispatch refactor (prerequisite).** `dispatch_for_type` (`store_dispatch.rs:1855`) is currently a closed match generic over 4 concrete client type params (one per existing backend: gh-issues, gh-milestones, gh-projects, git-ref) — adding ClickUp as a 5th would mean a 5th generic param plus editing every call site that constructs the dispatch (CLI commands, TUI). Past 2 concrete uses, that generic-param growth is the indirection dictum 6 says to add for. Before ClickUp lands: push each backend's client generic down into its own struct as a boxed trait object (`GithubIssuesStore` holds `Box<dyn GhClient>` instead of being generic over `G: GhClient`; same for the milestone/projects/git-ref stores) and turn `dispatch_for_type` into a non-generic registry lookup (`HashMap<StoreBackend, Box<dyn DocumentStore>>` or equivalent), built once at startup. `DocumentStore` itself needs no changes — 4 methods, no generic methods, already object-safe. `StoreBackend` stays a closed enum (config validation wants a known finite backend set); only its role changes, from "carries type info into a match" to "registry key." This is a pure refactor of the existing 4 backends — no behavior change, existing tests keep passing unmodified — and is scoped as a prerequisite story (Stories, story 0) so `ClickupTasksStore` is the *first* backend added the new way, not the thing that forces the old way to bend further.

**ClickupTasksStore.** Implements `DocumentStore` (`create`/`update`/`delete`/`set_provenance`), same shape as `GithubIssuesStore` post-refactor: a concrete (non-generic) struct holding `Box<dyn ClickupClient>` internally, registered in the dispatch registry under `StoreBackend::ClickupTasks`. `Store`'s read side is untouched — it just reads whatever `ClickupTasksStore` materializes to `.lazyspec/cache/<type>/<ID>.md`.

**Status handling.** No fixed local lifecycle table. When a type is bound via `clickup_list_id`, fetch that List's status definitions from the ClickUp API and populate the type's effective lifecycle states from that set. No edges/gating locally — ClickUp already enforces its own status-transition rules, lazyspec doesn't duplicate that (same posture `ticket` already takes with its empty `lifecycle.states`/`edges`). Doc `status` is the raw ClickUp status string, verbatim, no mapping table.

**Config.** New `TypeDef` field `clickup_list_id: Option<String>` — per-type, analog to `github_label`. Each lazyspec type binds to one ClickUp List. No `[clickup] list_id` global equivalent to `[github].repo`.

**Auth.** New credential file outside the repo, e.g. `~/.lazyspec/credentials.toml` (global, never committed). New setup flow — `lazyspec setup clickup` — prompts for a personal API token, validates it against ClickUp's `/user` endpoint, writes it. Unlike `gh`, which owns its own external credential store that lazyspec never touches, ClickUp has no CLI to piggyback on — this is lazyspec's first owned credential store. Credential file only for v1; no env var fallback.

**Transport.** Native HTTP client (reqwest) — lazyspec's first native HTTP client for a store backend. GitHub goes through the `gh` CLI shellout entirely (`gh.rs`); ClickUp has no equivalent CLI. Error handling classifies by real `reqwest::Error` variants and HTTP status codes ClickUp's API actually returns, not the fragile stderr-substring scraping `gh.rs` does. Known wart in the existing approach worth naming: `classify_gh_error`/`extract_http_status` (`gh.rs:1106-1148`) scans stderr for the literal substring `"http "` and parses the next token as a status with no validation — it was observed misparsing `x509: certificate signed by unknown authority` into a fake "HTTP 509" (the digits came from `x**509**`, not a real response). Don't repeat this pattern for ClickUp.

**Field mapping.** `priority`/`estimate`/`due` map directly to ClickUp's native task fields (priority enum, `time_estimate` in ms, `due_date` epoch ms) — no HTML-comment-in-body hack like `issue_body.rs` needed for these three, since ClickUp has native equivalents GitHub issues lack. Any other attr with no native ClickUp field falls back to a ClickUp custom field, looked up by name/id via a new config field (exact shape TBD at implementation time, e.g. `clickup_custom_field_map`).

**Relations.** Round-trip through a ClickUp custom field, not ClickUp's native dependency/linked-task API. One mechanism for every relation type lazyspec needs to persist. Avoids generalizing the `github_native` mechanism — currently hardcoded to the literal field name and string-matched in `cli/link.rs`'s `apply_native_milestone`/`apply_native_membership` — into a parallel `clickup_native` path for a payoff that's marginal at this scope.

**Caching/id-mapping.** New `TaskMap` mirroring `IssueMap`'s shape (external task id, `updated_at` for optimistic-lock, any node id ClickUp exposes), persisted to `.lazyspec/task-map.json`. Reuses `CacheLock` and `write_cache_file`/`write_cache_parent`/`write_cache_child` (`store_dispatch.rs`) unchanged for cache freshness and file writing under `.lazyspec/cache/<type>/`.

## Interfaces
- `DocumentStoreRegistry` @draft — replaces `dispatch_for_type`'s generic signature with a non-generic `StoreBackend -> &mut dyn DocumentStore` lookup (`store_dispatch.rs`)
- `GithubIssuesStore`/`GithubMilestonesStore`/`GithubProjectsStore`/`GitRefStore` @draft (refactor) — each becomes non-generic, holding `Box<dyn GhClient>` / `Box<dyn GitRefClient>` / etc. internally instead of a generic client type param
- `StoreBackend::ClickupTasks` @draft (`config.rs`)
- `TypeDef.clickup_list_id: Option<String>` @draft
- `ClickupClient` trait @draft — `auth_status`/`task_list`/`task_view`/`task_create`/`task_edit`/`task_close`/custom-field ops — reqwest-backed real impl + fake impl for tests, mirroring the `GhCli`/fake split
- `ClickupTasksStore` @draft, non-generic, holding `Box<dyn ClickupClient>`, implementing `DocumentStore`
- `TaskMap` @draft (`.lazyspec/task-map.json`)
- CLI: `lazyspec setup clickup` @draft — token prompt + credential write + validation
- Credential file: `~/.lazyspec/credentials.toml` @draft, `[clickup] api_token`
- `TypeDef.clickup_custom_field_map: Option<HashMap<String, String>>` @draft — attr/relation name to ClickUp custom field id, for anything with no native ClickUp field

## Decisions (ADRs to emit)
- `dispatch_for_type` moves from a closed generic match to a non-generic registry lookup; each backend's client generic moves into its own struct as a boxed trait object.
- ClickUp store binds per-type to one ClickUp List, not a global `list_id`.
- ClickUp task status is unmediated by a local lifecycle table; type lifecycle is derived from the bound List's status set at sync time.
- ClickUp relations round-trip via a custom field, not the native dependency/linked-task API.
- ClickUp credentials live in a global (not per-repo) credential file — no external CLI-managed store to piggyback on, unlike `gh`.

## Stories
Sequenced, dependencies noted:
0. Store dispatch refactor — push each existing backend's client generic (`GhClient`, git-ref client, milestone client) into its own struct as a boxed trait object; replace `dispatch_for_type`'s generic match with a non-generic registry lookup. Pure refactor of the 4 existing backends, no behavior change, existing tests unmodified. Blocks everything below — ClickUp is the first backend added via the registry, not the 5th generic param.
1. ClickUp API client — `ClickupClient` trait, reqwest impl, fake impl. Auth/token validation only, no store integration.
2. `lazyspec setup clickup` — token capture/storage/validation against story 1's client. Blocked by 1.
3. `StoreBackend::ClickupTasks` variant + config plumbing (`clickup_list_id` field, `config_write` round-trip) + registry wiring. Blocked by 0, 1.
4. `ClickupTasksStore` read path — fetch tasks for a bound list, map to `DocMeta` (status = raw ClickUp status, `priority`/`estimate`/`due` from native fields), write cache files (`TaskMap` + `write_cache_file` reuse), populate type lifecycle from the List's status set. `lazyspec fetch` works end-to-end read-only after this. Blocked by 2, 3.
5. `ClickupTasksStore` write path — create/update/delete against the ClickUp API, optimistic-lock via `TaskMap.updated_at`, status transitions push to ClickUp. `lazyspec create`/`update`/`advance` work end-to-end after this. Blocked by 4.
6. Relations via custom field — read+write path for lazyspec relation types through a configured ClickUp custom field. Blocked by 4 (needs doc-id resolution from the read path).

## Risks and tradeoffs
- **Dispatch refactor touches 4 working backends before any ClickUp code exists.** Story 0 changes `GithubIssuesStore`/`GithubMilestonesStore`/`GithubProjectsStore`/`GitRefStore` internals (generic param to boxed trait object) with a straight port, no new tests needed beyond existing coverage — but any behavior change there is a regression in production paths unrelated to ClickUp. Mitigate by keeping story 0 a pure mechanical port, landed and reviewed independently before story 1+ starts.
- **Custom-field-only relations** are cheaper to build than native dependency wiring, but lose ClickUp's own UI-native dependency arrows/visualization. A future RFC can add `clickup_native` relations if that visibility becomes a real ask.
- **Personal API token** means ClickUp-side actions execute as the token owner's ClickUp user — audit trail attributes changes to whoever ran `setup`, not to individual agents/users. Same limitation the `gh`-token approach already carries in a shared environment; not new here.
- **No List/Folder document type yet** — any epic/grouping structure lives in ClickUp only, invisible to lazyspec's relation graph until a future RFC adds it.
- **reqwest as lazyspec's first native HTTP dependency** — new dependency weight and TLS-backend/cert-store surface the `gh`-CLI-shellout approach never had to carry. Mitigated by reqwest being the mature, default choice for Rust CLI HTTP clients (dictum 5: follow ecosystem norms).
