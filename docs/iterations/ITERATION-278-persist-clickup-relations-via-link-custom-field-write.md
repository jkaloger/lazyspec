---
title: Persist ClickUp relations via link custom-field write
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-200
---

## Objective
`lazyspec link <task> <rel> <target>` on a ClickUp-backed doc persists the relation by writing the configured custom field through ClickupClient, round-tripping (re-fetch/decode yields it).

## Context
- Story+AC: STORY-200 (AC1)
- Design: RFC-056 §Relations, §Config
- Depends: read/decode + resolver from prior iter; ClickupClient trait (STORY-197) + ClickupTasksStore write path (STORY-199)
- Touch: cli/link.rs (route ClickUp-backed doc to custom-field persist), ClickupClient custom-field write op (reqwest impl + fake), encode using the resolver
- Live-API constraints (RFC-056 §Relations):
  - Field is a *text* custom field holding serialized lazyspec relation data (issue_body.rs YAML relations-block format) — not a relationship-type field (task-ids-only, can't hold cross-store targets like a filesystem RFC), not the native dep API.
  - `POST /task/{id}/field/{field_id}`, payload `{"value":"<serialized block>"}` — full replace; serialize the doc's complete relation set each write, no add/rem diffing.
  - Field must be pre-created in the bound List (text fields available on free plan). Missing/misconfigured field id -> a clear config error, not a mid-write failure.
  - One custom field per request (no batch on `PUT /task`).

## Satisfies
STORY-200 AC1.

## Tasks
1. link.rs: dispatch ClickUp-backed doc to a custom-field persist path (not apply_native_milestone/apply_native_membership, not native dep API).
2. add ClickupClient custom-field set op (reqwest real + fake), `POST /task/{id}/field/{field_id}` with `{"value":"<serialized>"}`, mirroring existing GhCli/fake split.
3. serialize the doc's complete relation set (issue_body.rs YAML relations-block format) into the configured text custom field via the resolver from the read iter; full-replace write, no add/rem diffing.
4. test-first w/ fake client: link implements RFC-056 -> fake records the custom-field set; decode (read iter) re-reads the relation.

## Out of scope
- read/decode + resolver (prior iter)
- ClickUp native dependency/linked-task API (RFC non-goal)
- non-native attr writes on general create/update (STORY-199 write path)
- generalizing github_native into clickup_native (RFC non-goal)

## Principles
- CLAUDE.md: dogfood cargo run, --json, keep tui/web/cli in sync
- RFC-056 decisions: one custom-field mechanism for every relation type
- RFC-056 §Transport: classify errors by real reqwest/HTTP status, not stderr substring scraping
- testing skill: test-first

## Verification
link then context --json on same ClickUp doc -> the just-linked relation resolves.

## Acceptance
Given a ClickUp-backed doc and clickup_custom_field_map mapping implements to a field id, When lazyspec link <task> implements RFC-056, Then the configured custom field is written via the ClickUp API (fake client records the set) and a subsequent decode resolves the implements relation.
Given the persist path, Then it uses the configured custom field, not ClickUp's native dependency/linked-task API.
