---
title: Git-ref remote config source of truth
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-218
- blocks: ITERATION-309
---

## Objective

One config source of truth for git-ref remote; `GitRefStore` carries it; fetch stops hardcoding `origin`.

## Satisfies

STORY-218 AC1. (AUDIT-019 F2)

## Context

- Struct no remote: `src/engine/git_ref_store.rs:32-37`
- Hardcode: `src/main.rs:141` (`"origin"` → `fetch::run`)
- Precedent: `ReservedConfig.remote` (`src/engine/config.rs:77-87`, default `origin`)
- Construction sites: `store_dispatch.rs:2359-2364`, `ops/create.rs:190-195`, `event_loop.rs:119-124`, `ops/link.rs:~715-737`

## Tasks

1. Config: git-ref remote field (e.g. `[git-ref] remote = "origin"`), default `origin`. Parse + `config --json` + `config_write` round-trip.
2. `GitRefStore.remote: String`; thread through all 4 construction sites.
3. `main.rs` fetch: resolve remote from config, drop literal.
4. Tests: config default, override honoured by fetch + store construction. `cargo test`.

## Out of scope

Pushing (next iteration). Reservation remote unification (keep `ReservedConfig.remote` separate for now).

## Verification

`cargo test`. `grep -n '"origin"' src/main.rs` → none.

