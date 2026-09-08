---
title: Write documents back to the shared git repo
type: story
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-072
---

## Context

STORY-281 leaves a `git` type readable and writes refused. This opens them. The hazard is not the file write -- it is everything around it: which commands count as writes, what a rejected push leaves behind, and which repo allocates the ID.

As a developer editing a shared spec from my service repo, I want writes to a `git`-stored type to land in the declared remote, so that a change I make is visible to every other repo reading those documents.

## Acceptance Criteria

- **Given** a `git` type
  **When** I `create` a document
  **Then** the file is written in the managed clone, committed, and pushed to the declared branch.

- **Given** a `git` type
  **When** I run `update`, `link`, `unlink`, `tag`, `delete`, `ignore`, `unignore`, `pin`, `provenance add`, `provenance remove`, or any `fix` mode that rewrites a document
  **Then** each is likewise committed and pushed -- no mutating command reaches the clone without one.

- **Given** a `git` type
  **When** a mutation reaches it from the TUI rather than the CLI
  **Then** it commits and pushes identically, including the `set_provenance` call the TUI makes directly at `src/tui/state/app.rs:3562` rather than through `ops::*`.

- **Given** the remote has moved ahead of the clone
  **When** a write is pushed and rejected
  **Then** the command exits non-zero; stdout carries no document path and `--json` emits no document object, only an error naming the remote, the branch, and `lazyspec fetch`.

- **Given** the same rejection
  **When** the command exits
  **Then** the outcome is an error rather than `git-ref`'s `PushOutcome::LocalOnly` / `synced: false` at exit 0 (`src/engine/git_ref_store.rs:138`): an unreachable remote is a retry, a moved remote is a conflict a human resolves. RFC-072 Decision 5 records the divergence.

- **Given** a push is rejected
  **When** the command exits
  **Then** the commit and the file it added are rolled out of the clone, leaving it byte-identical to its pre-command state, so `next_number` (`src/engine/fs_ops.rs:154`) does not count an orphan. `reserve_next` sets the precedent with `cleanup_local_ref` (`src/engine/reservation.rs:255`).

- **Given** that rollback
  **When** I run `fetch` and re-run the same command
  **Then** the write lands and the document carries an ID that does not collide with one the remote gained meanwhile.

- **Given** a `git` type using `reserved` numbering
  **When** I `create`
  **Then** the number is reserved against the type's `remote` rather than `[numbering.reserved].remote` (`src/engine/fs_ops.rs:129`), so two repos writing concurrently cannot allocate the same ID.

- **Given** a `git` type
  **When** I run `create --parent` with a parent in a different backend
  **Then** the existing same-store refusal at `src/engine/ops/create.rs:251` applies unchanged.

- **Given** two `git` types with different `remote` values
  **When** I run `create --parent` across them
  **Then** it is refused for the same reason: the guard compares the resolved repo, not just the `StoreBackend` discriminant.

## Scope

### In Scope

- Commit-and-push on every mutating command, CLI and TUI.
- Rejection: non-zero exit, no success output, rollback of the orphan commit.
- ID reservation against the type's remote.
- Extending the cross-backend parent guard to compare resolved repos.

### Out of Scope

- Locking, leasing, or content conflict resolution. The human re-runs after a fetch.
- Batching writes. Push on every write makes a `create` a network round trip; accepted over local commits nothing pushes and a divergence to explain.
- The web view, which is read-only.

## Notes

The write-command list is the crux. `ignore`/`unignore` (`src/cli/ignore.rs:11,20`), `pin` (`src/cli/pin.rs:111,192`), `fix` and `provenance` (`src/cli/provenance.rs:100,152`) all write document files without going through `ops::*`, so each is a silent local-only write unless named. `reservations` is clear -- ref-only, no doc write.

Inherited bug this slice will hit first: `src/engine/ops/create.rs:293` is `let parent_path = root.join(&parent_meta.path)` -- the project root, not the type's resolved doc root. For a `git` type the parent lives in the clone, so subdir-child creation resolves to the wrong path. STORY-283 owns `doc_root`, but nothing in the set names this call site.
