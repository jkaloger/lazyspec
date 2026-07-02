---
title: Push native issue type on lazyspec-created GitHub issues
type: iteration
status: complete
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-195
---

## Objective

`GithubIssuesStore::create` resolves `type_def.github_issue_type` before issue creation, pushes it via `push_issue_type` right after -- no follow-up `update --issue_type` needed.

## Context

- Story: STORY-195 (assumes STORY-191 schema landed -- ITERATION-262)
- Design (exact resolve-before-create ordering, error message reuse, `create_child_subissue` free-ride): STORY-195 body verbatim -- don't re-derive.
- Touch: src/engine/store_dispatch.rs `create` (868-940, `issue_create` call at 904; resolve+push added around it), reusing `push_issue_type` (373-410) and `update`'s existing resolve pattern (1023-1043) and error branches (1030-1040) verbatim.
- `materialize_one` (479-520) NOT touched -- separate call site, out of scope.

## Satisfies

STORY-195 AC1-AC4 (all -- one call-site change: resolve-then-create-then-push, tested across configured/unconfigured/unresolvable/child-subissue cases).

## Tasks

1. In `create`, after existing `label_ensure` call (899-901) and before `issue_create` (904): when `type_def.github_issue_type` is `Some(name)`, load `GhSchemaSnapshot::load(&self.root)` + `.issue_type_id(name)`; on empty `snapshot.issue_types` or unresolved name, fail with the same two error strings `update` uses (1030-1040) -- before any remote write.
2. After `issue_create` returns and `issue.number` is known: if resolved, call `self.push_issue_type(issue.number, Some(&resolved_id))?`. When `github_issue_type` unset: no call at all (no "clear" case needed -- new issue has no prior state).
3. Confirm `create_child_subissue` (728-772, delegates to `create` at line 748) inherits this behavior with no code change -- add a test proving it, not new logic.

## Out of scope

- `github_issue_tag` label resolution at creation -- STORY-190/191/192; this story assumes `create`'s existing `label_ensure`/`issue_create` call already resolves the right label.
- Read-side classification/discovery -- STORY-192/193.
- Dual materialization -- STORY-194.
- README -- STORY-196.
- `materialize_one`'s `issue_create` call site (502) -- plausible follow-up, not required here.
- Clearing a previously-pushed type on `create` -- not applicable, no prior state.

## Principles/conventions

Engine-only (CONVENTION.md L3) -- reuses `push_issue_type` and `update`'s error branches verbatim rather than inventing new error strings.

## Verification

- `github_issue_type` set to a name present in org schema: new issue has that native type set, via `push_issue_type` called right after `issue_create` succeeds.
- `github_issue_type` unset: no `push_issue_type` call, issue creation byte-for-byte unchanged.
- `github_issue_type` set to an unresolvable name (empty org schema, or name absent): same error `update --issue_type` produces, and no issue is created at all (fails before `issue_create`).
- `create_child_subissue` with `github_issue_type` configured on its type: child issue gets the type pushed too, with zero new code exercised beyond the delegation to `create`.

