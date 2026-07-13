---
title: "Remove residual orchestration: leases, claiming, heartbeat"
type: rfc
status: accepted
author: "unknown"
date: 2026-07-12
tags: []
related: []
---

## Summary

Remove the residual agent-orchestration subsystem from lazyspec: the document **lease** feature and its `claim`/`release`/`leases`/`heartbeat` CLI commands, the `[coordination]` config block, the lease gates on mutating commands, and every TUI/web reference to lease state. Lazyspec is a structured-markdown documentation tool (Principle 1); the lease machinery is the last surviving organ of the abandoned daemon/orchestration vision and no longer serves that function.

This RFC proposes **full removal**, not deprecation. The deprecate-then-remove path is recorded under Alternatives.

## Motivation

The orchestration/daemon direction (RFC-041 and kin) was abandoned. Lazyspec settled on being a simple, version-controlled markdown doc tool. The lease subsystem is dead weight left behind by that pivot:

- **Scope drift.** Distributed task coordination via git refs contradicts Principle 1 ("features are justified by how they serve producing/validating/serving structured markdown"). Leases coordinate *agents*, not documents.
- **Carrying cost.** ~2,100 lines across `engine/lease.rs` (1119) and `cli/lease.rs` (972), plus config, gate hooks, TUI form fields, and tests — surface that must be maintained, documented, and reasoned about on every change.
- **Friction on the hot path.** Every `create`, `update`, and status transition currently calls a lease gate (`check_lease_gate` / `check_lease_gate_for_create`). A doc tool should not gate a local file write on a distributed-lock check.
- **No live consumer.** No shipped workflow claims a lease before editing. The commands exist; nothing orchestrates with them.

## What gets removed

**CLI commands** (`cli.rs`, dispatch in `main.rs`):
- `Claim` (acquire lease, `--force` steal of expired lease)
- `Release` (release lease, admin `--holder` verify)
- `Leases` (list active leases)
- `Heartbeat` (extend expiry, `--if-stale` throttle)

**Engine + CLI modules:**
- `src/engine/lease.rs` in full (`Lease`, `LeaseEngine`, `parse_duration`, `lease_ref`/`lease_glob`, `refs/lazyspec/leases/*` scheme)
- `src/cli/lease.rs` in full (command runners + `check_lease_gate`, `check_lease_gate_for_create`)

**Gate hooks** (`main.rs`): the three `check_lease_gate*` call sites guarding create/update/advance.

**Config** (`engine/config.rs`, `cli/init.rs`): the `[coordination]` block — `CoordinationConfig`, `lease_duration` (and grace/retry/clock-skew fields), `default_coordination_lease_duration`, default emitted by `init`.

**TUI** (`tui/state/app.rs`, `tui/views/panels.rs`): `FieldPath::CoordinationLeaseDuration`, the `lease_duration` config-panel row and its edit path.

**Web** (`web/server.rs`): any surfacing of lease/coordination state (read-only view shows no lease data after removal).

**Tests:** lease unit/integration tests in the above modules; `[coordination]` fixtures in config tests.

## Explicitly out of scope (keep)

- **`git_ref.rs::push_ref_with_lease`** — this is git's `--force-with-lease` compare-and-swap primitive on `GitRefOps`, a generic atomic-ref-write. It is *named* "lease" but is not the orchestration feature. Keep it; it may back other git-ref uses (`git_ref_store.rs`).
- **The `GitRefOps` trait and git-ref store layer** generally, minus the lease-specific ref scheme.
- All document types, relationships, lifecycle, validation, stores.

## Migration & compatibility

- **Config:** `[coordination]` becomes an unknown block. Decide parse behavior: (a) silently ignore unknown top-level tables (forgiving), or (b) warn once. Recommend **ignore + one-line note in release notes** so existing configs don't error. No migration command needed — the block is inert once code is gone.
- **CLI:** removed subcommands return clap's standard unknown-subcommand error. Note in CHANGELOG; this is a breaking CLI change → minor/major bump per release-plz policy.
- **State:** any `refs/lazyspec/leases/*` refs left in repos are orphaned and harmless; document a one-liner to prune them (`git for-each-ref --format=... | xargs git update-ref -d`).
- **Docs/README:** strip lease/coordination command docs and config reference.

## Risks

- **Someone depends on it.** Low — no shipped workflow uses it. Mitigate by announcing in release notes and offering the pruning snippet.
- **Accidental over-removal of git-ref CAS.** Mitigate via the explicit out-of-scope carve-out above; the delivery iteration must not touch `push_ref_with_lease`.
- **Config parse regressions.** Mitigate: add a test that a config carrying a stray `[coordination]` block still parses.

## Alternatives considered

1. **Deprecate then remove.** Keep commands, print a deprecation warning for one release, remove next. Rejected: no user base to protect and no live consumer; a straight cut is cheaper and the abandoned-orchestration decision is already made.
2. **Keep behind a feature flag.** Rejected: violates Principle 6 (no indirection without two concrete uses) and Principle 1; parks dead code indefinitely.
3. **Repurpose leases for single-writer doc locking.** Rejected: not a demonstrated need; reintroducible later via a fresh RFC if a real multi-writer scenario appears.

## Delivery

This RFC is the proposal only. Actual code removal is a separate, human-initiated story + iteration downstream (crossing into `story` is a type boundary — not auto-created here).
