---
title: Git Ref Task Coordination
type: rfc
status: draft
author: jkaloger
date: 2026-03-26
tags:
- distributed
- git
- agents
- coordination
related:
- related-to: docs/rfcs/RFC-030-git-based-document-number-reservation.md
---


## Problem

lazyspec has two categories of documents that need different treatment:

_Specs_ (RFCs, stories, ADRs) are design artifacts. Humans write and review them. They belong in the working tree, in git history, in PRs. They're the permanent record.

_Iterations_ are tasks. An agent picks up an iteration, does the work, marks it done. The code is the deliverable -- the iteration document is coordination overhead. But right now iterations live in the filesystem alongside specs, cluttering the working tree, git history, and PRs with documents that are only useful during active work.

Meanwhile, there's no coordination mechanism at all. Two agents assigned to update the same RFC will both edit it, producing a merge conflict that's only visible at merge time. An orchestrator spawning 10 agents has no way to assign work, track progress, or detect crashed agents.

The existing reservation system (RFC-030) prevents numbering collisions. This RFC addresses what happens after documents exist: who owns them, where tasks live, and how agents coordinate.

## Intent

Add a `store` field to each document type, controlling where its documents are persisted:

- `filesystem` (default): documents live in the working tree as they do today. Visible in `git log`, diffs, and PRs. Protected by hard-gated locks when coordination is enabled.
- `git-ref`: documents live in git custom refs (`refs/lazyspec/{type}/{id}`). Invisible to the working tree, `git log`, and PRs. Visible to `lazyspec` CLI and TUI. Protected by the same lock mechanism.

The `store` field is a storage backend selector, not a semantic classification. Any document type can use any backend. The typical configuration puts specs (RFCs, stories, ADRs) in `filesystem` and tasks (iterations) in `git-ref`, but this is convention, not constraint.

The `store` field is designed to be extensible. Future backends (e.g. `sqlite`, `github-issues`) can be added without changing the document model, relationship system, or validation rules.

Both backends share the same coordination primitives: lease-based locks backed by git refs, agent identity, crash recovery via lease expiry.

Claude Code hooks handle orchestration: boot coordination, heartbeat leases, claim/release on session boundaries.

## Design

### Storage Backends

The `store` field on each type selects the persistence backend:

```toml
[[types]]
name = "rfc"
prefix = "RFC"
dir = "docs/rfcs"
store = "filesystem"      # default

[[types]]
name = "iteration"
prefix = "ITERATION"
dir = "docs/iterations"
store = "git-ref"
```

@ref src/engine/config.rs#TypeDef

@draft Store {
    Filesystem,   // default: working tree files
    GitRef,       // git custom refs
    // future: Sqlite, GithubIssues, etc.
}

The backend controls _where_ the document lives. Everything else -- frontmatter schema, relationships, validation rules, status tracking -- is the same regardless of backend. An iteration stored in `git-ref` has the same fields, the same `implements` link to a story, and the same validation as one stored in `filesystem`.

| | `filesystem` | `git-ref` |
|---|---|---|
| Storage | Working tree (`docs/rfcs/`, etc.) | `refs/lazyspec/{type}/{id}` |
| In working tree | Yes | No |
| In `git log` / PRs | Yes | No |
| In TUI | Yes | Yes |
| In `lazyspec show/list/context` | Yes | Yes |
| Has frontmatter & relationships | Yes | Yes |
| Write protection | Hard-gated lock | Hard-gated lock |
| Lifetime | Permanent in git history | Permanent in refs |

### Hard-Gated Locks

Locks live in `refs/lazyspec/locks/{type}/{id}`. Each lock ref points at a commit containing `lock.json`:

```json
{
  "agent": "agent-7",
  "acquired": "2026-03-26T10:00:00Z",
  "expires": "2026-03-26T11:00:00Z"
}
```

Lock refs use commits (not bare blobs) because hosted git platforms reject non-commit refs.

When coordination is configured, the lock is a hard gate. `lazyspec create`, `lazyspec update`, and `lazyspec delete` refuse to write without a held lock, regardless of storage backend:

```
$ lazyspec update RFC-042 --set-status accepted
Error: RFC-042 is not claimed. Run `lazyspec claim RFC-042` first.
```

> [!NOTE]
> For `filesystem` documents, direct file edits (`vim docs/rfcs/RFC-042.md`) bypass the gate because git can't enforce ref-based locks on working tree files. `lazyspec validate` detects unlocked modifications and warns. The gate covers all writes through `lazyspec` CLI and TUI.

#### Lock Operations

| Operation | Mechanism |
|-----------|-----------|
| Acquire | Create lock commit, push to `refs/lazyspec/locks/{type}/{id}`. Fails if ref exists. |
| Release | Delete lock ref on remote. Verifies caller is the holder. |
| Admin release | Delete lock ref, bypassing expiry. Requires `--expected-holder` matching current holder. For orchestrators. |
| Heartbeat | New lock commit with updated expiry, parented on current. Push with `--force-with-lease` (CAS). |
| Force-acquire | Check `now > lock.expires + grace_period`. If expired, delete and reacquire. |
| Query | `lazyspec locks` lists all held locks. |

#### Heartbeat and Lease Management

The CLI is stateless. Heartbeat is caller-driven:

- _Claude Code hooks_: a `post-tool-use` hook runs `lazyspec heartbeat` after each tool invocation, extending the lease while the agent is active. Session start hook claims, session end hook releases.
- _Orchestrators_: run `lazyspec heartbeat <doc> --agent-id <id>` on a timer.
- _TUI_: heartbeats held locks on its poll interval.
- _Humans_: don't use the iteration workflow. For spec edits, set a long lease (`60m`+).

Default lease duration is 60 minutes. Grace period for force-acquire is 2 minutes (absorbs NTP drift).

### Git-Ref Backend

Documents with `store = "git-ref"` are stored as commit chains under `refs/lazyspec/{type}/{id}`:

```
refs/lazyspec/iteration/042   → commit chain containing ITERATION-042.md
refs/lazyspec/iteration/043   → commit chain containing ITERATION-043.md
```

Each ref points at a commit whose tree contains the document markdown. Updates create new commits parented on the previous, giving per-document history. `git update-ref` uses the three-argument CAS form to prevent concurrent overwrites.

Git-ref documents are invisible to:
- `git log` (commits are in custom refs, not branch history)
- `git diff` / PRs (not in the working tree)
- IDE file trees

Git-ref documents are visible to:
- `lazyspec list iteration` (reads from refs)
- `lazyspec show ITERATION-042` (reads from refs, resolves `@ref` directives)
- `lazyspec context ITERATION-042` (shows full chain across backends)
- The TUI (displays alongside filesystem documents)

### Relationships Across Backends

Documents link to each other using the same relationship system, regardless of where each lives:

```yaml
---
title: "Auth refactor implementation"
type: iteration
status: in-progress
related:
- implements: docs/stories/STORY-075-auth-refactor.md
---
```

The `implements` target is a filesystem path (the story is a `filesystem` document). The engine resolves relationships across backends transparently. `lazyspec context` follows the chain:

```
$ lazyspec context ITERATION-042
RFC-030 (Git-Based Document Number Reservation)
  └── STORY-075 (Auth refactor)           ← filesystem
        └── ITERATION-042 (Implementation) ← git-ref, in-progress, held by agent-7
```

`lazyspec show ITERATION-042 -e` expands `@ref` directives in the body, pulling content from source code files as it does today.

### Unified Document Engine

The document engine currently reads from configured `dir` paths on the filesystem. With `git-ref` documents in refs, the engine needs a unified read path dispatched by `store`:

- `filesystem`: read from `TypeDef.dir`, as today
- `git-ref`: read from `refs/lazyspec/{type}/*` via git2

`lazyspec list`, `lazyspec search`, `lazyspec show`, `lazyspec validate`, `lazyspec context`, and `lazyspec status` all operate across backends. The TUI's document tree merges both sources.

`lazyspec validate` runs the same rules across backends: "iterations need stories" checks that an iteration has an `implements` link to a story, regardless of where each lives.

### Agent Identity

Priority chain:

1. `$LAZYSPEC_AGENT_ID` (explicit, for orchestrators)
2. `$CLAUDE_SESSION_ID` (auto-detected in Claude Code)
3. `git config user.name` + sqids-encoded PID (fallback)

### Claude Code Hooks

Claude Code hooks automate the coordination lifecycle:

```json
// .claude/settings.json
{
  "hooks": {
    "session-start": "lazyspec claim $ASSIGNED_TASK --agent-id $CLAUDE_SESSION_ID",
    "post-tool-use": "lazyspec heartbeat $ASSIGNED_TASK --agent-id $CLAUDE_SESSION_ID",
    "session-end": "lazyspec release $ASSIGNED_TASK --agent-id $CLAUDE_SESSION_ID"
  }
}
```

The orchestrator sets `$ASSIGNED_TASK` when spawning the agent. The hooks handle claim, heartbeat, and release without the agent needing to know about coordination.

### Git2 Crate

All git ref operations use the `git2` crate (libgit2 bindings) rather than shelling out. This eliminates process spawn overhead, `.git/FETCH_HEAD.lock` contention from concurrent fetches, and stderr parsing.

@ref src/engine/reservation.rs#reserve_next

The existing reservation module can be migrated to git2 in a follow-up.

### Fetch Refspecs

Custom refs aren't fetched by default. The `.git/config` needs:

```
[remote "origin"]
    fetch = +refs/lazyspec/*:refs/lazyspec/*
```

`lazyspec init` adds this when any type uses `store = "git-ref"`. `lazyspec setup` adds it for new clones. `lazyspec validate` warns if it's missing.

### Init and Setup

- `lazyspec init`: creates `.lazyspec.toml` with store config, adds refspec when needed, updates `.gitignore` for `git-ref` type dirs. Interactive wizard with flag overrides.
- `lazyspec setup`: for new clones. Reads existing `.lazyspec.toml`, adds refspec, runs initial fetch of ref-stored documents.

Shallow clones are detected and warned against (`git rev-parse --is-shallow-repository`).

### Configuration

@draft CoordinationConfig {
    remote: String,           // default "origin"
    lease_duration: String,   // default "60m"
    grace_period: String,     // default "2m"
    max_push_retries: u8,     // default 5
}

```toml
[[types]]
name = "rfc"
prefix = "RFC"
dir = "docs/rfcs"
store = "filesystem"

[[types]]
name = "story"
prefix = "STORY"
dir = "docs/stories"
store = "filesystem"

[[types]]
name = "iteration"
prefix = "ITERATION"
dir = "docs/iterations"
store = "git-ref"

[[types]]
name = "adr"
prefix = "ADR"
dir = "docs/adrs"
store = "filesystem"

[coordination]
remote = "origin"
lease_duration = "60m"
grace_period = "2m"
max_push_retries = 5
```

### Graceful Degradation

If the remote is unreachable:
- _Claim, release, heartbeat_: fail. Coordination requires a coordinator.
- _Git-ref create/update_: local ref commit succeeds. Push fails but document is readable locally.
- _Git-ref read_: works from locally fetched refs.
- _Filesystem reads_: always work.
- _Filesystem writes (via lazyspec)_: fail if lock check requires remote.

### Future Backends

The `store` field is an enum that can grow. Potential future backends:

- `sqlite`: local database for fast queries, bulk operations, and offline-first workflows. No remote coordination, but useful for large projects where filesystem scanning is slow.
- `github-issues`: documents backed by GitHub Issues or Discussions. Built-in commenting, review, and search. Requires network and couples to GitHub, but gives native reviewability.

Each backend implements the same trait: create, read, update, delete, list, search. The document model, relationships, and validation are backend-agnostic.

## Stories

1. Lock engine and CLI -- git2-based lease locks on `refs/lazyspec/locks/{type}/{id}` using commit objects. Acquire, release, admin-release, heartbeat, force-acquire with grace period, query. Agent identity resolution. `lazyspec claim/release/locks/heartbeat` subcommands. Hard-gate enforcement on writes via lazyspec (both backends).

2. Git-ref storage backend and unified engine -- git2-based commit-chain CRUD on `refs/lazyspec/{type}/{id}`. `Store` enum and backend dispatch trait. Unified document engine that reads from filesystem or refs based on type config. Extend `list/show/search/validate/context/status` to operate across backends. Relationship resolution across backends.

3. Init, setup, and config -- `store` field on `TypeDef`. `[coordination]` config section. `lazyspec init` wizard with store selection. `lazyspec setup` for new clones. Fetch refspec management. `.gitignore` for git-ref type dirs. Shallow clone detection. Shared remote validation with `[numbering.reserved]`.

4. TUI integration -- display git-ref documents alongside filesystem documents. Lock status indicators. Claim/release from TUI. Heartbeat on poll. Context chain display across backends (RFC -> Story -> Iteration). Status filtering.

5. Claude Code hooks -- hook definitions for session-start (claim), post-tool-use (heartbeat), session-end (release). Documentation for orchestrator integration. `$ASSIGNED_TASK` convention.
