---
title: Fall back to git user.name for the template author
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-007
---

## Value

As an author, I want `create` to stamp my git identity into `{author}` without passing `--author`, so documents stop landing with `author: unknown`.

## Acceptance Criteria

- AC1: `create` resolves the author as `--author`, then `git config user.name`, then `unknown`. An empty `user.name` counts as unset.
- AC2: The git lookup goes through the existing git seam (`GitRefOps`, `src/engine/git_ref.rs:86`), not a raw `Command` in the CLI layer. A mock returning a name makes the CLI test deterministic.
- AC3: The TUI create form uses the same resolution when its author field is blank.
- AC4: `create --help` documents the fallback order. The `unknown` default shown in help goes away.

## Scope

### In Scope

- Replace `#[arg(long, default_value = "unknown")]` (`src/cli.rs:117`) with an optional arg resolved after parsing.
- TUI blank-author fallback (`src/tui/state/app.rs:3043`).
- Fold the two existing raw lookups onto the seam: `git_user_name` (`src/cli/init.rs:361`) and `git_author` (`src/engine/ops/fix/fields.rs:174`). Sharing the seam is the point; this is not a new feature surface.

### Out of Scope

- Persisting an author in config. Config has no author field (`src/cli/init.rs:357`).
- `GIT_AUTHOR_NAME` or other identity sources.
