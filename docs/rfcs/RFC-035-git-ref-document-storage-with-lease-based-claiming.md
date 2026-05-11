---
title: Git Ref Task Coordination
type: rfc
status: accepted
author: jkaloger
date: 2026-03-26
tags:
- distributed
- git
- agents
- coordination
related:
- related-to: RFC-030
---




## Problem

lazyspec has two categories of documents that need different treatment:

_Specs_ (RFCs, stories, ADRs) are design artifacts. Humans write and review them. They belong in the working tree, in git history, in PRs. They're the permanent record.

_Iterations_ are tasks. An agent picks up an iteration, does the work, marks it done. The code is the deliverable -- the iteration document is coordination overhead. But right now iterations live in the filesystem alongside specs, cluttering the working tree, git history, and PRs with documents that are only useful during active work.

Meanwhile, there's no coordination mechanism at all. Two agents assigned to update the same RFC will both edit it, producing a merge conflict that's only visible at merge time. An orchestrator spawning 10 agents has no way to assign work, track progress, or detect crashed agents.

The existing reservation system (RFC-030) prevents numbering collisions. This RFC addresses what happens after documents exist: who owns them, where tasks live, and how agents coordinate.

## Intent

The `store` field on each document type already supports `filesystem` (default) and `github-issues` (RFC-037). This RFC adds two capabilities:

- `git-ref`: documents live in git custom refs (`refs/lazyspec/{type}/{id}`). Invisible to the working tree, `git log`, and PRs. Visible to `lazyspec` CLI and TUI. No external service dependency.
- **Lease-based coordination**: leases backed by git refs (`refs/lazyspec/leases/{type}/{id}`) that gate writes through `lazyspec` CLI and TUI, regardless of storage backend. Agent identity, heartbeat, crash recovery via lease expiry.

The typical configuration puts specs (RFCs, stories, ADRs) in `filesystem` and tasks (iterations) in `git-ref`, but any document type can use any backend.

Both backends share the same coordination primitives: leases backed by git refs, agent identity, crash recovery via lease expiry.

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

@draft StoreBackend {
    Filesystem,     // default: working tree files (existing)
    GithubIssues,   // GitHub Issues API (existing, RFC-037)
    GitRef,         // git custom refs (this RFC)
}

The backend controls _where_ the document lives. Everything else -- frontmatter schema, relationships, validation rules, status tracking -- is the same regardless of backend. An iteration stored in `git-ref` has the same fields, the same `implements` link to a story, and the same validation as one stored in `filesystem`.

| | `filesystem` | `git-ref` |
|---|---|---|
| Storage | Working tree (`docs/rfcs/`, etc.) | `refs/lazyspec/{type}/{id}` |
| Cache | N/A | `.lazyspec/cache/{type}/` (same path as `github-issues`) |
| In working tree | Yes | No |
| In `git log` / PRs | Yes | No |
| In TUI | Yes | Yes |
| In `lazyspec show/list/context` | Yes | Yes |
| Has frontmatter & relationships | Yes | Yes |
| Write protection | Lease-gated | Lease-gated |
| Lifetime | Permanent in git history | Permanent in refs |

### Lease-Gated Writes

Leases live in `refs/lazyspec/leases/{type}/{id}`. Each lease ref points at a commit containing `lease.json`:

```json
{
  "agent": "agent-7",
  "acquired": "2026-03-26T10:00:00Z",
  "expires": "2026-03-26T11:00:00Z"
}
```

Lease refs use commits (not bare blobs) because hosted git platforms reject non-commit refs.

> [!NOTE]
> This is distinct from the optimistic locking in `GithubIssuesStore`, which uses GitHub's `updated_at` timestamps to detect concurrent edits. Leases coordinate between lazyspec clients; optimistic locks coordinate between lazyspec and direct GitHub edits. Both mechanisms can be active simultaneously for `github-issues` documents (see RFC-037 § Interaction with RFC-035 Coordination).

When coordination is configured, the lease is a hard gate. `lazyspec create`, `lazyspec update`, and `lazyspec delete` refuse to write without a held lease, regardless of storage backend:

```
$ lazyspec update RFC-042 --set-status accepted
Error: RFC-042 is not claimed. Run `lazyspec claim RFC-042` first.
```

> [!NOTE]
> For `filesystem` documents, direct file edits (`vim docs/rfcs/RFC-042.md`) bypass the gate because git can't enforce ref-based leases on working tree files. `lazyspec validate` detects unclaimed modifications and warns. The gate covers all writes through `lazyspec` CLI and TUI.

#### Lease Operations

All operations use `GitRefOps` for ref manipulation. Creating a lease commit means writing `lease.json` as a blob, building a single-entry tree, creating a commit (parented on the previous lease commit if extending), and updating the ref. All operations fetch from remote before acting to ensure they operate on the latest state.

| Operation | Mechanism |
|-----------|-----------|
| Acquire | Fetch from remote. `create_ref_commit` with `lease.json`, push to `refs/lazyspec/leases/{type}/{id}`. Uses CAS (all-zeros SHA) to fail if ref already exists on remote. |
| Release | Fetch from remote. `delete_remote_ref` on `refs/lazyspec/leases/{type}/{id}`. Verifies caller is the holder. |
| Admin release | Fetch from remote. Delete lease ref, bypassing expiry. Requires `--expected-holder` matching current holder. For orchestrators. |
| Heartbeat | Fetch from remote. New lease commit with updated expiry, parented on current. `update_ref` with CAS (old SHA), then push. |
| Force-acquire | Fetch from remote. Check `now > lease.expires + grace_period` using commit timestamps for expiry reference. If expired, atomic ref swap via `push --force-with-lease`. |
| Query | Fetch from remote. `lazyspec leases` lists all held leases via `list_refs` on `refs/lazyspec/leases/*`. |

#### Heartbeat and Lease Management

The CLI is stateless. Heartbeat is caller-driven:

- _Claude Code hooks_: a `post-tool-use` hook runs `lazyspec heartbeat` after each tool invocation, extending the lease while the agent is active. Session start hook claims, session end hook releases.
- _Orchestrators_: run `lazyspec heartbeat <doc> --agent-id <id>` on a timer.
- _TUI_: heartbeats held leases on its poll interval.
- _Humans_: don't use the iteration workflow. For spec edits, set a long lease (`60m`+).

Default lease duration is 60 minutes. Grace period for force-acquire is 2 minutes (absorbs NTP drift).

### Git-Ref Backend

Documents with `store = "git-ref"` are stored as commit chains under `refs/lazyspec/{type}/{id}`:

```
refs/lazyspec/iteration/042   → commit chain containing ITERATION-042.md
refs/lazyspec/iteration/043   → commit chain containing ITERATION-043.md
```

Each ref points at a commit whose tree contains the document markdown. Updates create new commits parented on the previous, giving per-document history. `GitRefOps::update_ref` uses the three-argument CAS form (`git update-ref <ref> <new> <old>`) to prevent concurrent overwrites.

Git-ref documents are invisible to:
- `git log` (commits are in custom refs, not branch history)
- `git diff` / PRs (not in the working tree)
- IDE file trees

Git-ref documents are visible to:
- `lazyspec list iteration` (reads from refs)
- `lazyspec show ITERATION-042` (reads from refs, resolves `@ref` directives)
- `lazyspec context ITERATION-042` (shows full chain across backends)
- The TUI (displays alongside filesystem documents)

### Local Shadow Cache

Git-ref documents live in refs, not in the working tree. Reading a ref requires multiple git CLI calls (`resolve_ref`, `read_ref_blob`) on every access. For CLI commands that scan all documents (`list`, `search`, `status`, `validate`), this adds up. The TUI re-reads on every poll cycle.

A local shadow cache materializes git-ref documents into `.lazyspec/cache/{type}/{id}.md`, giving the engine a fast filesystem read path identical to how it reads `filesystem`-backend documents. The cache is gitignored and read-only.

#### Cache structure

```
.lazyspec/
  cache/
    iteration/
      ITERATION-042.md
      ITERATION-043.md
  cache.lock
```

`cache.lock` is a JSON file mapping each cached document to the ref SHA it was materialized from:

```json
{
  "iteration/042": "a1b2c3d4...",
  "iteration/043": "e5f6g7h8..."
}
```

#### Sync: explicit fetch

`lazyspec fetch` updates local refs from the remote _and_ rematerializes the cache in one operation:

1. `GitRefOps::fetch_refs(root, remote, "refs/lazyspec/*")`
2. For each ref whose SHA differs from `cache.lock`: read the blob from the ref's commit tree, write it to `.lazyspec/cache/{type}/{id}.md`, update `cache.lock`
3. Remove cache files for refs that no longer exist on the remote

Between fetches, reads hit the cache directory. The cache may be stale relative to the remote, which is the same trade-off git makes with branches. `lazyspec setup` runs an initial fetch-and-materialize for new clones.

#### Read path

The unified document engine reads `git-ref` types from `.lazyspec/cache/{type}/`, the same cache path used by `github-issues` types. `Store::load_with_fs` already dispatches on `StoreBackend` to select the read directory:

```rust
// existing pattern in store.rs
let full_path = match type_def.store {
    StoreBackend::Filesystem => root.join(&type_def.dir),
    StoreBackend::GithubIssues => root.join(".lazyspec/cache").join(&type_def.name),
    StoreBackend::GitRef => root.join(".lazyspec/cache").join(&type_def.name),
};
```

From the engine's perspective, cached git-ref documents are just another directory of markdown files. If the cache directory is empty (no fetch yet), the engine falls back to reading refs directly via `GitRefOps`, so the system works without a cache, just slower.

#### Write path

The cache is read-only. All mutations go through `lazyspec create`, `lazyspec update`, or `lazyspec delete`:

1. The CLI creates a new blob, tree, and commit on the ref via `GitRefOps::create_ref_commit`
2. `GitRefOps::update_ref` with CAS (old SHA from `cache.lock`) prevents concurrent overwrites
3. The CLI rematerializes the affected cache file and updates `cache.lock`
4. `GitRefOps::push_ref` pushes the ref to the remote

Because the cache is never edited directly, there is no risk of orphaned edits being silently overwritten on the next fetch. Agents and humans interact with git-ref documents exclusively through the CLI or TUI.

#### Init and .gitignore

`lazyspec init` adds `.lazyspec/cache/` to `.gitignore` when any type uses `store = "git-ref"`. The cache directory is created on first fetch.

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

The read side (`Store::load_with_fs`) already dispatches on `StoreBackend` to select the directory. Adding `GitRef` extends the match arm to read from `.lazyspec/cache/{type}/`, identical to `GithubIssues`. No new read-path abstraction is needed.

The write side uses `dispatch_for_type`, which currently takes `&mut FilesystemStore` and `Option<&mut GithubIssuesStore<G>>` as separate arguments. Adding a third `Option<&mut GitRefStore<R>>` parameter is workable but signals that the dispatch mechanism should evolve. The pragmatic path: add the third parameter for now, note it as tech debt. A future refactor could replace the positional arguments with a store registry, but that's not justified until there's a fourth backend.

@ref src/engine/store_dispatch.rs#dispatch_for_type

`lazyspec list`, `lazyspec search`, `lazyspec show`, `lazyspec validate`, `lazyspec context`, and `lazyspec status` all operate across backends. The TUI's document tree merges all sources.

`lazyspec validate` runs the same rules across backends: "iterations need stories" checks that an iteration has an `implements` link to a story, regardless of where each lives.

### Agent Identity

Priority chain:

1. `$LAZYSPEC_AGENT_ID` (explicit, for orchestrators)
2. `$CLAUDE_SESSION_ID` (auto-detected in Claude Code)
3. `git config user.name` (fallback)

### Claude Code Hooks

Claude Code hooks automate the coordination lifecycle:

```json
// .claude/settings.json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec claim \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec heartbeat \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --min-interval 15m --json || true"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec release \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ]
  }
}
```

The orchestrator sets `$ASSIGNED_TASK` when spawning the agent. The hooks handle claim, heartbeat, and release without the agent needing to know about coordination. The `[ -n "$ASSIGNED_TASK" ]` guard makes the snippet a no-op when the env var is unset (safe to install unconditionally); `|| true` swallows non-zero exits so a session never fails to end on a coordination error. `--min-interval 15m` matches the default `lease_duration / 4` (lease default 60m) -- tune if `lease_duration` changes.

See README § Claude Code Hooks and [`hooks/claude-code-settings.json`](../../hooks/claude-code-settings.json) for the canonical snippet.

### Git Ref Operations Trait

All git ref operations shell out to the `git` CLI behind a trait, following the same pattern as `GhIssueReader`/`GhIssueWriter` in `gh.rs`. A `GitRefOps` trait defines the I/O boundary; `GitCli` is the real implementation, tests inject a mock.

```rust
pub trait GitRefOps {
    fn resolve_ref(&self, root: &Path, refname: &str) -> Result<Option<String>>;
    fn list_refs(&self, root: &Path, pattern: &str) -> Result<Vec<(String, String)>>;
    fn read_ref_blob(&self, root: &Path, sha: &str, path: &str) -> Result<String>;
    fn create_ref_commit(&self, root: &Path, refname: &str, files: &[(&str, &str)]) -> Result<String>;
    fn update_ref(&self, root: &Path, refname: &str, new_sha: &str, old_sha: &str) -> Result<()>;
    fn delete_ref(&self, root: &Path, refname: &str) -> Result<()>;
    fn fetch_refs(&self, root: &Path, remote: &str, pattern: &str) -> Result<()>;
    fn push_ref(&self, root: &Path, remote: &str, refname: &str) -> Result<()>;
    fn push_ref_with_lease(&self, root: &Path, remote: &str, refname: &str, expected_old: Option<&str>) -> Result<()>;
    fn delete_remote_ref(&self, root: &Path, remote: &str, refname: &str) -> Result<()>;
    fn read_commit_timestamp(&self, root: &Path, sha: &str) -> Result<DateTime<Utc>>;
}
```

The `create_ref_commit` implementation pipes content through `git hash-object -w --stdin`, builds a tree with `git mktree`, creates a commit with `git commit-tree`, and points the ref with `git update-ref`. This is the same sequence the reservation module uses for ref-based number claims.

@ref src/engine/reservation.rs#reserve_next
@ref src/engine/gh.rs#GhIssueReader

This lives in a new `engine/git_ref.rs` module alongside `gh.rs`, following the same structure: data types, trait definition, `GitCli` implementation, `MockGitRefClient` in a `test_support` submodule.

The existing reservation module can be migrated to use `GitRefOps` in a follow-up, giving the trait a second concrete consumer.

### Distributed Safety Properties

The distributed lease protocol relies on the following safety properties:

- **Linearization point at the remote**: Every lease mutation (`acquire`, `heartbeat`, `release`, `force_acquire`) writes to the remote ref with explicit CAS via `git push --force-with-lease=ref:<expected_old>` before the local ref advances. The local `git update-ref <ref> <new> <old>` is a follow-up cache update, not the linearization point. Operations push first, then update local on success; local never advances past what the remote accepted.
- **Fetch-before-check (glob, with prune)**: `acquire`, `heartbeat`, `release`, `force_acquire` all fetch `refs/lazyspec/leases/{type}/*` with `--prune` before reading local state. Single-ref fetches leave stale local refs surviving when their remote counterparts have been deleted; glob fetch with prune clears them. The fetch is best-effort: a missing remote ref is benign (treated as "no lease"), other errors propagate.
- **Clock skew tolerance**: The `grace_period` (default 2m) absorbs NTP drift between honest agents. Force-acquire computes expiry from the commit's committer timestamp; that timestamp is client-written and baked into the commit SHA, so it cannot be rewritten server-side. To bound adversarial or pathological skew, `force_acquire` also rejects leases whose commit timestamp is more than `max_clock_skew` (default 5m) ahead of the caller's local clock. Operationally, agents must be reasonably NTP-synchronized: split-brain is reachable iff `|Δ_clocks| > duration + grace_period`.
- **Network partition behavior**: Lease mutations require the remote (fetch failures and push failures both abort the operation). The implementation does not silently fall back to local-only writes — a heartbeat under partition returns `Err` rather than advancing local state; an acquire under partition fails before producing any commit. The read-only `query()` is the one exception: a failed fetch is logged and the cached local view is returned. The agent layer is expected to handle these errors (retry with backoff, surface to the daemon, etc.) rather than the engine guessing a partition-tolerance policy.
- **Initial ref creation**: `acquire` pushes with `--force-with-lease=ref:0000000000000000000000000000000000000000`, requiring the remote ref to be absent. Two agents racing both see "ref does not exist" locally; only one push lands first and creates the ref, the other's CAS expectation (`expected_old = zero`) no longer matches and the push is rejected with a clear stale-info error.

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
    max_clock_skew: String,   // default "5m" - bound on committer-date trust during force_acquire
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
max_clock_skew = "5m"
```

### Graceful Degradation

If the remote is unreachable:
- _Claim, release, heartbeat_: fail. Coordination requires a remote.
- _Git-ref create/update_: local ref commit succeeds. Push fails but document is readable locally.
- _Git-ref read_: works from the shadow cache (as fresh as the last successful fetch). Falls back to locally fetched refs if cache is cold.
- _Filesystem reads_: always work.
- _Filesystem writes (via lazyspec)_: fail if lease check requires remote.

### Future Backends

The `store` field is an enum that can grow. `github-issues` was added in RFC-037. Potential future backends:

- `sqlite`: local database for fast queries, bulk operations, and offline-first workflows. No remote coordination, but useful for large projects where filesystem scanning is slow.

Each backend implements `DocumentStore` (create, update, delete) and uses `.lazyspec/cache/{type}/` for non-filesystem reads. The document model, relationships, and validation are backend-agnostic.

## Stories

1. `GitRefOps` trait and lease engine -- `GitRefOps` trait in `engine/git_ref.rs` (data types, trait, `GitCli` impl, `MockGitRefClient`). Lease CRUD on `refs/lazyspec/leases/{type}/{id}` using commit objects via `GitRefOps`. Acquire, release, admin-release, heartbeat, force-acquire with grace period, query. Agent identity resolution. `lazyspec claim/release/leases/heartbeat` CLI subcommands with `--json`. Lease-gate enforcement on writes via lazyspec (all backends).

2. Git-ref storage backend -- `StoreBackend::GitRef` variant. `GitRefStore<R: GitRefOps>` implementing `DocumentStore` (create/update/delete via commit-chain CRUD on `refs/lazyspec/{type}/{id}`). Shadow cache in `.lazyspec/cache/{type}/`. `Store::load_with_fs` dispatch for `GitRef`. `dispatch_for_type` extended with third parameter. `lazyspec fetch` materializes cache. Extend `list/show/search/validate/context/status` to operate across all three backends.

3. Init, setup, and config -- `store = "git-ref"` on `TypeDef`. `[coordination]` config section. `lazyspec init` wizard with store selection. `lazyspec setup` for new clones. Fetch refspec management. `.gitignore` for `.lazyspec/cache/` when git-ref types configured. Shallow clone detection. Shared remote validation with `[numbering.reserved]`.

4. TUI integration -- display git-ref documents alongside filesystem documents. Lease status indicators. Claim/release from TUI. Heartbeat on poll. Context chain display across backends (RFC -> Story -> Iteration). Status filtering.

5. Claude Code hooks -- hook definitions for session-start (claim), post-tool-use (heartbeat), session-end (release). Documentation for orchestrator integration. `$ASSIGNED_TASK` convention.
