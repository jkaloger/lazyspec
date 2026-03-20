---
title: Reservation management subcommands
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-070-reservation-management.md
---


## Context

STORY-070 adds `lazyspec reservations list` and `lazyspec reservations prune` subcommands for inspecting and cleaning up reservation refs on the remote. The core reservation module (`src/engine/reservation.rs`) already has git plumbing for `ls-remote`, `hash-object`, `update-ref`, and `push`. This iteration extends it with public functions for listing all reservations across prefixes and deleting specific refs, then wires up CLI subcommands following the existing pattern.

## Changes

### Task 1: Add `Reservation` struct and public list/delete functions to the reservation module

**ACs addressed:** AC-1, AC-2 (list)

**Files:**
- Modify: `src/engine/reservation.rs`

**What to implement:**

Add a public `Reservation` struct:

```rust
pub struct Reservation {
    pub prefix: String,
    pub number: u32,
    pub ref_path: String,
}
```

Add `pub fn list_reservations(repo_root, remote, prefixes: &[&str]) -> Result<Vec<Reservation>>`. This calls `git ls-remote --refs <remote> "refs/reservations/*"` (wildcard across all prefixes), parses each line into a `Reservation`. The existing private `ls_remote` filters by single prefix; this new function queries all reservations at once.

Add `pub fn delete_remote_ref(repo_root, remote, ref_path: &str) -> Result<()>`. This runs `git push <remote> --delete <ref_path>` to remove a reservation ref from the remote.

Refactor the existing private `ls_remote` to call the new `list_reservations` internally if useful, or leave it as-is since it serves a different purpose (single-prefix, returns `Vec<u32>`).

**How to verify:**
Unit-testable via the integration test harness. Seed refs on a bare repo, call `list_reservations`, assert the returned vec contains the expected entries.

---

### Task 2: Add `Reservations` CLI subcommand with `list` and `prune`

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Create: `src/cli/reservations.rs`
- Modify: `src/cli/mod.rs` (add `pub mod reservations;` and `Reservations` variant to `Commands`)
- Modify: `src/main.rs` (add dispatch for `Commands::Reservations`)

**What to implement:**

Add a `Reservations` variant to the `Commands` enum using clap's nested subcommand pattern:

```rust
/// Manage reservation refs
Reservations {
    #[command(subcommand)]
    command: ReservationsCommand,
},
```

Define `ReservationsCommand` in `src/cli/reservations.rs`:

```rust
#[derive(Subcommand)]
pub enum ReservationsCommand {
    /// List all reservation refs on the remote
    List {
        #[arg(long)]
        json: bool,
    },
    /// Remove reservation refs for documents that exist locally
    Prune {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
}
```

Implement `pub fn run_list(repo_root, config, json: bool)`:
- Read `ReservedConfig` from config (bail if not configured)
- Collect all configured prefixes from `config.types`
- Call `reservation::list_reservations(repo_root, &remote, &prefixes)`
- Human output: table with columns `TYPE`, `NUMBER`, `REF`
- JSON output: `serde_json::to_string_pretty` on the vec of `Reservation` (derive `Serialize`)

Implement `pub fn run_prune(repo_root, config, store, dry_run, json: bool)`:
- List reservations (same as above)
- For each reservation, check if a matching document exists locally. Match by scanning the store for documents whose filename starts with `{PREFIX}-{number_formatted}` (handling both `{:03}` incremental and sqids-encoded formats based on `reserved.format`)
- Classify each as `prunable` (matching doc exists) or `orphan` (no match)
- If not dry-run: call `delete_remote_ref` for each prunable ref
- Human output: print each ref with status (pruned/orphan/error)
- JSON output: structured object with `pruned: []`, `orphaned: []`, `errors: []`

Wire up in `main.rs` following the existing dispatch pattern. The `Reservations` command needs both `Config` (for `ReservedConfig` and type prefixes) and `Store` (for document existence checks in prune).

**How to verify:**
`cargo build` succeeds. Manual smoke test with `cargo run -- reservations list --help` and `cargo run -- reservations prune --help`.

---

### Task 3: Integration tests

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Modify: `tests/reservation_test.rs`

**What to implement:**

Six new tests, one per AC:

1. **`list_shows_all_reservations`** (AC-1): Seed refs `RFC/1`, `RFC/3`, `STORY/5` on bare. Call `list_reservations`. Assert all three returned with correct prefix, number, ref_path.

2. **`list_json_output_is_structured`** (AC-2): Same setup. Call `run_list` with `json: true`. Parse the stdout as `Vec<serde_json::Value>`. Assert each entry has `prefix`, `number`, `ref_path` keys.

3. **`prune_deletes_refs_with_matching_documents`** (AC-3): Seed ref `RFC/42` on bare. Create a file `docs/rfcs/RFC-042-some-title.md` in the fixture. Call `run_prune(dry_run: false)`. Assert ref no longer exists on remote.

4. **`prune_flags_orphans_without_deleting`** (AC-4): Seed ref `RFC/99` on bare. No matching local document. Call `run_prune(dry_run: false)`. Assert ref still exists on remote. Assert output mentions orphan.

5. **`prune_dry_run_does_not_delete`** (AC-5): Seed ref `RFC/42` on bare. Create matching doc. Call `run_prune(dry_run: true)`. Assert ref still exists on remote. Assert output indicates it would be pruned.

6. **`prune_json_output_is_structured`** (AC-6): Seed refs `RFC/42` (with matching doc) and `RFC/99` (orphan). Call `run_prune(json: true, dry_run: false)`. Parse stdout as JSON. Assert `pruned` array contains `RFC/42` ref, `orphaned` array contains `RFC/99` ref.

All tests reuse `TestFixture::with_git_remote()` and `seed_ref_on_bare()` from the existing test file.

**How to verify:**
`cargo test --test reservation_test` passes.

## Test Plan

| # | AC | Test name | What it verifies | Tradeoffs |
|---|-----|-----------|------------------|-----------|
| 1 | AC-1 | `list_shows_all_reservations` | `list_reservations` returns all refs across multiple prefixes | Integration test (hits real git), sacrifices Fast for Predictive |
| 2 | AC-2 | `list_json_output_is_structured` | JSON output contains expected keys and structure | Tests output format, not just logic |
| 3 | AC-3 | `prune_deletes_refs_with_matching_documents` | Refs for existing documents are removed from remote | Integration: verifies actual ref deletion |
| 4 | AC-4 | `prune_flags_orphans_without_deleting` | Refs without matching documents survive prune | Verifies the safety guarantee |
| 5 | AC-5 | `prune_dry_run_does_not_delete` | Dry-run previews without side effects | Verifies the dry-run contract |
| 6 | AC-6 | `prune_json_output_is_structured` | JSON output separates pruned from orphaned refs | End-to-end: covers both prune and orphan in one test |

All tests are integration-level because the reservation system's value is in its git interaction. Mocking git would defeat the purpose. Each test creates an isolated bare+working repo pair, so they're Isolated and Deterministic despite being integration tests.

## Notes

The `Reservations` subcommand uses clap's nested subcommand pattern (subcommand within a subcommand). This is the first two-level command in lazyspec. If the pattern feels heavyweight, an alternative is top-level `reservations-list` and `reservations-prune`, but nested subcommands group better in help output.

The prune matching logic needs to handle both incremental (`RFC-042`) and sqids-encoded (`RFC-k3f`) formats when determining if a document exists for a given reservation number. The `ReservedFormat` from config tells us which encoding to check.
