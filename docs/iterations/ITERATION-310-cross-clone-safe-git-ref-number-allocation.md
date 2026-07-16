---
title: Cross-clone-safe git-ref number allocation
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-218
---

## Objective

Git-ref doc numbers safe across clones — collision detected, never silent duplicate.

## Satisfies

STORY-218 AC3. (AUDIT-019 F3)

## Context

- Depends: push-on-mutation iteration
- Local-only allocation: `next_number_from_refs` (`src/engine/git_ref_store.rs:61-77`)
- Pattern to copy: reservation push-retry (`src/engine/reservation.rs:214-266` — push claims ref, rejected push → retry next number)
- `reserved_number` hardcoded `None` at all construction sites

## Tasks

1. `create`: push-retry loop — allocate from local refs, push new ref w/ lease (expect absent), rejection → fetch + retry next number (bounded retries).
2. Fallback when remote unreachable: local allocation + warning (consistent w/ push-on-mutation offline semantics).
3. Fake-client tests: collision → retry → next number; bounded retry exhaustion error. `cargo test`.

## Out of scope

Reservation-system unification for filesystem stores.

## Verification

`cargo test`. Fake-client collision test green.

