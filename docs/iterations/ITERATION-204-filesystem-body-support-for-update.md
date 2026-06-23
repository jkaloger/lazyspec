---
title: Filesystem body support for update
type: iteration
status: complete
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: STORY-100
---

## Intent

`lazyspec update <ID> --body`/`--body-file` work for filesystem-store docs. Today they bail; only github-issues store supports body update. Inconsistent: `create` already takes `--body` for filesystem. Authoring skills (co-write/scaffold/generate) tell agent apply draft via `update --body-file`, but `<NEVER>` says don't edit files directly. Contradiction on default store. Fix makes `--body` canonical body-write path.

## Root cause

`src/engine/fs_ops.rs:191` `update_document` bails when any update key == `body`. Function already splits `(yaml, body)` and recomposes via `compose_frontmatter`. Body support = set body segment instead of bail. No new I/O. Test `filesystem_update_rejects_body` (`store_dispatch.rs`) pins old behaviour → flip.

## Tasks

1. **fs_ops body write.** `update_document`: drop bail. Split body updates from frontmatter updates. Frontmatter keys → existing in-place YAML replace. `body` key → replace body segment. Recompose, write. Both in one call OK.
2. **Flip test.** `filesystem_update_rejects_body` → `filesystem_update_sets_body`: update body, re-read, assert new body present + frontmatter intact.
3. **Skill prose.** `skills/lazy` `<GITHUB-ISSUES-DOCUMENTS>` block + `<NEVER>` "do not edit files directly", and co-write/scaffold/generate "apply accepted draft" step: encourage `update --body`/`--body-file` as body-write path for ALL stores. Kill "edit the file directly" guidance. `<NEVER>` keep "use create/link not raw file writes" but reconcile with --body.
4. **README.** If `update --body` documented as github-issues-only, correct it. Update CLI interface section.

## Test plan

- `create` fs doc → `update --body "X"` → `show` body == "X". (was: error)
- Combined: `update --status review --body "Y"` → status + body both change.
- `--body-file f.md` and `--body-file -` (stdin).
- `cargo test` green; flipped test passes.
- Skill grep: no "edit the file directly" contradiction remains.

