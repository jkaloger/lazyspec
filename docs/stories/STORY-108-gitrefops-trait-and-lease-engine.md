---
title: GitRefOps trait and lease engine
type: story
status: accepted
author: jkaloger
date: 2026-04-01
tags: []
related:
- implements: RFC-035
---



## Context

RFC-035 introduces git-ref-based document storage with lease-based coordination between lazyspec agents. This story covers the foundational layer: the `GitRefOps` trait abstracting git ref manipulation, the lease engine built on top of it, agent identity resolution, CLI subcommands for lease management, and lease-gate enforcement on writes.

## Acceptance Criteria

### GitRefOps trait and GitCli implementation

- **Given** a new module `engine/git_ref.rs`
  **When** the module is compiled
  **Then** it exports a `GitRefOps` trait with methods: `resolve_ref`, `list_refs`, `read_ref_blob`, `create_ref_commit`, `update_ref`, `delete_ref`, `fetch_refs`, `push_ref`, `delete_remote_ref`

- **Given** the `GitCli` struct implementing `GitRefOps`
  **When** `create_ref_commit` is called with a refname and file contents
  **Then** it creates an orphan commit containing the given files and points the ref at it

- **Given** the `GitCli` struct implementing `GitRefOps`
  **When** `update_ref` is called with a new SHA and old SHA
  **Then** it performs a compare-and-swap update, failing if the current ref doesn't match old SHA

- **Given** a `MockGitRefClient` in a `test_support` submodule
  **When** used in tests
  **Then** it records calls and returns configurable responses, following the same pattern as `GhIssueReader`/`GhIssueWriter` mocks

### Lease acquire/release/heartbeat/force-acquire/query

- **Given** no existing lease for a document
  **When** an agent calls acquire on `refs/lazyspec/leases/{type}/{id}`
  **Then** a commit containing `lease.json` with agent, acquired timestamp, and expires timestamp is created and pushed to the remote

- **Given** a lease already exists for a document held by another agent
  **When** an agent calls acquire
  **Then** the operation fails with an error indicating the lease is held

- **Given** an agent holds a lease
  **When** the agent calls release
  **Then** the lease ref is deleted from the remote

- **Given** an agent holds a lease
  **When** another agent calls release without `--expected-holder` matching the current holder
  **Then** the release is rejected

- **Given** an agent holds a lease
  **When** admin-release is called with `--expected-holder` matching the current holder
  **Then** the lease ref is deleted regardless of expiry

- **Given** an agent holds a lease
  **When** heartbeat is called
  **Then** a new commit with an updated expiry is created, parented on the current lease commit, and pushed with CAS

- **Given** a lease that has expired beyond the grace period
  **When** an agent calls force-acquire
  **Then** the expired lease ref is deleted and a new lease is acquired

- **Given** a lease that has expired but is still within the grace period
  **When** an agent calls force-acquire
  **Then** the operation fails

- **Given** multiple leases exist across different documents
  **When** query (list) is called via `list_refs` on `refs/lazyspec/leases/*`
  **Then** all held leases are returned with their agent, acquired, and expires fields

### Agent identity resolution

- **Given** `$LAZYSPEC_AGENT_ID` is set
  **When** agent identity is resolved
  **Then** the value of `$LAZYSPEC_AGENT_ID` is used

- **Given** `$LAZYSPEC_AGENT_ID` is not set but `$CLAUDE_SESSION_ID` is set
  **When** agent identity is resolved
  **Then** the value of `$CLAUDE_SESSION_ID` is used

- **Given** neither `$LAZYSPEC_AGENT_ID` nor `$CLAUDE_SESSION_ID` is set
  **When** agent identity is resolved
  **Then** `git config user.name` combined with a sqids-encoded PID is used

### CLI subcommands (claim, release, leases, heartbeat) with --json

- **Given** the `lazyspec claim` subcommand
  **When** called with a document path and `--json`
  **Then** a lease is acquired and the lease details are output as JSON

- **Given** the `lazyspec release` subcommand
  **When** called with a document path and `--json`
  **Then** the held lease is released and confirmation is output as JSON

- **Given** the `lazyspec leases` subcommand
  **When** called with `--json`
  **Then** all held leases are listed as JSON

- **Given** the `lazyspec heartbeat` subcommand
  **When** called with a document path and `--json`
  **Then** the lease expiry is extended and the updated lease is output as JSON

- **Given** any lease CLI subcommand
  **When** the operation fails (e.g., lease held by another agent)
  **Then** a structured JSON error with a clear message is returned and the exit code is non-zero

### Lease-gate enforcement on writes

- **Given** coordination is configured
  **When** `lazyspec create`, `lazyspec update`, or `lazyspec delete` is called without a held lease
  **Then** the write is refused with an error indicating a lease is required

- **Given** coordination is configured and the agent holds a lease
  **When** `lazyspec create`, `lazyspec update`, or `lazyspec delete` is called
  **Then** the write proceeds normally

- **Given** coordination is not configured
  **When** `lazyspec create`, `lazyspec update`, or `lazyspec delete` is called
  **Then** writes proceed without lease checks (backward compatible)

### Coordination config parsing

- **Given** a `[coordination]` section in the lazyspec config
  **When** the config is parsed
  **Then** `remote`, `lease_duration`, `grace_period`, and `max_push_retries` are read with defaults (`"origin"`, `"60m"`, `"2m"`, `5`)

- **Given** no `[coordination]` section in the config
  **When** the config is parsed
  **Then** coordination is treated as disabled and lease-gate enforcement is skipped

## Scope

### In Scope

- `GitRefOps` trait in `engine/git_ref.rs` (data types, trait, `GitCli` impl, `MockGitRefClient`)
- Lease CRUD on `refs/lazyspec/leases/{type}/{id}` using commit objects via `GitRefOps`
- Acquire, release, admin-release, heartbeat, force-acquire with grace period, query
- Agent identity resolution (`$LAZYSPEC_AGENT_ID`, `$CLAUDE_SESSION_ID`, git config fallback)
- `lazyspec claim/release/leases/heartbeat` CLI subcommands with `--json`
- Lease-gate enforcement on writes via lazyspec (all backends)
- `[coordination]` config section parsing

### Out of Scope

- Git-ref storage backend (commit-chain document CRUD) -- Story 2
- Shadow cache and `lazyspec fetch` for git-ref backend -- Story 2
- `StoreBackend::GitRef` variant -- Story 2
- TUI integration for lease status -- Story 4
- Claude Code hooks -- Story 5
- `lazyspec init`/`lazyspec setup` wizard -- Story 3
