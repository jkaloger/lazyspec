---
title: TUI async document creation
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-073-tui-async-document-creation.md
---



## Context

`submit_create_form` (`src/tui/app.rs:1181`) calls `create::run` synchronously. When the document type uses `NumberingStrategy::Reserved`, this triggers `reserve_next` which runs git remote operations that block the TUI event loop. The TUI already has a background worker pattern (expansion worker, diagram renderer, probe) using `std::thread` + `crossbeam_channel` + `AppEvent`.

This iteration wires document creation into that same pattern for reserved-numbering types, keeping non-reserved types synchronous.

## Changes

### Task 1: Add AppEvent variants and CreateResult type

**ACs addressed:** AC-2, AC-3, AC-4, AC-5

**Files:**
- Modify: `src/tui/app.rs` (AppEvent enum at line 21, new CreateResult struct)

**What to implement:**

Add three new variants to the `AppEvent` enum:

```rust
CreateStarted,
CreateProgress { message: String },
CreateComplete { result: Result<CreateResult, String> },
```

Add a `CreateResult` struct:

```rust
pub struct CreateResult {
    pub path: PathBuf,
    pub doc_type: DocType,
}
```

Use `String` for the error case in `CreateComplete` rather than `anyhow::Error` (which isn't `Send`-safe without boxing).

**How to verify:**
`cargo build` compiles. The new variants trigger exhaustive match warnings in `handle_app_event` until Task 3 handles them.

---

### Task 2: Add loading state to CreateForm

**ACs addressed:** AC-2, AC-5

**Files:**
- Modify: `src/tui/app.rs` (CreateForm struct at line 120)
- Modify: `src/tui/ui.rs` (create form rendering)

**What to implement:**

Add two fields to `CreateForm`:

```rust
pub loading: bool,
pub status_message: Option<String>,
```

Initialize both in `CreateForm::new()` (`loading: false`, `status_message: None`). Reset them in `reset()`.

In `ui.rs`, when `create_form.loading` is true:
- Render inputs as read-only / dimmed
- Display `status_message` in place of the error area

In the key handler for the create form (`app.rs:1407`), when `loading` is true:
- Escape should call `close_create_form()` (AC-7)
- All other keys (Enter, Tab, Char, Backspace) should be ignored

**How to verify:**
`cargo build` compiles. Visual verification deferred to integration.

---

### Task 3: Spawn background thread in submit_create_form

**ACs addressed:** AC-1, AC-2, AC-3, AC-6

**Files:**
- Modify: `src/tui/app.rs` (`submit_create_form` at line 1181)

**What to implement:**

In `submit_create_form`, after validation (title, relations), check whether the selected doc type uses `NumberingStrategy::Reserved` by looking it up in `config.types`.

**If reserved:** clone the necessary values (`root`, `config`, `doc_type_str`, `title`, `author`, `tags`, `relations`, `event_tx`), then:
1. Set `self.create_form.loading = true` and `status_message = Some("Reserving...")`.
2. Send `AppEvent::CreateStarted` through the channel.
3. Spawn a `std::thread` that:
   - Calls `create::run()` (which internally calls `reserve_next` with a progress callback that sends `AppEvent::CreateProgress` through the cloned `event_tx`)
   - Applies tags and relations (same logic as the current synchronous path)
   - Sends `AppEvent::CreateComplete { result: Ok(CreateResult { path, doc_type }) }` on success
   - Sends `AppEvent::CreateComplete { result: Err(e.to_string()) }` on failure

This requires `create::run` to accept an `on_progress` callback so it can forward it to `reserve_next`. Add an `on_progress` parameter to `create::run`:

```rust
pub fn run(
    root: &Path,
    config: &Config,
    doc_type: &str,
    title: &str,
    author: &str,
    on_progress: impl Fn(ReservationProgress),
) -> Result<PathBuf>
```

Existing CLI callers pass `|_| {}`. The TUI caller passes a closure that sends `AppEvent::CreateProgress`.

**If not reserved:** run the existing synchronous path unchanged (AC-6). Pass `|_| {}` as the callback.

**How to verify:**
`cargo build` compiles. The `create::run` signature change requires updating its call sites (CLI `create` command, existing tests).

---

### Task 4: Handle new AppEvent variants in the event loop

**ACs addressed:** AC-2, AC-3, AC-4, AC-5, AC-7

**Files:**
- Modify: `src/tui/mod.rs` (`handle_app_event` at line 50)

**What to implement:**

Add match arms in `handle_app_event`:

- `AppEvent::CreateStarted` -- no-op (form already transitioned in Task 3). Exists for extensibility.
- `AppEvent::CreateProgress { message }` -- if `app.create_form.active && app.create_form.loading`, update `app.create_form.status_message = Some(message)`.
- `AppEvent::CreateComplete { result }`:
  - If `!app.create_form.active` (user already closed the form via Escape): discard silently (AC-7).
  - If `Ok(create_result)`: reload store for the new path, clear caches, navigate to the new document (same post-creation logic currently in `submit_create_form` lines 1224-1239), then `close_create_form()`.
  - If `Err(msg)`: set `app.create_form.loading = false`, `app.create_form.error = Some(msg)`, `app.create_form.status_message = None` (re-enables inputs, AC-5).

**How to verify:**
`cargo build` compiles. Full verification in tests.

---

### Task 5: Update CLI call sites for new create::run signature

**ACs addressed:** none directly (mechanical change from Task 3)

**Files:**
- Modify: `src/cli/create.rs` (the CLI `run` entrypoint that calls the inner `run`)
- Modify: any test files that call `create::run` directly

**What to implement:**

The CLI's `create` command currently calls `create::run(root, config, doc_type, title, author)`. Add the `|_| {}` no-op callback argument. Search for all call sites with `cargo build` errors and fix them.

**How to verify:**
`cargo build` and `cargo test` compile without errors related to `create::run` arity.

## Test Plan

### Test 1: Loading state disables form input (AC-2)
Set `create_form.loading = true`, send key events (Char, Enter, Tab, Backspace). Assert none of them modify form state. Assert Escape calls `close_create_form`.
- File: `tests/tui_create_form_test.rs`
- Properties: Isolated, Fast, Behavioral, Deterministic

### Test 2: CreateComplete Ok navigates to new document (AC-4)
Create a fixture with a document type. Simulate `AppEvent::CreateComplete` with an `Ok(CreateResult)` pointing to a real file. Assert the form closes and `selected_doc` points to the new document's path.
- File: `tests/tui_submit_form_test.rs`
- Properties: Isolated, Fast, Behavioral, Specific

### Test 3: CreateComplete Err re-enables form (AC-5)
Set `create_form.loading = true`, then handle `AppEvent::CreateComplete` with `Err("some error")`. Assert `loading` is false, `error` contains the message, and `status_message` is None.
- File: `tests/tui_submit_form_test.rs`
- Properties: Isolated, Fast, Deterministic, Specific

### Test 4: CreateComplete after form closed is discarded (AC-7)
Set `create_form.active = false`, then handle `AppEvent::CreateComplete` with `Ok(...)`. Assert no panic and no state change.
- File: `tests/tui_submit_form_test.rs`
- Properties: Isolated, Fast, Structure-insensitive

### Test 5: CreateProgress updates status message (AC-3)
Set `create_form.loading = true` and `active = true`. Handle `AppEvent::CreateProgress { message: "Push attempt 1/5" }`. Assert `status_message` equals the message.
- File: `tests/tui_create_form_test.rs`
- Properties: Isolated, Fast, Deterministic

### Test 6: Non-reserved type stays synchronous (AC-6)
Call `submit_create_form` with a doc type using `NumberingStrategy::Incremental`. Assert the document is created immediately (file exists on disk) without any `CreateStarted` event being sent.
- File: `tests/tui_submit_form_test.rs`
- Properties: Isolated, Fast, Predictive
- Tradeoff: This is a negative test (asserting something did NOT happen). We verify by checking the file exists synchronously after the call returns, which proves no background thread was used.

## Notes

The `create::run` signature change (Task 3/5) is the only cross-cutting modification. It touches the CLI path but is mechanical (add `|_| {}` to existing callers). The TUI-specific changes are self-contained within `src/tui/`.

Post-creation logic (reload store, navigate, clear caches) currently lives in `submit_create_form`. Task 4 needs to duplicate this into the `CreateComplete` handler. Consider extracting a shared helper like `finalize_create(&mut self, path, doc_type)` to avoid duplication between the sync and async paths.
