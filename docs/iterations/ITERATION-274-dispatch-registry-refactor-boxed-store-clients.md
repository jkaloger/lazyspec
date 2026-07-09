---
title: 'Dispatch registry refactor: boxed store clients'
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-198
- blocks: ITERATION-276
---

Objective: swap dispatch_for_type generic match for a non-generic StoreBackend registry; each backend holds a boxed trait-object client.

Refs: RFC-056 Design section Store dispatch refactor; store_dispatch.rs:1855 dispatch_for_type; backends GithubIssuesStore, GithubMilestonesStore, GithubProjectsStore, GitRefStore; DocumentStore trait (4 methods, object-safe).

Satisfies: STORY-198 enabler ACs (registry lookup + boxed client).

Tasks:
1. Each backend struct drops its generic client param, holds a boxed client (Box dyn GhClient / Box dyn GitRefClient) internally.
2. dispatch_for_type becomes a non-generic registry keyed by StoreBackend, built once at startup.
3. Fix call sites that constructed the generic dispatch (CLI commands, TUI).
4. Existing backend tests stay unmodified and green.

Out of scope: no ClickupTasks backend; no ClickupClient; zero behavior change; DocumentStore trait untouched.

Principles: pure mechanical port (RFC risk note demands story 0 land + review independently). Project CLAUDE conventions.

AC:
- Given the refactor is done, when cargo test runs, then existing backend tests pass unmodified.
- Given each backend, then it holds a boxed trait-object client and has no generic client param.
- Given dispatch_for_type, then it is a non-generic registry lookup keyed by StoreBackend and the generic match is gone.
