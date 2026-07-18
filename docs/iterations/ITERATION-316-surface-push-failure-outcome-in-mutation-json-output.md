---
title: Surface push-failure outcome in mutation JSON output
type: iteration
status: accepted
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-211
- related-to: BUG-006
---

## Objective

git-ref store push outcome returned from store to CLI + surfaced in every mutation JSON (`synced: false` / `warnings: [...]`), not stderr-only. Fixes BUG-006.

## Satisfies

BUG-006 (defect under STORY-211 machine-readable mutation output).

## Context

- Layers: engine (src/engine/git_ref_store.rs) returns outcome; CLI (src/cli) serializes it. TUI unaffected but must still compile against changed store signatures.
- Bug: `push_failure_warning` (src/engine/git_ref_store.rs:47) emitted via `eprintln!` at src/engine/git_ref_store.rs:140 and :324 → nothing reaches CLI JSON. `--json` mutation stdout shows clean success while doc saved local-only.
- Applies to create/update/link (and advance) on pushing stores. Non-pushing stores → no warning / `synced: true` default.

## Tasks

1. Engine: change pushing mutation ops to RETURN push outcome (e.g. enum synced vs local-only-with-warning) instead of `eprintln!` at git_ref_store.rs:140 + :324. Keep `push_failure_warning` text as the warning payload.
2. Thread outcome up through store mutation return types to CLI create/update/link/advance handlers.
3. CLI: include in every mutation `--json` output — `"synced": false` plus `"warnings": [<push_failure_warning text>]` on failure; `"synced": true` / omitted warnings on success. Keep human (non-json) stderr warning behaviour.
4. Tests: unit — pushing store w/ unreachable remote → mutation returns local-only outcome; CLI JSON contains `synced:false` + warnings. Success path → `synced:true`, no warnings. Non-pushing store unaffected.

## Out of scope

- Retry/auto-repush logic.
- Changing warning wording.
- Non-mutating commands.

## Principles/conventions

- CLAUDE.md: engine has no I/O assumptions — return outcome, do not print from engine. CLI owns serialization. Update TUI call sites to compile; mirror any user-facing signal.

## Verification

git-ref type + unreachable remote: `create <type> x --json` stdout has `"synced": false`. `cargo test`.
