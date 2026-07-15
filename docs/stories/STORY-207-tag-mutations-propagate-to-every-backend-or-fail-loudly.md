---
title: "Tag mutations propagate to every backend or fail loudly"
type: story
status: complete
author: "jkaloger"
date: 2026-07-14
tags:
- tags
- store
related:
- related-to: ADR-024
- related-to: STORY-206
---

## Context

`lazyspec tag add/remove` (STORY-206) propagates to the remote only for `github-issues`. For `clickup-tasks` and `git-ref` backed docs it rewrites the local frontmatter/cache, returns success, and never touches the remote — a silent drop. The propagation logic is a hardcoded `if store != GithubIssues { return Ok(()) }` in `cli/tag.rs`, outside the `DocumentStore` trait, so no backend is forced to handle tags. ADR-024 decides to lift tag sync onto the trait as `sync_tags`, making backend coverage compile-enforced: propagate, or `bail!` visibly.

This story makes the observable behaviour match: a tag mutation either lands on the correct backend or fails with a clear message. No command reports success while dropping a tag.

## Acceptance Criteria

### AC: Filesystem tag mutation unchanged

**Given** a filesystem document `RFC-001` with `tags: [architecture]`
**When** the user runs `lazyspec tag add RFC-001 security`
**Then** frontmatter `tags` becomes `[architecture, security]` and the command succeeds (behaviour identical to STORY-206)

### AC: GitHub-issues tag mutation unchanged

**Given** a github-issues document `ITERATION-042`
**When** the user runs `lazyspec tag add ITERATION-042 security`
**Then** the `security` label is created if absent, applied to the issue, and the local cache reflects the tag (behaviour identical to STORY-206)

### AC: git-ref tag add re-pushes the ref

**Given** a git-ref backed document with coordination configured and `tags: []`
**When** the user runs `lazyspec tag add <ID> security`
**Then** the local cache frontmatter contains `security` **and** the pushed git ref blob contains `security` in its frontmatter

### AC: git-ref tag remove re-pushes the ref

**Given** a git-ref backed document whose ref blob has `tags: [security, auth]`
**When** the user runs `lazyspec tag remove <ID> security`
**Then** the pushed ref blob frontmatter becomes `tags: [auth]`

### AC: git-ref without coordination does not push

**Given** a git-ref backed document with no coordination configured
**When** the user runs `lazyspec tag add <ID> security`
**Then** the local cache is updated and no ref push is attempted (matching `GitRefStore` create/update behaviour)

### AC: ClickUp tag mutation fails loudly

**Given** a clickup-tasks backed document
**When** the user runs `lazyspec tag add <ID> security`
**Then** the command exits non-zero with a message that the clickup-tasks write path is not implemented, and does not report success

### AC: sync routes through the store, not a backend match

**Given** the codebase
**When** a reviewer inspects `cli/tag.rs`
**Then** there is no `StoreBackend::GithubIssues` comparison in the CLI; propagation dispatches through `DocumentStore::sync_tags`

### AC: adding a backend forces a tag decision

**Given** the `DocumentStore` trait
**When** a new backend struct is added without a `sync_tags` impl
**Then** the crate fails to compile until the method is provided

### AC: JSON output preserved

**Given** any document
**When** the user runs `lazyspec tag add <ID> foo --json`
**Then** the output is the JSON document metadata including the updated tags (unchanged from STORY-206)

## Scope

### In Scope

- `sync_tags` (add/remove) method on the `DocumentStore` trait
- Per-backend impls: filesystem (no-op), github-issues (moved from `cli/tag.rs`), git-ref (re-push ref), clickup-tasks (`bail!` unimplemented)
- Routing `cli/tag.rs` propagation through store dispatch; removing the `StoreBackend::GithubIssues` special-case
- Preserving all STORY-206 behaviour for filesystem and github-issues

### Out of Scope

- A working ClickUp tag write path (blocked on RFC-056 write path; this story only makes it fail loudly)
- Tag propagation for `github-milestones` / `github-projects` (labels are not a milestone/project concept; those impls no-op)
- Tag validation or controlled vocabulary
- `tag list` subcommand or TUI changes
