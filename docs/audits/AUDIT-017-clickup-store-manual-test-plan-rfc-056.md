---
title: ClickUp store manual test plan (RFC-056)
type: audit
status: draft
author: jkaloger
date: 2026-07-08
tags: []
related:
- related-to: RFC-056
---

## Intent

Manual, end-to-end verification of the ClickUp store (RFC-056) against a **real ClickUp workspace**. Automated tests cover the pure logic (mapping, classification, gating, optimistic-lock arithmetic) with a `FakeClickupClient`; they cannot exercise the real HTTP transport, the OS keychain, actual task round-trips, or ClickUp-side state. This plan covers exactly that gap.

Execute each scenario in order (later scenarios depend on earlier setup). Tick the checkbox and record the result. Run the binary with `cargo run --` (or a release build); all commands support `--json`.

## Prerequisites

- [ ] A ClickUp account with a workspace you can freely mutate (use a throwaway Space/List — the write tests create, edit, advance, archive real tasks).
- [ ] A **personal API token** (`pk_...`): ClickUp → Settings → Apps → API Token.
- [ ] The numeric **List ID** of a test List (open the List in the browser; the id is in the URL, or via `GET /list`). Note its **status set** (e.g. `to do`, `in progress`, `complete`).
- [ ] One **text-type custom field** on that List for relations, and its **field UUID** (`GET /list/{id}/field`). Not a "relationship"-type field — a plain text field (RFC-056 stores lazyspec doc IDs directly).
- [ ] A lazyspec project to test in (`lazyspec init` in a scratch dir).
- [ ] A second machine or a fresh checkout/login is helpful for the concurrency (optimistic-lock) test, but not required.

## Known issue to be aware of

The unit test `engine::clickup::tests::transport_failure_never_becomes_an_http_status` is **non-hermetic**: it opens a real TCP connection to a TEST-NET-1 address (`192.0.2.1:81`) expecting a transport-layer failure. On a normal dev machine that address is unroutable and the test passes; inside a network sandbox/proxy it can be answered with an HTTP 403 and the test fails. This is a pre-existing test-quality wart (from ITERATION-268), **not** a product defect. Worth a follow-up to make it hermetic (inject the transport). It does not affect any manual scenario below.

---

## STORY-197 — Auth & credential storage

### 197.1 Valid token → keychain (happy path)
- [ ] Run `lazyspec setup clickup` and paste a valid `pk_` token (or `--token pk_...`).
- [ ] **Expect:** success message reporting the token was stored **in the OS keychain**; it validated against `GET /user` (your ClickUp identity resolved). No plaintext-file note on a machine with a working keychain.
- [ ] **Verify no leak:** the token must NOT appear in stdout, `--json` output, or any log line. `--json` reports only `ok`/user identity/storage location.

### 197.2 Invalid token → nothing written
- [ ] Run `lazyspec setup clickup --token pk_definitely_invalid_000`.
- [ ] **Expect:** a clear invalid-token error (from a 401/403), and **no** credential written. If a valid credential already existed, it is left byte-for-byte intact.

### 197.3 File fallback (no keychain backend)
- [ ] On a headless/Linux box with no Secret Service running (or otherwise no reachable keychain), run `lazyspec setup clickup --token pk_valid...`.
- [ ] **Expect:** a **loud, explicit** log that it fell back to the plaintext file `~/.lazyspec/credentials.toml`; the fallback is never silent.
- [ ] **Verify perms:** `ls -ld ~/.lazyspec` → `0700`; `ls -l ~/.lazyspec/credentials.toml` → `0600`. Loosen the file perms manually, re-run a read, and confirm it warns and repairs to `0600`.

### 197.4 Global, not repo-local
- [ ] From inside a git repo that has no local credential file, run a command that needs the token (e.g. `lazyspec fetch` for a clickup type).
- [ ] **Expect:** it authenticates from the global `~/.lazyspec` (keychain/file), never from a repo-local file.

---

## STORY-198 — Read path (fetch tasks as read-only docs)

Setup:
- [ ] Configure a `clickup-tasks` type bound to your List. In `.lazyspec.toml` add a type with `store = "clickup-tasks"` and `clickup_list_id = "<LIST_ID>"` (use `lazyspec config`/`type add --store clickup-tasks` where available; the list id is set in TOML).
- [ ] Create 2–3 tasks directly in the ClickUp UI with varied status, priority, due date, and a description.

### 198.1 Fetch materializes tasks
- [ ] Run `lazyspec fetch` (or the type-scoped fetch).
- [ ] **Expect:** each ClickUp task becomes a read-only cache doc under `.lazyspec/cache/<type>/`. `lazyspec list <type> --json` shows them.
- [ ] **Verify mapping:** `lazyspec show <TASK-id> --json` → title = task name; `status` = the **raw** ClickUp status string (verbatim); `priority`/`due`/`estimate` present as attributes; body = the task's markdown description.

### 198.2 Lifecycle derived from the List status set
- [ ] After fetch, inspect the type's lifecycle: `lazyspec config show --json` (the type's `lifecycle.states`).
- [ ] **Expect:** states = the List's statuses in ClickUp workflow (`orderindex`) order; **no edges** (ClickUp owns transitions). Re-order/rename a status in ClickUp, re-fetch, and confirm the states update. Confirm `.lazyspec.toml` decor/comments survive the rewrite and it only rewrites on an actual change.

### 198.3 Missing token / missing list id
- [ ] Remove the credential (or point at an unconfigured type) and `lazyspec fetch`.
- [ ] **Expect:** a clear error telling you to run `lazyspec setup clickup` (token missing) or naming the missing `clickup_list_id`.

### 198.4 Removal reconciliation
- [ ] Delete (or move out of the List) a task in ClickUp, then re-fetch.
- [ ] **Expect:** its cache doc and task-map entry disappear.

### 198.5 Pagination
- [ ] If feasible, populate the List with >100 tasks (ClickUp pages at 100).
- [ ] **Expect:** all tasks materialize (paged fetch until the last page), not just the first 100.

---

## STORY-199 — Write-through (create / update / advance / delete)

### 199.1 Create posts a task
- [ ] `lazyspec create <clickup-type> "Test task from lazyspec"` with a body.
- [ ] **Expect:** a new task appears in the bound ClickUp List (check the UI) with that name/description; status is the **List default** (lazyspec omits status on create). Locally the new doc is materialized into cache from ClickUp's response and recorded in the task-map.

### 199.2 Update round-trips native fields
- [ ] `lazyspec update <TASK-id>` changing title, body, and a native attribute (`--attr priority=high`, `--attr due=<epoch-ms>`, `--attr estimate=<ms>`).
- [ ] **Expect:** the changes appear on the task in ClickUp; re-reading the doc reflects the new values (cache re-materialized from the PUT response). Untouched fields are NOT blanked (partial edit).

### 199.3 Advance pushes status
- [ ] `lazyspec update <TASK-id> --status "in progress"` (any status in the List set).
- [ ] **Expect:** the task's status changes in ClickUp; the cache reflects it. Because the clickup lifecycle has no local edges, the transition is accepted without an edge check (ClickUp validates). Try an obviously-bogus status string and confirm ClickUp rejects it (surfaced as an error), not a local gate.

### 199.4 Optimistic lock rejects a stale write
- [ ] `lazyspec fetch` to establish a fresh baseline. Then, **in the ClickUp UI**, edit that same task (e.g. change its title) so its `date_updated` advances.
- [ ] Without re-fetching, run `lazyspec update <TASK-id> --attr priority=urgent`.
- [ ] **Expect:** a **conflict error** ("changed on ClickUp since your last fetch; run `lazyspec fetch` and retry") and **no** write performed (verify the UI still shows your manual edit, priority unchanged).
- [ ] Then `lazyspec fetch` and retry the update → now it succeeds.

### 199.5 Delete archives (does not hard-delete)
- [ ] `lazyspec delete <TASK-id>`.
- [ ] **Expect:** the task is **archived** in ClickUp (visible under archived tasks, not permanently gone). The local cache doc/task-map entry remain until the next `fetch`, which reconciles the disappearance (the archived task drops out of the List fetch).

---

## STORY-200 — Relations round-trip

Setup:
- [ ] Add a `clickup_custom_field_map` to the clickup type with the reserved `relations` key mapping to your text custom field UUID: `relations = "<FIELD_UUID>"`.

### 200.1 Write relations via link
- [ ] `lazyspec link <TASK-id> <SOME-TARGET-id> --type implements` (target may be a filesystem doc, e.g. an RFC — that's the cross-store case the design exists for).
- [ ] **Expect:** the configured text custom field on the ClickUp task now holds a serialized YAML relations block (`- implements: <SOME-TARGET-id>`), storing the lazyspec **doc id** directly. Check the field value in the ClickUp UI.

### 200.2 Read decodes relations
- [ ] From a clean cache (or another checkout), `lazyspec fetch`, then `lazyspec context <TASK-id> --json` / `lazyspec show <TASK-id> --json`.
- [ ] **Expect:** the `related` frontmatter shows `implements → <SOME-TARGET-id>`, decoded from the custom field.

### 200.3 Round-trip integrity
- [ ] Confirm 200.1's write and 200.2's read agree: the relation you wrote is the relation you read back, same type and target.

### 200.4 Unlink / full-replace
- [ ] Add a second relation (`lazyspec link <TASK-id> <OTHER-id> --type blocks`), confirm both appear in the field. Then `lazyspec unlink <TASK-id> <SOME-TARGET-id>`.
- [ ] **Expect:** the field is fully rewritten with only the remaining relation (`blocks`); removing the last relation clears the field to empty.

### 200.5 Missing config guardrails
- [ ] Temporarily remove the `relations` entry from the field map and try `lazyspec link` on a clickup task.
- [ ] **Expect:** a clear error (no `relations` field configured), no partial write.

---

## Cross-cutting checks

- [ ] **Rate limiting:** if you hammer fetch/writes enough to hit ClickUp's 100 req/min per-token budget, a `429` is handled as a rate-limit backoff (respecting `X-RateLimit-Reset`), not a crash or a bogus error.
- [ ] **`--json` everywhere:** every command used above produces valid, parseable JSON with `--json`.
- [ ] **Other backends unaffected:** run an existing github-issues or filesystem workflow (fetch, create, `update --status` across a lifecycle edge) and confirm the store-dispatch refactor (ITERATION-274) did not change their behavior — edge-gated status transitions are still enforced for non-clickup types.

