---
title: Core reservation git plumbing
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-068-core-reservation-mechanism.md
---



## Changes

### Task 1: Reservation module with git plumbing

**ACs addressed:** AC-1 (successful reservation), AC-2 (retry on conflict), AC-3 (exhausted retries), AC-4 (unreachable remote)

**Files:**
- Create: `src/engine/reservation.rs`
- Modify: `src/engine/mod.rs` (add `pub mod reservation;`)

**What to implement:**

A `reservation` module that wraps the four git plumbing operations needed for atomic number reservation. The module exposes a single public function:

```rust
pub fn reserve_next(
    repo_root: &Path,
    remote: &str,
    prefix: &str,
    max_retries: u8,
) -> Result<u32>
```

Internal flow:

1. `ls_remote(repo_root, remote, prefix) -> Result<Vec<u32>>` -- runs `git ls-remote --refs {remote} "refs/reservations/{prefix}/*"`, parses ref names to extract the integer suffix from each. Returns an empty vec if the remote is unreachable, but the caller distinguishes "no refs" from "unreachable" by checking the git exit code. If git fails with a non-zero exit and stderr contains connection/auth indicators, return a `ReservationError::RemoteUnreachable` with the remote name and stderr snippet.

2. `create_local_ref(repo_root, prefix, num) -> Result<()>` -- runs `git hash-object -t blob --stdin < /dev/null` to get an empty blob SHA, then `git update-ref "refs/reservations/{prefix}/{num}" {sha}`.

3. `push_ref(repo_root, remote, prefix, num) -> Result<bool>` -- runs `git push {remote} "refs/reservations/{prefix}/{num}"`. Returns `Ok(true)` on success, `Ok(false)` if the push is rejected (ref already exists on remote), and `Err` for other failures.

4. `cleanup_local_ref(repo_root, prefix, num) -> Result<()>` -- runs `git update-ref -d "refs/reservations/{prefix}/{num}"`.

The `reserve_next` function orchestrates:
- Call `ls_remote`. On `RemoteUnreachable`, return an error suggesting `--numbering incremental` or `--numbering sqids` as overrides.
- Find the max existing number, start candidate at max + 1.
- Loop up to `max_retries` times: `create_local_ref` -> `push_ref`. If push succeeds, return the number. If push fails (conflict), `cleanup_local_ref`, increment candidate, continue.
- If all retries exhausted, return a clear error with the number of attempts and the prefix.

Use `std::process::Command` for git, consistent with `src/engine/refs.rs`.

**How to verify:**
```
cargo test reservation
```

---

### Task 2: Integrate reservation into the create command

**ACs addressed:** AC-1 (end-to-end flow), AC-5 (pre-computed ID passthrough)

**Files:**
- Modify: `src/cli/create.rs`

**What to implement:**

Replace the local ID computation in the `NumberingStrategy::Reserved` match arm (lines 31-46 of `create.rs`) with a call to `reservation::reserve_next`. The reserved number is then formatted according to `reserved_cfg.format`:

- `ReservedFormat::Incremental` -- `format!("{:03}", num)`
- `ReservedFormat::Sqids` -- encode the raw `u32` through sqids using the existing `SqidsConfig`, producing a lowercase string

The formatted string is passed as `pre_computed_id` to `resolve_filename`, exactly as the current code does. The `resolve_filename` function and template layer require no changes.

The `repo_root` needed by `reserve_next` is the `root` parameter already available in `create::run`. The `remote`, `prefix`, and `max_retries` come from `ReservedConfig` and `TypeDef`.

**How to verify:**
```
cargo test cli_create
cargo build
```

---

### Task 3: Unit and integration tests for reservation

**ACs addressed:** AC-1 through AC-5

**Files:**
- Create: `tests/reservation_test.rs`
- Modify: `tests/common/mod.rs` (add git repo init helper)

**What to implement:**

Tests require a real git repo with a remote to exercise the plumbing. Add a helper to `TestFixture`:

```rust
pub fn with_git_remote() -> (Self, TempDir)
```

This creates a bare git repo in a second `TempDir`, initialises a git repo in the fixture root, and adds the bare repo as `origin`. This gives tests a local "remote" to push refs to.

Tests:

1. **Successful reservation** (AC-1): Configure reserved numbering, call `reserve_next`. Assert it returns 1 (no existing refs). Verify the ref exists on the remote via `git ls-remote`.

2. **Incremented from existing** (AC-1): Manually push `refs/reservations/RFC/3` to the remote. Call `reserve_next`. Assert it returns 4.

3. **Retry on conflict** (AC-2): After `ls_remote`, manually push a ref for the candidate number before the test's `push_ref` executes. This simulates a race. Verify that `reserve_next` retries and returns the next number. (Implementation: push a conflicting ref between calling `ls_remote` and calling `reserve_next`, so the first push attempt fails.)

4. **Exhausted retries** (AC-3): Push refs for numbers 1 through `max_retries + 1` to the remote. Call `reserve_next` with `max_retries = 1`. Assert it returns an error mentioning retry exhaustion. Assert no document file was created.

5. **Unreachable remote** (AC-4): Call `reserve_next` with `remote = "nonexistent"`. Assert the error mentions remote access and suggests `--numbering incremental` or `--numbering sqids`.

6. **Pre-computed ID passthrough** (AC-5): Call `resolve_filename` with a `pre_computed_id` of `"042"`. Assert the filename uses `042` verbatim. (This is a template-layer test confirming the contract; the existing tests in `template.rs` partially cover this but an explicit reserved-format test is worth adding.)

7. **Cleanup on failure** (AC-2): After a failed push attempt, verify the local ref was cleaned up (doesn't exist in `.git/refs/reservations/`).

**Tradeoffs:** These tests shell out to real git, sacrificing Fast for Predictive. The alternative (mocking git commands) would be fast but not predictive of real git behaviour, which is the whole point of this feature. Each test creates its own temp dirs, so they remain Isolated and Deterministic.

**How to verify:**
```
cargo test reservation
```

## Test Plan

| # | AC | Test | Property tradeoffs |
|---|-----|------|--------------------|
| 1 | AC-1 | Successful reservation returns correct number, ref exists on remote | Predictive over Fast (real git) |
| 2 | AC-1 | Increments past existing reservations | Deterministic |
| 3 | AC-2 | Retries when push is rejected, returns next number | Predictive over Fast |
| 4 | AC-3 | Fails with clear error after max retries exhausted | Specific |
| 5 | AC-4 | Fails immediately with actionable error for unreachable remote | Specific |
| 6 | AC-5 | Pre-computed ID passed through to filename unchanged | Isolated, Fast |
| 7 | AC-2 | Local ref cleaned up after failed push | Specific |

## Notes

The git plumbing pattern follows `src/engine/refs.rs` which already uses `std::process::Command::new("git")`. No new dependencies needed.

The retry-on-conflict test (test 3) is the trickiest to make deterministic. The approach is to pre-seed the remote with a ref that collides with the expected first candidate, rather than trying to simulate a true race condition. This tests the retry logic without flakiness.

`src/cli/fix.rs` already returns `None` for `Reserved` in `renumber_doc` (line 768), which is correct -- reserved numbers are stable and should not be renumbered during conflict resolution.
