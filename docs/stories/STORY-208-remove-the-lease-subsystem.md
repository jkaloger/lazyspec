---
title: Remove the lease subsystem
type: story
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- implements: RFC-061
- related-to: AUDIT-018
---

## Value

As a lazyspec user, my doc writes are not gated on a distributed lock, and the CLI surface matches the doc-tool scope. Executes RFC-061 (full removal, scope defined there).

## Acceptance Criteria

- AC1: `claim`, `release`, `leases`, `heartbeat` subcommands gone; invoking them yields clap's standard unknown-subcommand error.
- AC2: `create`, `update`, and status transitions run no lease gate (`check_lease_gate*` deleted with call sites).
- AC3: a config carrying a stray `[coordination]` block still parses (ignored); regression test exists.
- AC4: `git_ref.rs::push_ref_with_lease` and the `GitRefOps` trait untouched (RFC-061 carve-out).
- AC5: no lease/coordination reference remains in TUI, web view, README, or templates; CHANGELOG notes the breaking CLI change and the orphaned-refs prune one-liner.
- AC6: README command table and TUI keybinding table match the shipped surface (AUDIT-018 F8 — sync while stripping lease docs).

## Notes

Scope, motivation, migration: RFC-061. Findings: AUDIT-018 F6 (+ moot lease findings list).

