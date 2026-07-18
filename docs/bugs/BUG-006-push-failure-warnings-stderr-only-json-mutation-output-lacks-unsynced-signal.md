---
title: "Push-failure warnings stderr-only; --json mutation output lacks unsynced signal"
type: bug
status: triaged
author: "unknown"
date: 2026-07-18
tags: []
related: []
---

## Summary

When a git-ref store mutation saves locally but fails to push, the warning goes to stderr only. `--json` stdout output for the mutation carries no field indicating the doc is unsynced — agents consuming JSON see a clean success.

## Reproduction

1. git-ref-backed type with unreachable remote.
2. `lazyspec create <type> "x" --json` (or update/advance).
3. stderr: "warning: DOC-123 was saved locally but could not be pushed...". stdout JSON: normal success, no sync field.

## Expected

JSON mutation output signals local-only state, e.g. `"synced": false` or a `"warnings": [...]` array.

## Actual

`push_failure_warning` (src/engine/git_ref_store.rs:47) emitted via `eprintln!` at src/engine/git_ref_store.rs:140 and :324; nothing reaches the CLI layer's JSON serialization.

## Fix direction

Surface push outcome from the store to the CLI (return value, not stderr), and include it in every mutation's JSON output. Applies to create/update/advance/link across stores that push.
