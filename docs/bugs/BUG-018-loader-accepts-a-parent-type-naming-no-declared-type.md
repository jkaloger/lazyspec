---
title: "Loader accepts a parent_type naming no declared type"
type: bug
status: reported
author: "Jack Kaloger"
date: 2026-09-02
tags: []
related: []
---

## Symptom

`config add-type note notes docs/notes NOTE --parent-type nonsense` writes the config and it loads clean. `create note` then succeeds with no parent, and `validate` reports every document valid. The parent gate is silently inert.

## Cause

`Config::parse` never reads `parent_type` against the declared type names. Every other cross-reference on a type or edge row is refused at load; this one is not. ITERATION-395's read-back guard cannot refuse what the loader accepts, so the CLI, the TUI settings save and `fix --config` all write it.

## Fix

Add a `parent_type` check to `parse_inner` beside the edge-position checks, refusing a name no `[[types]]` row declares, with a message naming the type and the missing parent. The CLI guard and the TUI save then surface it for free. Invert `an_unknown_parent_type_is_written_because_the_loader_has_no_check_for_it` in `src/cli/config.rs` into the refusal it was written to stand in for.

## Found

STORY-261 review pass, 2026-09-02.
