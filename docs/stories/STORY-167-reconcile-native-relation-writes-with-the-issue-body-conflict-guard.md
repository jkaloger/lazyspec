---
title: "Reconcile native-relation writes with the issue-body conflict guard"
type: story
status: complete
author: "jkaloger"
date: 2026-06-26
tags: []
related:
- implements: RFC-050
---
## Context

Two staleness policies coexist in the github-issues store and disagree. The issue-body round-trip uses conflict detection: a write checks the remote `updated_at` against the locally cached timestamp and bails if the remote moved ("TICKET-N has been modified on GitHub since your last fetch"). Native relations were specified as last-write-wins with no `check_lock` (STORY-158 for milestones, STORY-161 for membership). When `link` writes a native milestone edge, it issues the milestone PATCH (last-write-wins, succeeds) but then mirrors the edge into the local cache `.md`, and that mirror write goes through the issue store's conflict guard.

Dogfooding reproduced the divergence: posting a comment on the issue via `gh` (an out-of-band edit) bumped the remote `updated_at`; the next `link ... targets` errored on the conflict guard even though the milestone field is last-write-wins. The PATCH had already applied remotely, so GitHub showed the milestone set, but the cache-mirror aborted and `show TICKET-N --json` reported `related: []` — the local view and the remote disagreed precisely because the write was half-applied. An out-of-band comment, which touches no field this write cares about, should not block a last-write-wins relation write.

This slice makes the native-relation write path and its cache mirror consistent. The native edge write keeps last-write-wins; the subsequent cache mirror must not be gated by a conflict guard that the field write itself does not honor. Either the mirror refreshes from remote before writing (so it never conflicts), or it writes the known-good post-PATCH state directly. The invariant to restore: after a successful native-relation write, the local cache reflects the edge, so `show --json .related` agrees with GitHub regardless of unrelated out-of-band edits.

## Acceptance Criteria

- **Given** an issue whose remote `updated_at` advanced due to an out-of-band edit that touched no lazyspec-managed field (e.g. a comment)
  **When** `link <issue> targets <milestone>` runs
  **Then** the milestone PATCH applies and the cache mirror completes without a conflict error, and `show <issue> --json` reports the edge in `related`.

- **Given** a successful native-relation write (milestone or membership)
  **When** the operation returns
  **Then** the local cache `.md` and the remote agree on the edge — no half-applied state where GitHub has the edge and the cache does not.

- **Given** a genuine conflict on a lazyspec-managed field (the issue body itself changed remotely)
  **When** a body-affecting write is attempted
  **Then** the conflict guard still fires for that write (this slice does not weaken body-write protection).

- **Given** `unlink` of a native relation under the same out-of-band-edit condition
  **When** the unlink runs
  **Then** it applies and the cache mirror agrees, symmetric with `link`.

## Scope

### In Scope

- Decouple the native-relation cache mirror from the issue-body conflict guard, so a last-write-wins relation write is not blocked by unrelated remote `updated_at` advances.
- Restore the post-write invariant: cache `.related` matches the remote edge after `link`/`unlink`.
- Cover both milestone (STORY-158) and membership (STORY-161) native relations.
- Fakes at the gh seam reproducing the out-of-band `updated_at` advance and asserting the mirror completes.

### Out of Scope

- Removing or weakening conflict detection on issue-body writes (body protection is retained).
- Field-level conflict detection (detecting that the *specific* field changed remotely) — last-write-wins is retained for native fields.
- Three-way merge of concurrent edits.
- The schema-snapshot/org-resolution warnings that share the same dogfooding session (STORY-163, STORY-165).
