---
title: "Async git operations with progress feedback"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-03-20
tags: [reservation, threading, cli, tui]
related:
- related-to: docs/rfcs/RFC-028-git-based-document-number-reservation.md
---


## Problem

The reservation system (RFC-028) introduced git remote operations into the document creation path. Every `lazyspec create` with `numbering = "reserved"` runs `git ls-remote`, `git hash-object`, `git update-ref`, and `git push` synchronously. The `reservations list` and `reservations prune` commands do the same.

This has two consequences:

1. **CLI has no feedback.** A `create` against a slow remote (high-latency SSH, large reflist) hangs silently for seconds. The user has no way to distinguish "working" from "stuck." The `prune` command is worse -- it runs sequential `git push --delete` calls, one per ref, with no indication of progress.

2. **TUI freezes.** `submit_create_form` calls `create::run` on the main thread. While git operations block, the event loop stops: no rendering, no input handling, no file watch events. On a 2-second `ls-remote`, the terminal looks unresponsive.

These are the same class of problem the TUI already solved for ref expansion (background thread + `AppEvent::ExpansionResult`) and diagram rendering (background thread + `AppEvent::DiagramRendered`). The reservation system just hasn't been wired into that pattern yet.

## Intent

Move git remote operations off the calling thread and provide progress feedback in both the CLI and TUI.

The reservation module's public API (`reserve_next`, `list_reservations`, `delete_remote_ref`) stays synchronous and blocking -- it's the callers' job to run them on a background thread. This keeps the engine layer simple and testable. The CLI and TUI each provide their own concurrency and progress UI appropriate to their context.

No async runtime. The existing `std::thread` + `crossbeam_channel` pattern is sufficient for shelling out to git.

## Design

### Progress reporting from the engine

The reservation functions currently return `Result<T>` with no intermediate feedback. To support progress indicators, `reserve_next` and the prune workflow need a way to report what step they're on.

A callback is simpler than a channel here -- the caller passes a closure, the reservation function calls it at each stage:

@ref src/engine/reservation.rs#reserve_next

@draft ReservationProgress {
    QueryingRemote,
    PushAttempt { attempt: u8, max: u8, candidate: u32 },
    PushRejected { candidate: u32 },
    Reserved { number: u32 },
}

```rust
pub fn reserve_next(
    repo_root: &Path,
    remote: &str,
    prefix: &str,
    max_retries: u8,
    docs_dir: &Path,
    on_progress: impl Fn(ReservationProgress),
) -> Result<u32>
```

Existing callers that don't care about progress pass `|_| {}`. The callback runs on the same thread as the git operations -- it's the caller's responsibility to forward messages to a UI thread if needed.

The same pattern applies to `list_reservations` (report "querying remote") and the prune loop (report each deletion).

@draft PruneProgress {
    QueryingRemote,
    Deleting { current: usize, total: usize, ref_path: String },
    Done { pruned: usize, orphaned: usize },
}

### CLI: indicatif spinners

The CLI wraps each remote operation with an `indicatif::ProgressBar` spinner on stderr. This keeps stdout clean for `--json` output.

For `create` with reserved numbering:
- Spinner starts: "Querying remote for existing reservations..."
- Updates on each push attempt: "Reserving RFC-029 (attempt 1/5)..."
- Clears on success or prints error on failure

For `reservations list`:
- Spinner: "Querying remote..."
- Clears when results arrive, then prints the table/JSON

For `reservations prune`:
- Spinner during initial query
- Progress bar during deletions: "Pruning [3/12] refs/reservations/RFC/005"
- Summary on completion

The spinner thread is the main thread. The git operation runs on a spawned thread. The main thread polls for completion while ticking the spinner.

```rust
let pb = ProgressBar::new_spinner();
pb.set_message("Querying remote...");
pb.enable_steady_tick(Duration::from_millis(80));

let handle = std::thread::spawn(move || {
    reservation::reserve_next(&root, &remote, &prefix, max_retries, &dir, |p| {
        // send progress to channel
    })
});

// main thread drives spinner from progress channel until thread joins
let result = handle.join().unwrap()?;
pb.finish_and_clear();
```

When `--json` is passed, the spinner is suppressed entirely. The output is just the JSON object on stdout, same as today.

### TUI: background worker + AppEvent

Follow the expansion worker pattern documented in ARCH-005/threading-model:

@ref src/tui/app.rs#AppEvent

New event variants:

@draft AppEvent::CreateStarted
@draft AppEvent::CreateProgress { message: String }
@draft AppEvent::CreateComplete { result: Result<CreateResult> }

@draft CreateResult {
    path: PathBuf,
    doc_type: String,
}

The flow:

1. `submit_create_form` validates inputs, then spawns a thread instead of calling `create::run` inline.
2. The create form transitions to a "Reserving..." state. The form stays visible but inputs are disabled.
3. The background thread calls `create::run`, which calls `reserve_next` with a progress callback that sends `AppEvent::CreateProgress` messages through the channel.
4. On `AppEvent::CreateProgress`, the TUI updates the create form's status message.
5. On `AppEvent::CreateComplete`, the TUI either closes the form and navigates to the new document (success) or displays the error in the form (failure).

The main thread never blocks on git operations. The TUI remains fully responsive during reservation.

For non-reserved document types, the create path is fast enough (local filesystem only) that it can stay synchronous. The background worker is only needed when `numbering = "reserved"`.

### What this doesn't change

- The reservation protocol itself (RFC-028) is unchanged.
- Config format is unchanged.
- The `reserve_next` function remains synchronous and blocking internally -- the async boundary is at the caller level.
- No async runtime (tokio, async-std) is introduced.

## Stories

1. **Progress-aware reservation API** -- Add `ReservationProgress` and `PruneProgress` callback parameters to `reserve_next`, `list_reservations`, and the prune workflow. Add `indicatif` to `Cargo.toml`. Existing callers pass no-op callbacks until the UI stories land.

2. **CLI spinners for git remote operations** -- Wire `indicatif` spinners into `create` (when reserved), `reservations list`, and `reservations prune`. Suppress spinners when `--json` is passed.

3. **TUI async document creation** -- Background worker for `submit_create_form` when the type uses reserved numbering. New `AppEvent` variants, loading state in the create form UI, error display on failure.
