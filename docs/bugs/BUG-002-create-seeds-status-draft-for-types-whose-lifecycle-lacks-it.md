---
title: "create seeds status draft for types whose lifecycle lacks it"
type: bug
status: triaged
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Summary

`lazyspec create` seeds `status: draft` regardless of the type's lifecycle. For a type whose lifecycle has no `draft` state (e.g. `bug`: reported → triaged → in-progress → fixed/wontfix), the new doc is born in an invalid state.

## Reproduction

1. Config a type with lifecycle states `[reported, triaged, ...]` (shipped `bug` type).
2. `lazyspec create bug \"x\" --json` → `\"status\": \"draft\"`.
3. `lazyspec update BUG-N --status reported` → `Error: invalid transition for type \"bug\": no edge from \"draft\" to \"triaged\" (allowed targets: (none))`.
4. `lazyspec fix BUG-N --dry-run --json` → no fixes offered.

## Expected

New docs start at the first state of the type's lifecycle (`states[0]`), and `fix` repairs a status that is not in the lifecycle.

## Actual

Doc stuck at `draft` with no legal transitions; only escape is hand-editing frontmatter.

## Fix direction

Create seeds `lifecycle.states[0]`; `fix` flags out-of-lifecycle statuses and offers `states[0]` (field fix). Observed live on BUG-001 in this repo.
