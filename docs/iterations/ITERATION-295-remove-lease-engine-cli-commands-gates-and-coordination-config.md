---
title: Remove lease engine, CLI commands, gates, and coordination config
type: iteration
status: in-progress
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-208
- blocks: ITERATION-296
- blocks: ITERATION-300
---

## Objective

Delete lease subsystem: engine, CLI commands, write gates, `[coordination]` config.

## Satisfies

STORY-208 AC1, AC2, AC3, AC4. AC5/AC6 deferred → next iteration.

## Context

- Scope + carve-outs: RFC-061 ("What gets removed" / "Explicitly out of scope")
- Convention: CONVENTION principles 1, 6; DICTUM-001/004
- Touch: `src/engine/lease.rs` (delete), `src/cli/lease.rs` (delete), `src/cli.rs` (Claim/Release/Leases/Heartbeat variants), `src/main.rs` (dispatch + 3 `check_lease_gate*` call sites), `src/engine/config.rs` (`CoordinationConfig`, `default_coordination_lease_duration`), `src/cli/init.rs` (default `[coordination]` emission), `src/engine.rs`/`src/cli.rs` module decls, lease tests

## Tasks

1. Delete `src/cli/lease.rs` + `src/engine/lease.rs`; remove module decls.
2. Remove 4 clap variants + main.rs dispatch arms + 3 gate call sites.
3. Strip `CoordinationConfig` from config.rs + init.rs default output.
4. Add test: config w/ stray `[coordination]` block parses clean (RFC-061 Risks).
5. `cargo build && cargo test`. Fix fallout (imports, exhaustive matches).

## Out of scope

- TUI field, web, README, CHANGELOG → next iteration (STORY-208 AC5/AC6).
- `push_ref_with_lease`, `GitRefOps`, `git_ref_store.rs` — DO NOT TOUCH (RFC-061 carve-out).

## Verification

`grep -ri "lease\|coordination\|claim\|heartbeat" src/engine src/cli src/main.rs` → only `push_ref_with_lease`/git-ref CAS hits remain.

