---
title: Route agent edits through the CLI and inject their specs
type: story
status: draft
author: Jack Kaloger
date: 2026-09-12
tags: []
related:
- implements: RFC-071
- blocks: STORY-288
---

## Context

As an agent working in a lazyspec repo, I want my writes to lazyspec-owned files refused before they run, and my writes to source files to arrive with the governing documents attached, so that conformance is a mechanism instead of prose I can argue past.

Ten skills carry a NEVER block in six textual variants and nothing enforces any of them. Separately, RFC-068 built a reverse index (`why <path>`) that no skill calls and no event fires, so the pins sit unread. Both are the same fix: the edit itself is the trigger.

## Acceptance Criteria

- **Given** a `PreToolUse` hook on `Edit|Write|MultiEdit`, **when** the path is a `*.md` under any `dir` declared in `lazyspec config --json`, **then** it is denied with a reason naming `lazyspec update <ID> --body`.
- **Given** the path is `.lazyspec.toml`, **then** it is denied naming `lazyspec config set <key> <value>`; **given** a path under `.lazyspec/cache/`, **then** it is denied naming `lazyspec fetch`.
- **Given** a repo whose type dirs are not `docs/`, **then** the guard still refuses its document paths — dirs come from config, never hardcoded.
- **Given** an edit to a source file `lazyspec why --json` matches, **then** context gains a line of the shape `Governed by: SPEC-002 Document Store (draft) via src/engine/store/**` and the edit proceeds.
- **Given** the inject branch, **then** the hook omits `permissionDecision` entirely so the permission flow is untouched. `permissionDecision: "allow"` is never emitted on any branch.
- **Given** a `Bash` command naming a path under a typed `dir` alongside a write form (`>`, `>>`, `sed -i`, `tee`, `mv`, `cp`), **then** it is denied with the same reason the edit guard gives; a read of the same path (`cat`, `grep`) is allowed.
- **Given** any other path, **then** the hook exits silently.

## Scope

### In Scope

- One `PreToolUse` entry on `Edit|Write|MultiEdit` and one on `Bash`, scripts in `hooks/`, registered in `hooks/hooks.json`, shipped by the existing `.claude-plugin` manifest. Shell and jq only — no Rust.
- Classification via `config --json`; injection shaped from `why --json`.

### Out of Scope

- Staleness. The line reports governance, not currency; a stale `reviewed` sha still reads as authoritative. RFC-069 owns that.
- A session cache for injection. Five edits to one file inject the same line five times; accepted.
- Parsing shell. The Bash guard is a heuristic over the write forms harness modes actually use — a form it misses gets through, a read it misreads costs a retry.
- Any allowlist, and any skill prose telling the agent to run `why`.
