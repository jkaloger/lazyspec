---
title: Restrict milestone link source to github-issues docs
type: iteration
status: complete
author: unknown
date: 2026-06-29
tags: []
related:
- implements: STORY-158
---

## Goal

Milestone = native GitHub-issue field. Only github-issues docs may target milestone via `targets` relation (`github_native = milestone`). Today source unconstrained: any non-milestone doc (specs, local docs) offered `targets` in TUI, fails late at runtime (link.rs "no GitHub issue number"). Restrict source to `StoreBackend::GithubIssues` at both seams.

## Root cause

`validate_milestone_relation` (src/cli/link.rs) blocks milestone-as-source + requires milestone target, but never checks source store. TUI `open_link_editor` (src/tui/state/app.rs) offers milestone-native rel keywords on every non-milestone doc.

## Tasks

- [ ] CLI: `validate_milestone_relation` -- when rel is `github_native=milestone`, require source store == GithubIssues. New error msg when not. (src/cli/link.rs)
- [ ] TUI: `open_link_editor` -- only include milestone-native keywords (+inverses) in `rel_types` when viewed doc store == GithubIssues. Non-issue source never offered `targets`. Set source_blocked/empty-state right. (src/tui/state/app.rs)
- [ ] Tests: CLI validate rejects non-issue source for milestone rel, accepts issue source. TUI rel_types excludes targets for non-issue doc.
- [ ] README: update link constraints if documented.

## Acceptance criteria

- Non-issue doc cannot be source of `targets` -- rejected at CLI validate with clear error, never reaches runtime PATCH.
- TUI link editor does not offer `targets` (or its inverse) when source doc is not github-issues store.
- github-issues source -> milestone still works.
- Existing milestone tests pass; new tests cover both seams.
