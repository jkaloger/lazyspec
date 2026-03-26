---
title: Progress callback API for reservation functions
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: STORY-071
---




## Changes

### Task 1: Define progress enums and update engine function signatures

**ACs addressed:** AC-1, AC-2, AC-4, AC-5

**Files:**
- Modify: `src/engine/reservation.rs`

**What to implement:**

Add two enums at the top of `reservation.rs`:

```rust
#[derive(Debug, Clone)]
pub enum ReservationProgress {
    QueryingRemote,
    PushAttempt { attempt: u8, max: u8, candidate: u32 },
    PushRejected { candidate: u32 },
    Reserved { number: u32 },
}

#[derive(Debug, Clone)]
pub enum PruneProgress {
    QueryingRemote,
    Deleting { current: usize, total: usize, ref_path: String },
    Done { pruned: usize, orphaned: usize },
}
```

Update `reserve_next` (line 178) to accept `on_progress: impl Fn(ReservationProgress)`. Insert callback invocations:
- `on_progress(ReservationProgress::QueryingRemote)` before the `ls_remote` call (line 185)
- `on_progress(ReservationProgress::PushAttempt { attempt: attempt as u8 + 1, max: max_retries, candidate })` before `push_ref` (line 194)
- `on_progress(ReservationProgress::PushRejected { candidate })` in the `Ok(false)` branch (line 196)
- `on_progress(ReservationProgress::Reserved { number: candidate })` before `return Ok(candidate)` (line 195)

Update `list_reservations` (line 14) to accept `on_progress: impl Fn(ReservationProgress)`. Insert:
- `on_progress(ReservationProgress::QueryingRemote)` before the `Command::new("git")` call (line 15)

**How to verify:**
```
cargo build 2>&1 | head -20
```
Build will fail on caller mismatches (expected -- Task 2 and 3 fix callers).

---

### Task 2: Thread callbacks through CLI callers and prune workflow

**ACs addressed:** AC-3, AC-6, AC-7, AC-8

**Files:**
- Modify: `src/cli/reservations.rs`
- Modify: `src/cli/create.rs`
- Modify: `src/main.rs`

**What to implement:**

In `src/cli/reservations.rs`:
- Update `run_list` (line 32): pass `|_| {}` to `list_reservations`
- Update `run_prune` signature (line 107) to accept `on_progress: impl Fn(PruneProgress)`. Add callback invocations:
  - `on_progress(PruneProgress::QueryingRemote)` before `list_reservations` call (line 118)
  - Pass `|_| {}` to `list_reservations` call (line 118)
  - `on_progress(PruneProgress::Deleting { current: idx + 1, total, ref_path: r.ref_path.clone() })` before each `delete_remote_ref` call (line 133). Compute `total` as the count of reservations with matching local documents before the loop.
  - `on_progress(PruneProgress::Done { pruned: pruned.len(), orphaned: orphaned.len() })` before the JSON output block (before line 161)

In `src/cli/create.rs` (line 35):
- Pass `|_| {}` as the last argument to `reserve_next`

In `src/main.rs`:
- Pass `|_| {}` to `run_prune` call (line 128)

**How to verify:**
```
cargo build 2>&1 | head -20
```
Build will fail on test mismatches only (expected -- Task 3 fixes tests).

---

### Task 3: Update tests and add indicatif dependency

**ACs addressed:** AC-9, AC-10

**Files:**
- Modify: `tests/reservation_test.rs`
- Modify: `Cargo.toml`

**What to implement:**

In `Cargo.toml`, add to `[dependencies]`:
```toml
indicatif = "0.17"
```

In `tests/reservation_test.rs`, update every call to `reserve_next` (lines 61, 76, 111, 128, 142, 365, 381) by appending `|_| {}` as the final argument.

Update every call to `list_reservations` (lines 212, 238) by appending `|_| {}` as the final argument.

The `run_prune` calls (lines 263, 280, 298) need `|_| {}` appended for the new `PruneProgress` callback. The binary-based test at line 325 (`prune_json_output_is_structured`) calls the CLI binary directly and needs no change.

**How to verify:**
```
cargo test 2>&1 | tail -20
```
All existing tests should pass with no-op callbacks.

## Test Plan

The existing test suite in `tests/reservation_test.rs` already covers the reservation and prune workflows thoroughly (12 tests covering success, retry, exhaustion, unreachable remote, cleanup, list, prune, dry-run, JSON output). Passing all of these with no-op callbacks verifies AC-9 (no regression).

AC-10 (indicatif compiles) is verified by a successful `cargo build`.

ACs 1-8 (callback invocations and no-op callers) are structural -- the compiler enforces that every caller passes the callback parameter. The callback invocation points are verified by reading the diff. No new tests are needed for this iteration because:
- The callbacks are invoked with hardcoded enum variants at fixed points in the control flow. Incorrect invocations would be type errors.
- The no-op closures mean observable behavior is identical to before.
- Story 2 (CLI spinners) will add integration tests that assert on actual progress output.

## Notes

`PruneProgress` is defined in the engine module (`reservation.rs`) but consumed in the CLI module (`reservations.rs`). This keeps all progress types co-located with the reservation domain. The `run_prune` function in the CLI layer accepts `impl Fn(PruneProgress)` and orchestrates the invocations itself, since the prune loop lives in CLI code rather than the engine.
