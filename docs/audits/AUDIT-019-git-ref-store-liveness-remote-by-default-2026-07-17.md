---
title: "Git-ref store liveness: remote-by-default (2026-07-17)"
type: audit
status: accepted
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Scope

Git-ref store liveness review, 2026-07-17. Criterion: git-ref stores are meant to be *live* by default — mutations reach the remote configured in `.lazyspec.toml`, reads reflect it — rather than writing only local refs. Compared against the other remote backends (github-issues, clickup-tasks), which are live by construction, and against the reservation code, which already pushes refs to a configured remote.

## Findings

### High

**F1. No git-ref mutation ever pushes — writes are local-only**
- Location: `src/engine/git_ref_store.rs:100-355` (create/update/set_provenance/delete/sync_tags)
- Every mutation uses local plumbing only (`create_ref_commit`/`create_commit` + `update_ref`/`delete_ref` → local `git update-ref`). The remote ops already exist on the client trait (`GitRefOps::fetch_refs/push_ref/push_ref_with_lease/delete_remote_ref`, `src/engine/git_ref.rs:216-278`) and are unit-tested, but no production call site invokes them. Contrast: github-issues and clickup mutations call the remote inside every operation (`store_dispatch.rs`). README documents the local-only behaviour (`README.md:863-865`).
- Recommendation: push after each local CAS — `push_ref_with_lease(remote, refname, new_sha, Some(old_sha))` for update/provenance/tags, plain push for create, `delete_remote_ref` for delete — so remote rejection surfaces as the existing conflict error. Update README.

**F2. `GitRefStore` has no remote at all; fetch hardcodes `"origin"`**
- Location: `src/engine/git_ref_store.rs:32-37` (struct, no `remote` field); `src/main.rs:134-144` (fetch remote hardcoded `"origin"` at `:141`)
- Remote resolution is inconsistent across three paths: reads/fetch use a hardcoded `"origin"`, reservations use `ReservedConfig.remote` from config (`src/engine/config.rs:77-87`, default `"origin"`), and mutations use nothing. There is no `[git-ref]` config section and `TypeDef` has no remote field.
- Recommendation: one config source of truth for the git-ref remote (default `"origin"`), threaded through all four `GitRefStore` construction sites (`store_dispatch.rs:2359-2364`, `ops/create.rs:190-195`, `event_loop.rs:119-124`, `ops/link.rs` inline edit) and through `fetch::run`.

**F3. Number allocation reads local refs only — cross-clone ID collisions**
- Location: `src/engine/git_ref_store.rs:61-77` (`next_number_from_refs`)
- `create` allocates the next doc number from *local* refs without fetching; two clones can mint the same ID. The reservation code already solves this shape with push-retry-on-rejection (`src/engine/reservation.rs:214-266`), but git-ref stores never use it — `reserved_number` is hardcoded `None` at every construction site.
- Recommendation: fetch-before-allocate, or adopt the reservation push-retry loop so a colliding number is detected by a rejected push.

### Medium

**F4. TUI edit paths duplicate local-only git-ref writes**
- Location: `src/tui/infra/event_loop.rs:119-127` (`try_push_git_ref_edit` — despite the name, no push); `src/engine/ops/link.rs:~710-737` (`sync_git_ref_link_edit`, inline `GitCli` commit+CAS)
- Both bypass any future store-level liveness unless they go through the same push-enabled store methods.
- Recommendation: route both through `GitRefStore` so liveness lands in one place.

**F5. Offline/unreachable-remote semantics undefined**
- Location: n/a (design gap)
- Reservations detect unreachable remotes and hint a fallback (`reservation.rs:57-67`); a live git-ref store needs the same decision — hard-fail the mutation vs write-local-and-warn — consistent across create/update/delete.
- Recommendation: pick write-local-and-warn or hard-fail explicitly, document it, and cover it with a fake-client test.

### Info

**F6. No `live` toggle exists anywhere**
- Location: codebase-wide (no `live` flag/field)
- Other remote backends are unconditionally live; "live by default" for git-ref can mean simply making it unconditionally live too. A config toggle is only needed if opt-out is a requirement.
- Recommendation: make git-ref unconditionally live; skip the toggle until someone asks for opt-out (CONVENTION principle 6).

