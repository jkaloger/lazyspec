---
title: "Scope lease gate to gitref-backed types"
type: iteration
status: accepted
author: "agent"
date: 2026-06-21
tags: []
related: []
---

## Problem

Lease gate fires for ALL doc types when `[coordination]` set. Gate = `check_lease_gate` (`src/cli/lease.rs:51-114`), called from `create`/`update`/`delete` (`src/main.rs:109,174,199`).

Leases protect shared-remote `gitref` backend. Filesystem docs = local files, no cross-agent write race on a remote ref. github-issues docs locked by GitHub, not git refs. Gating them = needless friction: single-agent local edit blocked until `lazyspec claim`.

Fix: gate iff resolved type `store == StoreBackend::GitRef`. Non-gitref types skip gate entirely.

## Scope

In: auto-gate on create/update/delete scoped by backend.
Out: `claim`/`release`/`extend`/`force-acquire` CLI cmds unchanged (still act on any type). Create-needs-any-lease semantics for gitref unchanged (separate concern, see Notes).

## Acceptance Criteria

**AC1 filesystem update/delete not gated**
Given coordination on, type `store=filesystem`, no lease held
When `lazyspec update <doc>` or `delete <doc>`
Then succeeds, no lease error.

**AC2 gitref update/delete still gated**
Given coordination on, type `store=git-ref`, doc not claimed by you
When `lazyspec update <doc>` or `delete <doc>`
Then bail `document is not claimed...`.

**AC3 filesystem create not gated**
Given coordination on, target type `store=filesystem`, no lease held
When `lazyspec create <type> ...`
Then succeeds, no lease error.

**AC4 gitref create still gated**
Given coordination on, target type `store=git-ref`, no lease held
When `lazyspec create <type> ...`
Then bail `no active lease...`.

**AC5 github-issues not gated**
Given coordination on, type `store=github-issues`, no lease held
When create/update/delete
Then succeeds, no lease error (non-gitref → skip).

**AC6 no-coordination unchanged**
Given no `[coordination]` section
When create/update/delete any type
Then succeeds (early return, regression guard).

## Test Plan

Unit tests, `src/cli/lease.rs` tests module. Use `_with` variants (inject `MockGitRefClient` + agent str) per trait-seam dictum. No I/O, deterministic.

Update helper `config_with_coordination` (`lease.rs:326`): make `rfc` type `store=GitRef`, keep `story` type `store=Filesystem` (default), add a `github-issues` type. Lets one helper cover gitref + filesystem + gh cases.

Existing tests using `Some("RFC-001")` (lines ~406-806): RFC now gitref → assertions unchanged (still gated). Verify each still passes.

Existing tests using `None` (create, any-lease: lines ~457,465,479,576,593): migrate to new create-gate fn with a gitref type name (`"rfc"`). Assertions unchanged.

New tests:
- `filesystem_doc_update_skips_gate` — `Some("STORY-001")`, no lease ref → `Ok`. (AC1)
- `gitref_doc_still_gated_when_unclaimed` — `Some("RFC-001")`, no lease ref → bail. (AC2)
- `gitref_doc_held_by_other_still_bails` — `Some("RFC-001")`, lease held by other agent → bail. (AC2)
- `filesystem_create_skips_gate` — create-gate `"story"`, no lease → `Ok`. (AC3)
- `gitref_create_requires_lease` — create-gate `"rfc"`, no lease → bail. (AC4)
- `github_issues_doc_skips_gate` — `Some("<GH-PREFIX>-001")`, no lease → `Ok`. (AC5)
- `no_coordination_skips_check` exists (AC6) — keep.

Tradeoff: helper change touches many existing tests. Alternative = second helper, but two near-identical configs drift. One helper w/ mixed backends is closer to real config. Accept the churn.

## Changes

**Task 1 — scope doc-specific gate (update/delete) by backend** (AC1, AC2, AC5)
File: `src/cli/lease.rs`.
- Import `StoreBackend` (and `TypeDef`) from `crate::engine::config`.
- Replace `resolve_doc_type` (returns `&str` name) with `resolve_type_def(config, doc_id) -> Result<&TypeDef>` (find by `doc_id.starts_with(&t.prefix)`).
- Change `check_lease_gate_with` signature: `doc_id: Option<&str>` → `doc_id: &str`. Remove `None` branch (moves to Task 2). Body: `extract_doc_id` → `resolve_type_def` → `if td.store != StoreBackend::GitRef { return Ok(()); }` → build refname with `td.name` → existing lease-held-by-you check.
- Change `check_lease_gate` signature `doc_id: Option<&str>` → `doc_id: &str`; keep early `coordination.is_none()` return; resolve agent; call `_with`.
Verify: `cargo build`.

**Task 2 — add create gate scoped by backend** (AC3, AC4, AC5)
File: `src/cli/lease.rs`.
- Add `check_lease_gate_for_create_with<R: GitRefOps>(root, config, doc_type: &str, git: &R, agent: &str) -> Result<()>`:
  - `coordination` None → `Ok`.
  - `config.type_by_name(doc_type)`; if None → `Ok` (create errors later w/ own msg); if `store != GitRef` → `Ok`.
  - else existing any-lease check (moved verbatim from old `None` branch: fetch `refs/lazyspec/leases/*`, list, match `lease.agent == agent`, bail `no active lease...` if none).
- Add `check_lease_gate_for_create(root, config, doc_type) -> Result<()>`: `coordination.is_none()` → `Ok`; `resolve_agent_id`; call `_with` with `GitCli`.
Verify: `cargo build`.

**Task 3 — wire call sites** (AC1-AC6)
File: `src/main.rs`.
- `:109` create: `check_lease_gate(&cwd, &config, None)?` → `check_lease_gate_for_create(&cwd, &config, &doc_type)?` (`doc_type` = Create arm CLI arg).
- `:174` update: `check_lease_gate(&cwd, &config, Some(&path))?` → `check_lease_gate(&cwd, &config, &path)?`.
- `:199` delete: same change as update.
Verify: `cargo build`.

**Task 4 — tests** (all AC)
File: `src/cli/lease.rs` tests module.
- Edit `config_with_coordination`: `rfc` → `store: StoreBackend::GitRef`; `story` stays `Filesystem`; add gh-issues type (e.g. name `bug`, prefix `BUG-`, `store: StoreBackend::GithubIssues`).
- Migrate `None` create tests → `check_lease_gate_for_create_with(..., "rfc", ...)`.
- Add new tests per Test Plan.
Verify: `cargo test lease` green. Then `cargo test` full suite.

## Notes

- Rule is "gitref-gated", title says "filesystem" — github-issues also non-gitref so also skips. AC5 pins this.
- `create` for gitref still requires holding *any* lease (old `None` semantics preserved). Flagged earlier as conceptually weak (new doc has no contended resource). Out of scope here; candidate follow-up: claim-on-create for gitref.
- `claim`/`release` still operate on filesystem ids if user runs them manually. Harmless (lease ref just unused). Not gating reads (`list`/`show`/`link`) — unchanged.
- `StoreBackend::default() == Filesystem` (`config.rs:114`), so types w/o explicit `store` are ungated.
