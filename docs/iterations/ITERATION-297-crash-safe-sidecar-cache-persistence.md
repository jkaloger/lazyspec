---
title: Crash-safe sidecar cache persistence
type: iteration
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-209
---

## Objective

Crash-safe sidecar persistence: atomic writes, corrupt-lock hard error, fetch staging swap.

## Satisfies

STORY-209 AC1, AC2, AC3.

## Context

- Findings: AUDIT-018 C1, C4, C5
- Touch: `src/engine/cache_lock.rs` (`:47` save), `src/engine/issue_cache.rs` (`:62-64` load_lock, `:404-411` fetch_all), `src/engine/clickup_cache.rs` (`:72-81`), `src/engine/issue_map.rs` (`:62`), `src/engine/task_map.rs` (`:53`), `src/engine/sync.rs` (`:426`), `src/engine/store_dispatch.rs` (`:2070`)
- Convention: DICTUM-002 (trait seams), DICTUM-004 (testing)

## Tasks

1. Atomic write helper (temp file same dir + rename) in engine fs module; route all 7 sidecar save sites through it.
2. `load_lock`: absent → default; present-but-unparseable → hard error. Never persist defaulted lock. Unit tests both paths.
3. `fetch_all` (both caches): write to staging dir, swap on success; failure leaves old cache. Test: injected write failure → old docs intact.
4. `cargo test`.

## Out of scope

Advisory interprocess lock (STORY-209 out-of-scope). Cache schema changes.

## Verification

Truncate `cache.lock` mid-content → next op errors, file unchanged. Kill fetch midway (fault injection) → previous cache readable.

