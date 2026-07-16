---
title: Git-ref stores are live by default
type: story
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- related-to: AUDIT-019
---

## Value

As a lazyspec user with a git-ref store, mutations reach the configured remote automatically and IDs never collide across clones — the same live semantics every other remote backend already has (AUDIT-019).

## Context

AUDIT-019 findings: no mutation pushes (F1); no remote on `GitRefStore`, fetch hardcodes `origin` (F2); local-only number allocation collides across clones (F3); TUI edit paths bypass the store (F4); offline semantics undefined (F5); make unconditionally live, no toggle (F6).

## Acceptance Criteria

- AC1: one config source of truth for the git-ref remote (default `origin`); `GitRefStore` carries it; `main.rs` fetch stops hardcoding `origin`.
- AC2: every store mutation (create/update/set_provenance/delete/sync_tags) pushes: `push_ref_with_lease` after CAS updates, `delete_remote_ref` on delete; remote rejection surfaces as the existing conflict error.
- AC3: create allocates numbers safely across clones (fetch-before-allocate or reservation-style push-retry).
- AC4: TUI body-edit and link-edit paths go through the same push-enabled store methods.
- AC5: unreachable remote has one defined, tested behaviour across all mutations, and README's 'no automatic remote push' section is rewritten to match.

## Out of scope

A live on/off toggle (add only if opt-out is requested); changing the `refs/lazyspec/` namespace.
