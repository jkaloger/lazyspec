---
title: Push native issue type on lazyspec-created GitHub issues
type: story
status: complete
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-055
---## Problem

RFC-055 lets a `github-issues`-store type declare a native GitHub Issue Type via `github_issue_type` (STORY-191's config schema). Today that field, once it exists, is read on `update --issue_type` only: `GithubIssuesStore::update`'s issue-type handling (src/engine/store_dispatch.rs:952-1070) resolves a `--issue_type` CLI value to a native type-id and pushes it via `push_issue_type` (src/engine/store_dispatch.rs:373-410). `create` (src/engine/store_dispatch.rs:868-940) has no equivalent: a brand-new issue for a type with `github_issue_type` configured is created with no native issue type set at all, leaving the config directive silently unhonored until someone runs a manual `update --issue_type` afterward. RFC-055's Design ("Write side: create") calls this out explicitly as required write-side symmetry.

## Goal

`lazyspec create <type> ...`, for a type with `github_issue_type` configured, pushes that native issue type onto the newly created GitHub issue in the same `create` call — no follow-up `update --issue_type` required. Reuse `push_issue_type` (store_dispatch.rs:373-410) exactly as `update` already does; add no new mutation.

## Design

### Call site: `GithubIssuesStore::create`, not `materialize_one`

Two places in store_dispatch.rs call `.issue_create(...)`: `create` (the `DocumentStore` entry point behind `lazyspec create`, store_dispatch.rs:868-940, `issue_create` at line 904) and `materialize_one` (store_dispatch.rs:479-520, `issue_create` at line 502, used by `materialize_subdir` to push pre-existing filesystem docs onto GitHub). `create_child_subissue` (store_dispatch.rs:728-772) delegates to `create` internally (line 748), so it inherits this story's behavior for free. This story touches only `create`'s call site (line 904); `materialize_one` is explicitly out of scope (see Non-goals).

### Resolve the type id before creating the issue, not after

`update`'s existing pattern (store_dispatch.rs:1023-1043) resolves and validates the native issue-type id from `type_def`'s configured name *before* any remote write, so an invalid value rejects without a wasted mutation. `create` should follow the same discipline, one step earlier: resolve `type_def.github_issue_type` (when `Some`) to a type-id via `GhSchemaSnapshot::load(&self.root)` + `.issue_type_id(name)` (gh_schema.rs:57, :75) *before* calling `issue_create` (store_dispatch.rs:904) — right after the existing `label_ensure` call (store_dispatch.rs:899-901) that resolves the type's label. This way an unresolvable `github_issue_type` (no org-owned repo, or a name absent from the org's schema) fails before a stray issue is created, rather than creating the issue and then failing to tag it. Reuse `update`'s two error branches verbatim (store_dispatch.rs:1030-1040): "native issue types require an organization-owned repository" when `snapshot.issue_types` is empty, "invalid issue_type '{name}': not a known GitHub issue type" otherwise.

### Push the id immediately after issue creation

Immediately after `issue_create` returns (store_dispatch.rs:904-905), with `issue.number` now known, call `self.push_issue_type(issue.number, Some(&resolved_id))?` when `type_def.github_issue_type` was configured and resolved. When it is unset, make no call at all — unlike `update`, which has an explicit "clear" case (`Some(None)` when `--issue_type ""`, store_dispatch.rs:1024), `create` has no prior state to clear: an issue with no configured `github_issue_type` is simply created with GitHub's default (no issue type), exactly as today.

### `github_issue_tag` is not this story's work

RFC-055's Design notes `github_issue_tag`, if also configured, is applied as a label at creation "the same way the default `lazyspec:{type}` / custom `github_label` is today" — i.e. through the *same* existing `label_ensure`/`issue_create` call this story touches (store_dispatch.rs:899-904), not a new one. Once STORY-192's per-type match-rule plumbing resolves a type's effective creation label (falling back through `github_issue_tag` / STORY-190's `github_label` / the default `lazyspec:{type}`), `create`'s existing label call site applies whatever that resolution yields automatically. This story does not implement that label resolution; it only adds the issue-type push alongside it.

## Non-goals

- `github_issue_tag` label resolution at creation time — STORY-190, STORY-191, STORY-192. This story assumes whatever label `create`'s existing `label_ensure`/`issue_create` call resolves to is already correct.
- Read-side classification and discovery (which issues match which type's rule) — STORY-192, STORY-193.
- Dual materialization — STORY-194.
- README documentation of `github_issue_type`/`github_issue_tag` — STORY-196.
- `materialize_one` / `materialize_subdir`'s `issue_create` call site (store_dispatch.rs:502). Pushing native issue type there for subdir-materialized docs is a plausible follow-up but is not required by RFC-055's Story 5 and is left untouched here.
- Clearing a previously-pushed issue type on `create` — not applicable; a newly created issue has no prior native-type state to clear.

## Acceptance criteria

- **Given** a `github-issues`-store type with `github_issue_type` set to a name present in the repo's org issue-type schema, **when** `lazyspec create <type> ...` runs, **then** the newly created GitHub issue has that native issue type set, via a `push_issue_type(issue.number, Some(id))` call made immediately after `issue_create` succeeds.
- **Given** a type with `github_issue_type` unset, **when** `create` runs, **then** no `push_issue_type` call is made and issue creation is byte-for-byte unchanged from today (regression-free default).
- **Given** a type with `github_issue_type` set to a name that does not resolve (empty org issue-type schema, or a name absent from it), **when** `create` runs, **then** it fails with the same error the equivalent `update --issue_type` failure already produces, and no GitHub issue is created (the id resolves, and fails, before `issue_create` is called).
- **Given** `create_child_subissue`, **when** its underlying type has `github_issue_type` configured, **then** the created child issue gets the native issue type pushed too, with no additional code — it delegates to `create`.
