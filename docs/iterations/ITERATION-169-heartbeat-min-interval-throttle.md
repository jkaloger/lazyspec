---
title: Heartbeat --min-interval throttle
type: iteration
status: accepted
author: agent
date: 2026-05-11
tags:
- leasing
- cli
related:
- implements: STORY-120
---

## Context

STORY-120 ACs 4-7. Hook fires per tool call. Full heartbeat = fetch+commit+push every call. Need throttle so hook stays one-liner.

State file `.lazyspec/state/heartbeat-{type}-{id}`. Contents: RFC3339 UTC. Atomic write (temp+rename). One file per task. Gitignored.

Out of slice: hook scripts (ACs 1-3, 8-9). Separate iteration.

## Changes

1. **Add `--min-interval` flag to `Commands::Heartbeat`** — `src/cli.rs:323-333`. Field `min_interval: Option<String>` after `agent_id`. Doc: "Skip heartbeat if last run within duration (e.g. 15m). State in .lazyspec/state/".

2. **Plumb flag in main.rs** — `src/main.rs:443-462`. Add `min_interval` to destructure + pass through to `run_heartbeat`.

3. **Throttle logic in `run_heartbeat`** — `src/cli/lease.rs:203-226`. Signature gains `min_interval: Option<&str>`. Before engine call:
   - If `min_interval` None → run unconditionally (preserves AC6).
   - Parse via `crate::engine::lease::parse_duration`.
   - State path: `root.join(".lazyspec/state").join(format!("heartbeat-{}-{}", type_name, doc_id))`.
   - Read file if exists. Parse RFC3339. If `now - last < interval` → skip path: JSON emits `{"skipped": true, "reason": "throttled", "last": "<ts>"}`, non-JSON prints `Heartbeat ITERATION-042 skipped (throttled, last <ts>)`. Return Ok.
   - Else run engine heartbeat. On success: ensure dir, write current `Utc::now().to_rfc3339()` to temp file, rename.
   - State file write failure → log warn to stderr, don't fail the operation.

4. **Gitignore `.lazyspec/state/`** — `src/cli/init.rs:69`. Extend `GITIGNORE_ENTRIES` with `.lazyspec/state/`. Predicate at line 72 must trigger when EITHER github-issues OR coordination is configured. Add `has_coordination()` or use `config.coordination.is_some()`. Simplest: gate the gitignore write on `has_github_issues_types() || config.coordination.is_some()`.

5. **Unit tests** — `src/cli/lease.rs` test mod. New tests below.

## Test Plan

Test seam: `run_heartbeat` already injects `LeaseEngine<R: GitRefOps>` via `require_coordination`. For state-file tests, use `TempDir` for root; production `GitCli` not exercised. Engine path mocked through `MockGitRefClient` (existing pattern, lease.rs:236).

Refactor: extract `run_heartbeat_with<R: GitRefOps>(root, config, engine, doc_id, agent_id, min_interval, json)` mirroring `check_lease_gate_with`. Public `run_heartbeat` builds real engine and delegates. Lets tests pass a mock engine and a real `TempDir`.

Tests (unit, `#[cfg(test)] mod tests` in lease.rs):

1. `heartbeat_without_min_interval_runs_unconditionally` — AC6. No state file. Mock engine returns lease. Assert engine called.

2. `heartbeat_skips_when_last_run_within_interval` — AC4. Pre-seed `.lazyspec/state/heartbeat-iteration-ITERATION-042` with `now - 5m`. min_interval=15m. Assert: engine NOT called (mock records zero heartbeat calls), JSON output contains `"skipped":true`.

3. `heartbeat_runs_when_state_file_older_than_interval` — AC5. Pre-seed state with `now - 30m`. min_interval=15m. Assert engine called, state file timestamp updated to ~now.

4. `heartbeat_runs_when_state_file_absent` — AC5. No state file. min_interval=15m. Engine called. State file created with current timestamp.

5. `heartbeat_state_file_written_atomically` — assert no `.tmp` residue after run; final file present.

6. `heartbeat_state_write_failure_does_not_fail_command` — chmod state dir read-only, run heartbeat. Engine call succeeds, command exits 0, stderr warns. (Skip on platforms where chmod doesn't apply; gate on `#[cfg(unix)]`.)

7. `heartbeat_skipped_path_emits_json_skipped_true` — AC4 JSON shape exactness.

Gitignore tests in `src/cli/init.rs`:

8. `gitignore_includes_state_when_coordination_configured` — config with coordination present, no github-issues types. Assert `.lazyspec/state/` line in output.

9. `gitignore_includes_state_when_git_ref_type_configured` — git-ref type + coordination, no github-issues. Same assertion.

10. `gitignore_omits_state_when_no_coordination_and_no_github_issues` — filesystem-only, no coordination. No `.lazyspec/state/` entry.

All tests: `tempfile::TempDir`, fixed timestamps (no `Utc::now()` in assertions; inject via test helper that overrides via param). For throttle, take `now: DateTime<Utc>` as a parameter on the testable helper.

## Notes

- State filename uses `{type}-{id}` to avoid collision if two task ids ever share a prefix across types. Cheap; if `id` already encodes type (e.g. `ITERATION-042`), the type prefix is redundant but harmless.
- `parse_duration` already exists in `src/engine/lease.rs:18`. Reuse.
- Atomic write pattern: `tempfile::NamedTempFile::new_in(state_dir)?.persist(target)?` or manual temp+rename. Pick the simpler one.
- AC4 says JSON output `{"skipped": true, "reason": "throttled"}`. Engine `Lease` serialization is unaffected on run-path; only skip-path emits this shape. CLI must distinguish at the `println!` site.
