---
title: tui editor save pushes clickup task body
type: iteration
status: complete
author: Jack Kaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-199
---

## Objective

Editing a `clickup-tasks` doc in the TUI external editor pushes the edited body back to ClickUp — parity with github-issues/git-ref.

## Context

- Story: STORY-199 (write-through). RFC-056 §Field mapping.
- Root cause: editor-exit block (`src/tui/infra/event_loop.rs:694-736`) pushes back for only two backends — `try_push_gh_edit` (`:63-97`, gated `StoreBackend::GithubIssues`) and `try_push_git_ref_edit` (`:99-124`, gated `GitRef`). No ClickUp arm. A clickup-backed edit gets only the local reload (`:703`); never PUT to ClickUp.
- Write capability exists, unused by this path: `ClickupTasksStore::update` (`src/engine/store_dispatch.rs:226`) maps `("body", ...)` → `markdown_content`, runs optimistic lock, PUTs. CLI already wires clickup push-back via `push_if_clickup_backed` (`src/cli/link.rs:519`) — gap is TUI-only.
- Store build refs: `ClickupTasksStore` struct + `token` field (`store_dispatch.rs:99-108`); token load `LayeredCredentialStore::global().load_clickup_token()` + `ClickupHttpClient` build (poll arm `refresh_clickup_cache` `event_loop.rs:148-186` and `main.rs` show the pattern).

## Satisfies

STORY-199 write-through — extends update push-back from CLI to TUI editor save. No new AC; closes CLI-vs-TUI gap for ClickUp.

## Tasks

1. Add `try_push_clickup_edit(root, relative, config) -> Result<(), String>` mirroring `try_push_gh_edit` (`event_loop.rs:63-97`): read file, `split_frontmatter`, `Store::load`, resolve doc id + type_def; early-return `Ok(())` unless `type_def.store == StoreBackend::ClickupTasks`.
2. In it: load token via `LayeredCredentialStore::global().load_clickup_token()` — absent → return `Err` one-line warning (mirror `NO_TOKEN`). Build `ClickupHttpClient`, construct `ClickupTasksStore { client, root, config, token: Some(..) }`, call `update(type_def, &doc_id, &[("body", body_trimmed)])`.
3. Wire into editor-exit block (`event_loop.rs:721-733` sibling of the git-ref thread): spawn thread, send result on existing `AppEvent::GhPushResult` (reuse; no new event). Unconditional spawn like the git-ref arm — gating is internal.
4. Test: `try_push_clickup_edit` returns `Ok(())` (no-op) for a non-clickup type; pushes for clickup type via `FakeClickupClient` at the seam (`clickup.rs:619`). Token-absent → `Err`, no client call.

## Out of scope

- Registry-dispatch refactor of all three push helpers → deferred (see [[ITERATION-274]] dispatch registry); this slice adds the 3rd bespoke helper matching existing pattern.
- `create`/`advance`/status push from editor — body update only.
- Optimistic-lock conflict UX beyond surfacing the existing error string.

## Principles

- Layering: token/network I/O in TUI arm, not engine `Store::load` (dictum 3).
- Traits at seam: `ClickupClient` fake in tests (dictum 4).

## Verification

Edit a clickup-tasks doc body in nvim from the TUI, save, quit → change lands on ClickUp within the push (confirm in ClickUp UI). Stale local baseline → conflict error surfaced, no clobber.

