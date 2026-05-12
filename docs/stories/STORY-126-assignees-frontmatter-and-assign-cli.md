---
title: Assignees frontmatter and assign CLI
type: story
status: draft
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
- blocks: STORY-128
---

## In Scope

- An `assignees` list on document frontmatter, applicable to the configured `claim_type` (default `story`) and conceptually any type.
- A new `[orchestration]` configuration section with an `agent_users` setting that lists the user names lazyspec treats as agent-eligible (e.g. `claude-bot`).
- Bidirectional synchronisation of `assignees` with native issue assignees on the GitHub Issues backend.
- Per-backend validation at the store boundary: the GitHub Issues backend rejects writes whose assignees do not resolve to real GitHub users; the filesystem and git-ref backends accept arbitrary free-form strings.
- A `lazyspec assign <DOC_ID>` CLI command that adds an assignee to a document's frontmatter as a normal store write. If `--user` is omitted, the first entry in `agent_users` is used as the default. The command works whether or not a daemon is running. When the daemon's IPC socket is reachable, the command additionally sends a `kick` notification so the daemon re-evaluates eligibility immediately.
- `--json` output mode for `lazyspec assign`.

## Out of Scope

- Daemon eligibility filter logic (which assignees count as agents, work selection): covered by slice 4.
- Lease acquisition and release: covered by slice 4.
- The daemon-side handler for the `kick` IPC message: covered by slice 6. This slice only sends the message when a socket is reachable.
- The daemon process itself: covered by slice 2.

## Acceptance Criteria

1. **Filesystem store round-trip.**
   Given a document on the filesystem backend with assignees `["alice", "claude-bot"]` in frontmatter,
   When the document is loaded and re-written through the store,
   Then the assignees field is preserved exactly in order and value.

2. **Git-ref store round-trip.**
   Given a document stored under the git-ref backend with assignees `["bob"]`,
   When the document is read back from the ref,
   Then the assignees field contains `["bob"]`.

3. **GitHub Issues bidirectional sync (lazyspec to GitHub).**
   Given a document backed by a GitHub issue and assignees set locally to `["claude-bot"]`,
   When the document is written through the GitHub Issues backend,
   Then the corresponding GitHub issue has `claude-bot` set as a native assignee.

4. **GitHub Issues bidirectional sync (GitHub to lazyspec).**
   Given a GitHub issue whose native assignees are `["alice"]`,
   When the document is loaded through the GitHub Issues backend,
   Then the document's `assignees` field is `["alice"]`.

5. **GitHub Issues validation rejects unknown user.**
   Given a write to the GitHub Issues backend with an assignee that does not resolve to a real GitHub user,
   When the store attempts to persist the document,
   Then the write fails with a validation error identifying the unresolvable assignee, and no partial state is persisted.

6. **Filesystem and git-ref accept free-form strings.**
   Given a write to the filesystem or git-ref backend with assignees `["not-a-real-github-user"]`,
   When the store persists the document,
   Then the write succeeds and the value is stored verbatim.

7. **`lazyspec assign` default user.**
   Given `agent_users = ["claude-bot", "other-bot"]` in configuration and a document without `claude-bot` in its assignees,
   When the user runs `lazyspec assign <DOC_ID>` without `--user`,
   Then `claude-bot` is appended to the document's `assignees` and the change is persisted to the active store.

8. **`lazyspec assign --user` explicit user.**
   Given any document,
   When the user runs `lazyspec assign <DOC_ID> --user alice`,
   Then `alice` is appended to the document's `assignees` and the change is persisted.

9. **`--json` output.**
   Given any successful `lazyspec assign` invocation,
   When `--json` is supplied,
   Then stdout is a single JSON object describing the document ID, the assignee added, and the resulting assignees list.

10. **Kick on reachable daemon, no-op when absent.**
    Given a running daemon listening on its IPC socket,
    When `lazyspec assign` completes a write,
    Then a `kick` message is sent to the daemon socket before the command returns; and when no socket is reachable, the command still succeeds and exits cleanly without error.

## Notes

- This slice covers the **Eligibility metadata** and **Configuration (`agent_users`, `claim_type`)** sections of RFC-041, plus the `assign` portion of the **CLI surface**.
- The validation behaviour at the store seam follows dictum 4 of the project's architectural dicta: each backend is responsible for enforcing the constraints of its underlying system, rather than pushing validation up into shared code.
- The `kick` message is best-effort. The daemon-side contract for handling it is intentionally deferred to slice 6 so this slice can land independently and ship value (assigning agents to work) without the daemon.
- Default `claim_type` is `story`. The `assignees` field is not specific to stories; it lives on the generic document frontmatter so other types can adopt it later.

