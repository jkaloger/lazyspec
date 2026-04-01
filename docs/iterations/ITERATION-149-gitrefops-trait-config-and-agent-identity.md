---
title: GitRefOps trait, config, and agent identity
type: iteration
status: accepted
author: agent
date: 2026-04-01
tags: []
related:
- implements: STORY-108
---


## Context

First of two iterations against STORY-108. Establishes the engine-layer foundation that Iteration B (lease engine, CLI, write gate) builds on: the `GitRefOps` trait abstracting git ref manipulation, coordination config parsing, and agent identity resolution.

## Acceptance Criteria Addressed

From STORY-108:
- AC Group 1: GitRefOps trait and GitCli implementation (all 4 criteria)
- AC Group 6: Coordination config parsing (both criteria)
- AC Group 3: Agent identity resolution (all 3 criteria)

## Changes

1. **Create `src/engine/git_ref.rs` — GitRefOps trait and GitCli implementation**
   - ACs: Group 1 (all 4 criteria)
   - Define `GitRefOps` trait with methods: `resolve_ref`, `list_refs`, `read_ref_blob`, `create_ref_commit`, `update_ref`, `delete_ref`, `fetch_refs`, `push_ref`, `delete_remote_ref`
   - Implement `GitCli` struct implementing `GitRefOps` using git CLI commands, following the pattern in `src/engine/reservation.rs:146-197` (hash-object, mktree, commit-tree, update-ref pipeline)
   - `create_ref_commit`: pipe content through `git hash-object -w --stdin`, build tree with `git mktree`, create commit with `git commit-tree`, point ref with `git update-ref`
   - `update_ref`: three-argument CAS form (`git update-ref <ref> <new> <old>`)
   - `push_ref` / `delete_remote_ref`: `git push` with appropriate refspecs
   - `fetch_refs`: `git fetch <remote> <pattern>`
   - `list_refs`: `git for-each-ref` or `git ls-remote` depending on local/remote context
   - `read_ref_blob`: `git show <sha>:<path>` to extract file content from a commit
   - `resolve_ref`: `git rev-parse` on the refname
   - `delete_ref`: `git update-ref -d <refname>`
   - Create `MockGitRefClient` in a `pub mod test_support` submodule (gated behind `#[cfg(any(test, feature = "test-support"))]`), following `src/engine/gh.rs:464-653` MockGhClient pattern: struct with `RefCell`/`Cell` fields for recording calls and returning configurable responses, builder methods for test setup
   - Register module in `src/engine/mod.rs`
   - Verification: unit tests against MockGitRefClient confirm trait contract; integration test using `TestFixture::with_git_remote()` from `tests/common/mod.rs` exercises GitCli against a real bare repo

2. **Add `[coordination]` config section to `src/engine/config.rs`**
   - ACs: Group 6 (both criteria)
   - Define `CoordinationConfig` struct with fields: `remote: String` (default `"origin"`), `lease_duration: String` (default `"60m"`), `grace_period: String` (default `"2m"`), `max_push_retries: u8` (default `5`)
   - Add `RawCoordination` struct for TOML deserialization, following the pattern of `RawNumbering` at `src/engine/config.rs:238-242`
   - Add `coordination: Option<RawCoordination>` field to `RawConfig`
   - Parse in `Config::parse()` at `src/engine/config.rs:419`, storing as `Option<CoordinationConfig>` on `DocumentConfig` or `Config`
   - When `[coordination]` section is absent, the field is `None` — downstream code treats this as "coordination disabled"
   - Verification: unit test parsing a TOML string with `[coordination]` section produces correct `CoordinationConfig`; unit test parsing TOML without the section produces `None`

3. **Add agent identity resolution to `src/engine/git_ref.rs` (or a new `src/engine/agent.rs`)**
   - ACs: Group 3 (all 3 criteria)
   - Implement `resolve_agent_id()` function with priority chain: `$LAZYSPEC_AGENT_ID` → `$CLAUDE_SESSION_ID` → `git config user.name` + sqids-encoded PID
   - For the sqids encoding: use the `sqids` crate (add to Cargo.toml) to encode the PID as a short alphanumeric string, appended to git user.name with a separator (e.g., `jack-a3b`)
   - The function takes `repo_root: &Path` as parameter (needed for `git config user.name` fallback)
   - Verification: unit tests with env vars set/unset confirming each priority level; integration test for git config fallback using a TestFixture with configured user.name

## Test Plan

### GitRefOps trait (unit tests in `src/engine/git_ref.rs`)
- Test MockGitRefClient records calls correctly (arrange mock, call trait method, assert recorded call)
- Test MockGitRefClient returns configured responses

### GitRefOps GitCli (integration tests in `tests/`)
- Test `create_ref_commit` creates an orphan commit with correct tree contents in a real bare repo
- Test `update_ref` with correct old SHA succeeds; with wrong old SHA fails (CAS semantics)
- Test `resolve_ref` returns SHA for existing ref, None for missing ref
- Test `list_refs` returns matching refs with correct pattern filtering
- Test `read_ref_blob` extracts file content from a commit
- Test `delete_ref` removes the ref
- Test `push_ref` and `fetch_refs` against a bare remote (using TestFixture::with_git_remote)

### Coordination config (unit tests in `src/engine/config.rs`)
- Test TOML with `[coordination]` section parses all fields with explicit values
- Test TOML with `[coordination]` section using only defaults
- Test TOML without `[coordination]` section produces `None`
- Test invalid values in `[coordination]` produce parse errors

### Agent identity (unit tests)
- Test with `$LAZYSPEC_AGENT_ID` set: returns that value
- Test with only `$CLAUDE_SESSION_ID` set: returns that value
- Test with neither set: returns git config user.name + sqids PID
- Test with neither set and no git config: appropriate error or fallback

## Notes

- The `GitRefOps` trait has one concrete implementation (`GitCli`) today, which would normally argue against a trait per Principle 6. However, the RFC explicitly calls for `MockGitRefClient` for testing, and the reservation module is identified as a future second consumer. The trait is justified by the I/O boundary principle (Principle 4).
- The reservation module (`src/engine/reservation.rs`) already does git ref manipulation inline. A follow-up iteration can migrate it to use `GitRefOps`, giving the trait its second concrete consumer.
- Agent identity is small enough to live in `git_ref.rs` initially. If it grows, extract to `agent.rs`.
