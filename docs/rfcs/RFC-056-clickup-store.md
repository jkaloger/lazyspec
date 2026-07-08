---
title: "ClickUp store"
type: rfc
status: accepted
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
- No passphrase-encrypted file store. At-rest encryption comes from the OS keychain (see Auth); the plaintext file fallback stays plaintext by design, for headless/CI where no keychain backend exists. A bespoke encrypted-file scheme (age/argon2) is out of scope for v1.
- No generalized "external tracker" trait layer refactor above `DocumentStore`. ClickUp gets its own parallel implementation, same as GitHub's — this RFC does not touch `gh.rs`/`issue_body.rs`/`issue_cache.rs`/`issue_map.rs`. This is a distinct layer from the dispatch-registry refactor below: auth/transport/field-mapping stay bespoke per backend; only the *mechanism that selects a backend* changes.

## Design

**Store dispatch refactor (prerequisite).** `dispatch_for_type` (`store_dispatch.rs:1855`) is currently a closed match generic over 4 concrete client type params (one per existing backend: gh-issues, gh-milestones, gh-projects, git-ref) — adding ClickUp as a 5th would mean a 5th generic param plus editing every call site that constructs the dispatch (CLI commands, TUI). Past 2 concrete uses, that generic-param growth is the indirection dictum 6 says to add for. Before ClickUp lands: push each backend's client generic down into its own struct as a boxed trait object (`GithubIssuesStore` holds `Box<dyn GhClient>` instead of being generic over `G: GhClient`; same for the milestone/projects/git-ref stores) and turn `dispatch_for_type` into a non-generic registry lookup (`HashMap<StoreBackend, Box<dyn DocumentStore>>` or equivalent), built once at startup. `DocumentStore` itself needs no changes — 4 methods, no generic methods, already object-safe. `StoreBackend` stays a closed enum (config validation wants a known finite backend set); only its role changes, from "carries type info into a match" to "registry key." This is a pure refactor of the existing 4 backends — no behavior change, existing tests keep passing unmodified — and is scoped as a prerequisite story (Stories, story 0) so `ClickupTasksStore` is the *first* backend added the new way, not the thing that forces the old way to bend further.

**ClickupTasksStore.** Implements `DocumentStore` (`create`/`update`/`delete`/`set_provenance`), same shape as `GithubIssuesStore` post-refactor: a concrete (non-generic) struct holding `Box<dyn ClickupClient>` internally, registered in the dispatch registry under `StoreBackend::ClickupTasks`. `Store`'s read side is untouched — it just reads whatever `ClickupTasksStore` materializes to `.lazyspec/cache/<type>/<ID>.md`. `delete` archives the ClickUp task (`PUT /task/{id}` with `archived: true`), never hard-deletes — archived tasks drop out of `task_list` fetches so the doc leaves the cache on next sync, but the task stays recoverable in ClickUp.

**Status handling.** No fixed local lifecycle table. When a type is bound via `clickup_list_id`, fetch that List's status definitions from the ClickUp API and populate the type's effective lifecycle states from that set. No edges/gating locally — ClickUp already enforces its own status-transition rules, lazyspec doesn't duplicate that (same posture `ticket` already takes with its empty `lifecycle.states`/`edges`). Doc `status` is the raw ClickUp status string, verbatim, no mapping table.

**Config.** New `TypeDef` field `clickup_list_id: Option<String>` — per-type, analog to `github_label`. Each lazyspec type binds to one ClickUp List. No `[clickup] list_id` global equivalent to `[github].repo`.

**Auth.** New credential store outside the repo (global, never committed). New setup flow — `lazyspec setup clickup` — prompts for a personal API token, validates it against ClickUp's `/user` endpoint, then persists it. Unlike `gh`, which owns its own external credential store that lazyspec never touches, ClickUp has no CLI to piggyback on — this is lazyspec's first owned credential store, so where the token lands is lazyspec's responsibility.

The token is a bearer credential: full read/write as the token owner, no scope limit, no expiry. Compromise = account takeover. It must not sit in plaintext on disk where local processes, backups, or cloud-sync sweeps of `~` can lift it. Storage precedence:

- **Default — OS keychain, via the `keyring` crate** (macOS Keychain, Linux Secret Service / libsecret, Windows Credential Manager). Token encrypted at rest by the OS, unlocked by the login session. This is the ecosystem-norm choice for a Rust CLI holding a secret (dictum 5).
- **Fallback — plaintext file** `~/.lazyspec/credentials.toml` under `[clickup] api_token`, used only when no keychain backend is reachable (headless boxes, CI). This is an explicit, logged fallback, never a silent default. When the file path is used: create `~/.lazyspec/` with dir mode `0700` and the file with mode `0600`, enforced on write; on read, refuse (or warn loud and repair) a credential file whose perms are looser than `0600`.
- **Redaction everywhere.** The token never appears in logs, error messages, `Debug` output, or `--json`. Wrap it in a newtype whose `Debug`/`Display` prints a fixed mask, so an accidental `{:?}` can't leak it.

Credential store only for v1; no env-var fallback. Global, not per-repo; never committed.

**Transport.** Native HTTP client (reqwest) — lazyspec's first native HTTP client for a store backend. GitHub goes through the `gh` CLI shellout entirely (`gh.rs`); ClickUp has no equivalent CLI. Error handling classifies by real `reqwest::Error` variants and HTTP status codes ClickUp's API actually returns, not the fragile stderr-substring scraping `gh.rs` does. Known wart in the existing approach worth naming: `classify_gh_error`/`extract_http_status` (`gh.rs:1106-1148`) scans stderr for the literal substring `"http "` and parses the next token as a status with no validation — it was observed misparsing `x509: certificate signed by unknown authority` into a fake "HTTP 509" (the digits came from `x**509**`, not a real response). Don't repeat this pattern for ClickUp. Base URL `https://api.clickup.com/api/v2`. Client trait method → endpoint: `auth_status`→`GET /user`, list-status fetch→`GET /list/{id}`, `task_list`→`GET /list/{id}/task?page=N` (paginated, 100/page, `include_closed`/`subtasks` params), `task_create`→`POST /list/{id}/task`, `task_edit`→`PUT /task/{id}`, `task_archive`→`PUT /task/{id}` with `{"archived":true}` (`DELETE /task/{id}` is a hard delete and stays unused), `task_view`→`GET /task/{id}`, custom-field ops→`POST /task/{id}/field/{field_id}`.

**Rate limits.** ClickUp rate-limits *per token*: 100 req/min on Free/Unlimited/Business, 1000 on Business Plus, 10000 on Enterprise. Over-limit → HTTP 429 with `X-RateLimit-Limit`/`X-RateLimit-Remaining`/`X-RateLimit-Reset` (Unix epoch) headers. This is a real constraint for the read path — a full list fetch paginates at 100 tasks/page and each `task_view`/custom-field write is a separate request, so a large bound List can burn the budget. The reqwest client classifies 429 as a distinct retryable error and honors `X-RateLimit-Reset` for backoff; it does not silently spin into the limit. Personal tokens share the OAuth-token limit — no separate personal-token allowance.

**Field mapping.** `priority`/`estimate`/`due` map directly to ClickUp's native task fields — no HTML-comment-in-body hack like `issue_body.rs` needed for these three, since ClickUp has native equivalents GitHub issues lack. The field mapping is *not symmetric* between read and write, and the mapping layer must handle both directions explicitly:
- `priority`: read returns an object `{"priority":"normal","color":..,"id":"3","orderindex":"3"}`; write takes a bare integer `1=Urgent, 2=High, 3=Normal, 4=Low` (`null` clears). Parse object → emit int.
- `due_date`/`start_date`/`time_estimate`: read returns *strings* of epoch-ms (`"1748541600000"`); write accepts integers. Serde deser must accept string-or-int for every epoch/duration field.
- body: write field is `markdown_content` (takes precedence over plain `description`); read fields are `markdown_description` + `text_content`.
- custom task types: a task carries `custom_item_id` (e.g. `1018` = "Initiative"). Custom-field values are only persisted when the field is applicable to the task's `custom_item_id`, so `task_create` must send the bound List's `custom_item_id` when the List uses custom task types.

Any other attr with no native ClickUp field falls back to a ClickUp custom field (keyed by uuid), looked up by name/id via a new config field (exact shape TBD at implementation time, e.g. `clickup_custom_field_map`).

**Relations.** Round-trip through a ClickUp *text* custom field holding serialized lazyspec relation data — not a "relationship"-type custom field, and not ClickUp's native dependency/linked-task API. A relationship field's values are ClickUp task ids, which makes cross-store relations unrepresentable: the primary use case is linking a ClickUp task to a filesystem doc (`link <task> implements RFC-056`), and a filesystem RFC has no task id. A text field stores lazyspec doc IDs directly, so relation targets can live in any store. One mechanism for every relation type lazyspec needs to persist. Serialization reuses the YAML relations block `issue_body.rs` already embeds in GitHub issue bodies (`- implements: RFC-056` lines, `serialize`/`deserialize` shape) — same format, stored in the field value instead of an HTML comment, keeping the task body clean. Avoids generalizing the `github_native` mechanism — currently hardcoded to the literal field name and string-matched in `cli/link.rs`'s `apply_native_milestone`/`apply_native_membership` — into a parallel `clickup_native` path for a payoff that's marginal at this scope. Constraints from the live API:
- The text custom field must be *pre-created* in the bound List (text fields are available on the free plan, unlike relationship fields). `setup`/config validation should surface a clear error when the configured field id is absent, not fail mid-write.
- Set-value payload is `POST /task/{id}/field/{field_id}` with `{"value":"<serialized relations block>"}` — a full replace, so the store serializes the doc's complete relation set on every write; no add/rem diffing.
- Update touches one custom field per request (no batch on `PUT /task`; only `task_create` can inline multiple `custom_fields`).

**Caching/id-mapping.** New `TaskMap` mirroring `IssueMap`'s shape (external task id, `updated_at` for optimistic-lock, any node id ClickUp exposes), persisted to `.lazyspec/task-map.json`. `TaskMap.updated_at` maps to ClickUp's `date_updated` field (returned as an epoch-ms *string*, e.g. `"1774587145901"`) — that is the timestamp the optimistic-lock compares. Reuses `CacheLock` and `write_cache_file`/`write_cache_parent`/`write_cache_child` (`store_dispatch.rs`) unchanged for cache freshness and file writing under `.lazyspec/cache/<type>/`.

## Interfaces
- `DocumentStoreRegistry` @draft — replaces `dispatch_for_type`'s generic signature with a non-generic `StoreBackend -> &mut dyn DocumentStore` lookup (`store_dispatch.rs`)
- `GithubIssuesStore`/`GithubMilestonesStore`/`GithubProjectsStore`/`GitRefStore` @draft (refactor) — each becomes non-generic, holding `Box<dyn GhClient>` / `Box<dyn GitRefClient>` / etc. internally instead of a generic client type param
- `StoreBackend::ClickupTasks` @draft (`config.rs`)
- `TypeDef.clickup_list_id: Option<String>` @draft
- `ClickupClient` trait @draft — `auth_status`/`task_list`/`task_view`/`task_create`/`task_edit`/`task_archive`/custom-field ops — reqwest-backed real impl + fake impl for tests, mirroring the `GhCli`/fake split
- `ClickupTasksStore` @draft, non-generic, holding `Box<dyn ClickupClient>`, implementing `DocumentStore`
- `TaskMap` @draft (`.lazyspec/task-map.json`)
- CLI: `lazyspec setup clickup` @draft — token prompt + validation + credential write
- Credential store @draft — keychain-primary via the `keyring` crate; plaintext-file fallback `~/.lazyspec/credentials.toml` (`[clickup] api_token`, dir `0700`/file `0600`) only when no keychain backend is reachable. New `keyring` dependency.
- Token newtype @draft — masks `Debug`/`Display` so the token can't leak into logs/errors/`--json`.
- `TypeDef.clickup_custom_field_map: Option<HashMap<String, String>>` @draft — attr/relation name to ClickUp custom field id, for anything with no native ClickUp field

## Decisions (ADRs to emit)
- `dispatch_for_type` moves from a closed generic match to a non-generic registry lookup; each backend's client generic moves into its own struct as a boxed trait object.
- ClickUp store binds per-type to one ClickUp List, not a global `list_id`.
- ClickUp task status is unmediated by a local lifecycle table; type lifecycle is derived from the bound List's status set at sync time.
- ClickUp relations round-trip as serialized lazyspec relation data (the `issue_body.rs` YAML relations-block format) in a *text* custom field — not a relationship-type field (whose values are task ids only, blocking cross-store targets) and not the native dependency/linked-task API. Writes are full-replace of the field value.
- `DocumentStore::delete` on a ClickUp-backed doc archives the task, never hard-deletes.
- ClickUp credentials live in a global (not per-repo) store — no external CLI-managed store to piggyback on, unlike `gh`. The token is stored in the OS keychain (via `keyring`) by default; a plaintext `~/.lazyspec/credentials.toml` (`0600`) is an explicit, logged fallback only where no keychain backend exists (headless/CI). The token is redacted in all logs/errors/`--json`.

## Stories
Sequenced, dependencies noted:
0. Store dispatch refactor — push each existing backend's client generic (`GhClient`, git-ref client, milestone client) into its own struct as a boxed trait object; replace `dispatch_for_type`'s generic match with a non-generic registry lookup. Pure refactor of the 4 existing backends, no behavior change, existing tests unmodified. Folded into story 4's story doc as enabler acceptance criteria, delivered as its own iteration reviewed before any ClickUp store code. Blocks the store-integration stories (3–6), not the client/auth stories (1–2) — ClickUp is the first backend added via the registry, not the 5th generic param.
1. ClickUp API client — `ClickupClient` trait, reqwest impl, fake impl. Auth/token validation only, no store integration.
2. `lazyspec setup clickup` — token capture/validation against story 1's client, then keychain-primary / plaintext-file-fallback storage with `0600` perms and token redaction. Blocked by 1.
3. `StoreBackend::ClickupTasks` variant + config plumbing (`clickup_list_id` field, `config_write` round-trip) + registry wiring. Blocked by 0, 1.
4. `ClickupTasksStore` read path — fetch tasks for a bound list, map to `DocMeta` (status = raw ClickUp status, `priority`/`estimate`/`due` from native fields), write cache files (`TaskMap` + `write_cache_file` reuse), populate type lifecycle from the List's status set. `lazyspec fetch` works end-to-end read-only after this. Blocked by 2, 3.
5. `ClickupTasksStore` write path — create/update/delete (delete = archive, never hard-delete) against the ClickUp API, optimistic-lock via `TaskMap.updated_at`, status transitions push to ClickUp. `lazyspec create`/`update`/`advance` work end-to-end after this. Blocked by 4.
6. Relations via custom field — read+write path for lazyspec relation types, serialized into a configured ClickUp text custom field. Blocked by 4 (needs doc-id resolution from the read path).

## Risks and tradeoffs
- **Dispatch refactor touches 4 working backends before any ClickUp code exists.** Story 0 changes `GithubIssuesStore`/`GithubMilestonesStore`/`GithubProjectsStore`/`GitRefStore` internals (generic param to boxed trait object) with a straight port, no new tests needed beyond existing coverage — but any behavior change there is a regression in production paths unrelated to ClickUp. Mitigate by keeping story 0 a pure mechanical port in its own reviewed iteration, landed before any ClickUp store code touches the dispatch path.
- **Text-custom-field relations** are cheaper to build than native dependency wiring and are the only shape that can hold cross-store targets, but in ClickUp's UI the relation is an opaque serialized blob in a field — no native dependency arrows/visualization. A future RFC can add `clickup_native` relations for ClickUp↔ClickUp links if that visibility becomes a real ask.
- **Personal API token** means ClickUp-side actions execute as the token owner's ClickUp user — audit trail attributes changes to whoever ran `setup`, not to individual agents/users. Same limitation the `gh`-token approach already carries in a shared environment; not new here.
- **Plaintext file fallback is unencrypted at rest.** When no keychain backend is reachable (headless/CI), the token lands in `~/.lazyspec/credentials.toml` protected only by `0600` perms — readable by the user's own processes and any backup/cloud-sync of `~`. This is a deliberate v1 tradeoff for environments without a secret service; the keychain default avoids it on developer machines. A passphrase-encrypted file is the escalation if the fallback's exposure becomes a real ask.
- **`keyring` adds a native, per-OS dependency.** Behavior differs across macOS/Linux/Windows secret backends, and headless environments may have none (hence the file fallback). New dependency weight on top of reqwest's. Mitigated by `keyring` being the ecosystem-standard crate for this (dictum 5).
- **No List/Folder document type yet** — any epic/grouping structure lives in ClickUp only, invisible to lazyspec's relation graph until a future RFC adds it.
- **reqwest as lazyspec's first native HTTP dependency** — new dependency weight and TLS-backend/cert-store surface the `gh`-CLI-shellout approach never had to carry. Mitigated by reqwest being the mature, default choice for Rust CLI HTTP clients (dictum 5: follow ecosystem norms).
