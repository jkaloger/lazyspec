---
title: Add --json to delete, link, unlink, ignore, unignore
type: iteration
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-211
- blocks: ITERATION-300
---

## Objective

`--json` on delete, link, unlink, ignore, unignore.

## Satisfies

STORY-211 AC1, AC2, AC3.

## Context

- Finding: AUDIT-018 F4. Pattern to mirror: `tag` command (`src/main.rs:310-331`, `src/cli/tag.rs`).
- Touch: `src/cli.rs:151-180,223-234` (clap defs), `src/main.rs:267-297,352-363` (dispatch printlns), respective `src/cli/*.rs` runners (`LinkOutcome` etc. already structured)
- Convention: CONVENTION principle 2; DICTUM-006 (CLI patterns)

## Tasks

1. Add `--json` flag to 5 clap variants.
2. Serialize existing outcomes (id, path, action, target) per command; human output unchanged sans flag.
3. Integration tests: each command × `--json` → valid JSON w/ expected fields.
4. README flag mentions where commands documented.
5. `cargo test`.

## Out of scope

`setup`, `skills` (STORY-211 out-of-scope). Changing outcome semantics.

## Verification

`for c in delete link unlink ignore unignore; do lazyspec $c --help | grep -q json; done` all pass; outputs parse w/ jq.

