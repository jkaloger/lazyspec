---
title: Surface milestone targeted-by issues in related frontmatter
type: iteration
status: superseded
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-168
---## Goal

Milestone doc `related` = `targeted-by` entry per issue assigned to milestone. Inverse of `targets` (github_native milestone). Read-only, derived at resolve.

## Seams

- `src/engine/gh.rs:266` `trait GhMilestoneApi` — add issue-listing method.
- `src/engine/gh.rs:858` real impl (`gh`/REST) + `:1403,:1496` two test fakes.
- `src/engine/store_dispatch.rs:1022` `meta_from_milestone` — `related: vec![]` at `:1050`. Inject here.
- `issue_map` (store field) — number↔shorthand+type. Resolve targets through it; skip unmapped.
- Forward write path ref: `src/cli/link.rs:119` `apply_native_milestone`.

## Approach

1. Extend `GhMilestoneApi`: `fn milestone_issues(&self, repo: &str, number: u64) -> Result<Vec<u64>>`. Real impl = `gh issue list --milestone <title|number> --state all --json number` (state all → open+closed). Map field exact per gh CLI.
2. `meta_from_milestone`: call `milestone_issues`, resolve each number → shorthand via `issue_map`, drop unmapped, push `{type: "targeted-by", target: <SHORTHAND>}` into `related`. Sort stable (by number) → deterministic cache.
3. Callsites already write cache (`create`/`update`/refresh) — inverse rides along, no new write path.
4. Both test fakes (`:1403`,`:1496`) + `NoopGh` (fetch_prune_test.rs) impl new trait method.

## Task breakdown

- [ ] T1: add `milestone_issues` to `GhMilestoneApi` + real gh impl. Verify gh json field name.
- [ ] T2: impl in 3 fakes (return configured map number→milestone).
- [ ] T3: `meta_from_milestone` inject `targeted-by`, resolve via `issue_map`, skip unmapped, stable sort.
- [ ] T4: unit test — milestone w/ 2 mapped + 1 unmapped issue → 2 `targeted-by`, sorted, unmapped absent.
- [ ] T5: test closed issues included (state all).
- [ ] T6: integration — `show MILESTONE-n --json` related shows `targeted-by`; `validate` no dangling.
- [ ] T7: README/CLI doc if surfaced output changes.

## Acceptance criteria

- AC1: milestone `related` has `targeted-by` per open+closed assigned issue mapped to lazyspec doc.
- AC2: targets = shorthand IDs (STORY-n/TICKET-n), never raw number / cache path.
- AC3: unmapped issues skipped, no dangling-relation finding (`validate --json` clean).
- AC4: consistent w/ forward — issue X `targets` M ⇒ M lists X `targeted-by`.
- AC5: read-only — no GitHub milestone writes; derived at resolve.
- AC6: deterministic order (stable sort by issue number) → no cache churn.

## Risks

- gh CLI milestone filter takes title not number in some versions → verify, fallback REST `GET /issues?milestone=n&state=all`.
- `issue_map` may not be populated for issues never synced as docs → those skip (AC3), acceptable.

## Non-goals

- Project `has-member` inverse. Writing relations back to GitHub.