---
title: Pubsub expansion architecture with disk cache
type: iteration
status: accepted
author: agent
date: 2026-03-12
tags: []
related:
- implements: STORY-058
---




## Context

Supersedes ITERATION-055. The current TUI async expansion uses `std::sync::mpsc` with `try_recv()` polling and an in-memory `HashMap` cache. This iteration replaces that with a unified `crossbeam-channel` event loop, cooperative thread cancellation, and a persistent disk cache under `~/.lazyspec/cache/`.

**Story ACs addressed:**
- TUI shows raw body immediately with loading indicator, expanded body replaces it once ready
- Document switch discards stale expansion and starts new one
- (Implicit performance AC) Disk cache avoids redundant git+tree-sitter work across sessions

## Changes

### Task 1: Add crossbeam-channel dependency and define AppEvent enum

**ACs addressed:** TUI async expansion, document switch handling

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/tui/app.rs`

**What to implement:**

Add `crossbeam-channel = "0.5"` to `[dependencies]` in `Cargo.toml`.

In `src/tui/app.rs`, define a unified event enum:

```rust
pub enum AppEvent {
    Terminal(crossterm::event::KeyEvent),
    FileChange(notify::Event),
    ExpansionResult { path: PathBuf, body: String },
    #[cfg(feature = "agent")]
    AgentFinished,
}
```

Replace the `expansion_tx`/`expansion_rx` fields (`mpsc::Sender`/`mpsc::Receiver`) on `App` with a single `crossbeam_channel::Sender<AppEvent>` stored on App (the receiver lives in the event loop in `mod.rs`). Also add:
- `expansion_cancel: Option<Arc<AtomicBool>>` for cooperative cancellation

Remove the `poll_expansion_results()` method entirely. Remove `start_expansion_if_needed()` -- it will be replaced by a method that takes the sender as an argument.

Add a new method `request_expansion(&mut self, tx: &crossbeam_channel::Sender<AppEvent>)` that does what `start_expansion_if_needed` did, but spawns threads that send `AppEvent::ExpansionResult` through the crossbeam sender. Before spawning, if `expansion_cancel` is `Some`, set it to `true`. Create a fresh `Arc<AtomicBool>` for the new thread.

**How to verify:**
`cargo check` passes. No runtime test yet -- wiring happens in Task 2.

---

### Task 2: Restructure TUI event loop with crossbeam select

**ACs addressed:** TUI async expansion, document switch handling

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

Replace the current event loop structure (lines 53-148) with a crossbeam-channel-based architecture:

1. Create a `crossbeam_channel::unbounded::<AppEvent>()` pair. The sender is shared; the receiver is owned by the loop.

2. Spawn a dedicated terminal input thread:
   ```rust
   let term_tx = tx.clone();
   std::thread::spawn(move || {
       loop {
           if crossterm::event::poll(Duration::from_millis(50)).unwrap_or(false) {
               if let Ok(Event::Key(key)) = crossterm::event::read() {
                   let _ = term_tx.send(AppEvent::Terminal(key));
               }
           }
       }
   });
   ```

3. Wire the file watcher to send `AppEvent::FileChange` through a cloned sender (replacing the current `mpsc::channel` watcher).

4. The main loop becomes:
   ```rust
   loop {
       terminal.draw(|f| ui::draw(f, &mut app))?;
       app.request_expansion(&tx);

       match rx.recv_timeout(Duration::from_millis(100)) {
           Ok(AppEvent::Terminal(key)) => {
               app.handle_key(key.code, key.modifiers, &root, config);
           }
           Ok(AppEvent::FileChange(event)) => {
               // existing file change handling logic
           }
           Ok(AppEvent::ExpansionResult { path, body }) => {
               if app.expansion_in_flight.as_ref() == Some(&path) {
                   app.expansion_in_flight = None;
               }
               app.expanded_body_cache.insert(path.clone(), body.clone());
               app.disk_cache.write(&path, &body); // Task 3
           }
           Err(_) => {} // timeout, redraw
       }
       // drain remaining events without blocking
       while let Ok(event) = rx.try_recv() { /* handle same as above */ }

       // editor/resume/fix handling unchanged
       if app.should_quit { break; }
   }
   ```

5. Remove `use std::sync::mpsc;` from mod.rs. Keep the `crossterm::event` import for `Event::Key` pattern matching.

**How to verify:**
`cargo run -- tui` launches and responds to keyboard input. Select a document with `@ref` directives, confirm loading indicator appears then expanded body replaces it. Switch documents quickly, confirm no stale expansion shows.

---

### Task 3: Disk cache module

**ACs addressed:** Performance (implicit), cache invalidation on file watch

**Files:**
- Create: `src/engine/cache.rs`
- Modify: `src/engine/mod.rs`
- Modify: `src/tui/app.rs`

**What to implement:**

Create `src/engine/cache.rs` with a `DiskCache` struct:

```rust
pub struct DiskCache {
    dir: PathBuf,  // ~/.lazyspec/cache/
}
```

Methods:
- `DiskCache::new() -> Self` -- resolves `~/.lazyspec/cache/`, creates dir if missing via `fs::create_dir_all`
- `fn cache_key(path: &Path, body_hash: u64) -> String` -- hash the doc path + a hash of the raw body (so cache invalidates when the doc changes). Use `std::hash::DefaultHasher`.
- `fn read(&self, path: &Path, body_hash: u64) -> Option<String>` -- read `{dir}/{cache_key}` if it exists
- `fn write(&self, path: &Path, body_hash: u64, expanded: &str)` -- write to `{dir}/{cache_key}`. Ignore write errors (cache is best-effort).
- `fn invalidate(&self, path: &Path)` -- remove all cache files whose name starts with the hash prefix for this path. Called on file watcher events.
- `fn clear(&self)` -- remove all files in the cache dir. Called when a non-markdown file changes (conservative invalidation).

Add `pub mod cache;` to `src/engine/mod.rs`.

Add a `disk_cache: DiskCache` field to `App`. In `request_expansion`, check the disk cache before spawning a thread. If a disk cache hit, insert directly into `expanded_body_cache` and return.

The body hash approach means: if a doc's raw body changes (e.g. new `@ref` added), the old cache entry is stale but harmless (orphaned file). The new body hash won't match, so a fresh expansion runs. Periodic cleanup is out of scope.

**How to verify:**
`cargo test` for unit tests on `DiskCache` (see Test Plan). Manual: open TUI, view a doc with `@ref`, quit, reopen -- expanded body should appear instantly without the loading indicator.

---

### Task 4: Cancellation token in RefExpander

**ACs addressed:** Document switch discards stale expansion

**Files:**
- Modify: `src/engine/refs.rs`

**What to implement:**

Add a cancellation-aware expand method to `RefExpander`:

```rust
pub fn expand_cancellable(
    &self,
    content: &str,
    cancel: &AtomicBool,
) -> Result<Option<String>>
```

This works like `expand()` but checks `cancel.load(Ordering::Relaxed)` before each `resolve_ref` call. If cancelled, returns `Ok(None)`.

The existing `expand()` method stays unchanged (used by CLI `show -e`).

In `App::request_expansion`, pass the `Arc<AtomicBool>` into the spawned thread. The thread calls `expand_cancellable` and only sends `AppEvent::ExpansionResult` if it gets `Some`.

**How to verify:**
Unit test: create a `RefExpander`, set cancel to true before calling `expand_cancellable` on content with multiple `@ref` directives, assert it returns `Ok(None)`. Manual: rapidly switch between documents in TUI, confirm no stale expansions appear and CPU usage drops quickly.

## Test Plan

| AC | Test | Type | Tradeoffs |
|----|------|------|-----------|
| TUI shows raw then expanded | Open TUI, select doc with `@ref`, observe loading -> expanded | Manual | Not automatable without TUI test harness (sacrifices Deterministic for Predictive) |
| Document switch discards stale | Select doc A (with `@ref`), quickly switch to B, verify B loads independently | Manual | Same as above |
| Cancellation token works | `expand_cancellable` returns `None` when cancel is set before call | Unit (`cargo test`) | Isolated, deterministic, fast |
| Cancellation mid-expand | `expand_cancellable` with multiple refs, set cancel after first resolve | Unit (`cargo test`) | Tests cooperative check between refs |
| Disk cache read/write | Write expanded body, read back, assert equality | Unit (`cargo test`) | Fast, isolated, deterministic |
| Disk cache invalidate | Write entry, invalidate path, read returns None | Unit (`cargo test`) | Specific failure on cache bug |
| Disk cache clear | Write entries, clear, all reads return None | Unit (`cargo test`) | Covers conservative invalidation path |
| Disk cache persists across sessions | Open TUI, view doc, quit, reopen, verify instant render | Manual | Predictive but not automatable |
| Event loop handles all event types | `cargo run -- tui` responds to keys, file changes, expansion results | Manual | Integration-level verification |
| Full suite passes | `cargo test` | Automated | Regression gate |

## Notes

- Supersedes ITERATION-055. If ITERATION-055 was partially implemented, its mpsc channel code and in-memory cache get replaced by this iteration's crossbeam + disk cache approach.
- The terminal input thread is a daemon thread (no join handle needed). It will exit when the process exits.
- `recv_timeout` with 100ms matches the current poll interval, keeping the UI responsive without busy-waiting.
- The disk cache uses body hash as part of the key, so it self-invalidates when doc content changes. No TTL needed for HEAD refs; the file watcher + body hash covers invalidation.
- `crossbeam-channel` is a well-established crate (~50M downloads) with no transitive dependencies beyond `crossbeam-utils`.
