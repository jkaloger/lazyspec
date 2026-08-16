---
title: Hydra parity across watch, graph, web and write guards
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-08-17
tags: []
related:
- implements: STORY-252
- blocks: ITERATION-365
---

## Objective

Hydra documents behave correctly in the live TUI, the graph and the web view, and every write command refuses them.

## Satisfies

STORY-252 AC6, AC7, AC11. Depends on ITERATION-362 and ITERATION-363.

## Context

- Story + ACs: STORY-252
- Why ids break at the two path-derived sites, and the read-only rationale: RFC-066 §Identity, §Non-goals
- Touch: `src/engine/graph.rs:446`, `src/web/render.rs:316`, `src/engine/watch.rs`, `src/engine/store_dispatch.rs` (write paths), `src/engine/ops/link.rs`

## Tasks

1. Change `src/engine/graph.rs:446` and `src/web/render.rs:316` to resolve a document's id via the store rather than re-deriving it from the path stem. Audit the other `extract_id_from_name` call sites listed by `grep -rn extract_id_from_name src/` and fix any that a `.hydra/<lowercase-slug>.json` path would also break.
2. Extend the file watch to the configured hydra dir so a `hydra cut` during a live TUI session reloads the document.
3. Reject `create`, `update`, `delete`, `link`, `unlink`, `tag` and status moves targeting a hydra document, with an error naming the `hydra` command to use instead. Put the check at the store-dispatch seam so it cannot be bypassed per-command.
4. Add the hydra store section to the README: config snippet, read-only boundary, id scheme, and the fact that the `hydra` binary is not required.
5. Tests: id resolution for a lowercase-stemmed path; each write command refusing a hydra target and leaving the file byte-identical.

## Out of scope

- STORY-253 (linking and validation behaviour).
- Any lazyspec-side write path into `.hydra`, now or later — RFC-066 §Non-goals.

## Principles/conventions

`lazyspec convention` — CLI and TUI must not depend on each other; the read-only guard belongs at the engine seam both go through.

## Verification

With the TUI open on `HYDRA-HYDRA-STORE`, running `hydra reopen store-shape` in another terminal flips the document to `in-progress` without a restart, and `cargo run -- update HYDRA-HYDRA-STORE --status draft` fails without touching `.hydra`.

