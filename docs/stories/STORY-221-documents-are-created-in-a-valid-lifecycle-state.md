---
title: Documents are created in a valid lifecycle state
type: story
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- related-to: BUG-002
---

## Value

As a lazyspec user with custom lifecycles, `create` seeds the type's first lifecycle state — docs are never born outside their lifecycle (BUG-002).

## Acceptance Criteria

- AC1: `create` seeds `lifecycle.states[0]` for the type (still `draft` for default lifecycles — no behaviour change there).
- AC2: `fix` flags a status not in the type's lifecycle and offers `states[0]` as a field fix.
- AC3: regression test on the shipped `bug` type: created doc starts at `reported` and `update --status triaged` works.
- AC4: applies across stores (create path is store-agnostic).

## Out of scope

Migrating existing out-of-lifecycle docs automatically.
