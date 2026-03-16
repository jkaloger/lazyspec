---
title: Fix TUI input lag
type: iteration
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- related-to: docs/audits/AUDIT-003-tui-input-lag-audit.md
---



## Context

Audit AUDIT-003 identified 8 sources of input lag in the TUI. The root cause is a combination of polling timeouts (150ms worst-case latency floor) and synchronous disk I/O on the render hot path. This iteration addresses all 8 findings in 4 tasks, ordered by impact.

## Changes

### Task 1: Reduce event loop latency (Findings 1, 8)

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

The input thread currently polls with a 50ms timeout, then the main loop waits up to 100ms on `recv_timeout`. Together these create a 150ms worst-case latency floor.

Two changes:

1. **Input thread (line 142-154):** Replace `poll(50ms)` + `read()` with a blocking `crossterm::event::read()` when not paused. When paused, keep the current `sleep(50ms)` + `continue` pattern. The thread is dedicated to input, so blocking is correct.

```rust
std::thread::spawn(move || {
    loop {
        if paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        // Blocking read - wakes immediately on keypress
        if let Ok(Event::Key(key)) = crossterm::event::read() {
            let _ = term_tx.send(AppEvent::Terminal(key));
        }
    }
});
```

2. **Main loop (line 173):** Replace `recv_timeout(Duration::from_millis(100))` with `recv_timeout(Duration::from_millis(16))`. 16ms gives ~60fps render cadence while still allowing idle sleep. The channel is already woken by terminal events, file watcher events, and expansion results, so most frames won't actually wait the full 16ms.

**How to verify:**
- Run the TUI and type rapidly in search mode. Characters should appear without perceptible delay.
- Verify idle CPU usage hasn't increased significantly (the blocking read prevents busy-polling).

---

### Task 2: Remove synchronous disk I/O from hot paths (Findings 2, 3)

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/mod.rs`

**What to implement:**

Three call sites do synchronous `fs::read_to_string` on the main thread:

**a) `draw_preview_content` (ui.rs:455-459):** Replace the `get_body_raw` fallback with an empty string. The expansion cache is populated by `request_expansion` each loop iteration, so the empty fallback is only visible for a single frame.

```rust
let body = app.expanded_body_cache.get(&doc.path)
    .cloned()
    .unwrap_or_default();
```

**b) `draw_fullscreen` (ui.rs:788-792):** Same change:

```rust
let body = app.expanded_body_cache.get(&doc.path)
    .cloned()
    .unwrap_or_default();
```

**c) `request_expansion` (app.rs:794):** Move the `get_body_raw` call into the spawned thread. The early-exit for non-`@ref` docs also moves into the thread, which sends back an `ExpansionResult` either way.

```rust
pub fn request_expansion(&mut self, tx: &crossbeam_channel::Sender<AppEvent>) {
    let doc_path = match self.selected_doc_meta() {
        Some(meta) => meta.path.clone(),
        None => return,
    };
    if self.expanded_body_cache.contains_key(&doc_path) { return; }
    if self.expansion_in_flight.as_ref() == Some(&doc_path) { return; }

    if let Some(cancel) = &self.expansion_cancel {
        cancel.store(true, Ordering::Relaxed);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    self.expansion_cancel = Some(cancel.clone());
    self.expansion_in_flight = Some(doc_path.clone());

    let root = self.store.root().to_path_buf();
    let tx = tx.clone();
    let disk_cache = self.disk_cache.clone();
    std::thread::spawn(move || {
        let body = match Store::read_body_raw(&root, &doc_path) {
            Ok(b) => b,
            Err(_) => return,
        };
        if !body.contains("@ref ") {
            let _ = tx.send(AppEvent::ExpansionResult {
                path: doc_path, body_hash: DiskCache::body_hash(&body), body,
            });
            return;
        }
        let body_hash = DiskCache::body_hash(&body);
        if let Some(cached) = disk_cache.read(&doc_path, body_hash) {
            let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body: cached, body_hash });
            return;
        }
        let expander = RefExpander::new(root);
        match expander.expand_cancellable(&body, &cancel) {
            Ok(Some(expanded)) => {
                let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body: expanded, body_hash });
            }
            Ok(None) => {}
            Err(_) => {
                let _ = tx.send(AppEvent::ExpansionResult { path: doc_path, body, body_hash });
            }
        }
    });
}
```

This requires either making `DiskCache` cloneable (it wraps a `PathBuf` for the cache dir, so `Clone` is trivial) or extracting a static `Store::read_body_raw(root, path) -> Result<String>` method.

**d) Main loop diagram body fallback (mod.rs:161-163):** The `unwrap_or_else(|| app.store.get_body_raw(...))` fallback also does a sync read. Replace with cache-only:

```rust
if let Some(meta) = app.selected_doc_meta() {
    if let Some(body) = app.expanded_body_cache.get(&meta.path) {
        let blocks = diagram::extract_diagram_blocks(body);
        for block in &blocks {
            app.request_diagram_render(block, &tx);
        }
    }
}
```

**How to verify:**
- Navigate rapidly between documents with arrow keys. The TUI should remain responsive.
- Documents with `@ref` markers should still expand (just asynchronously).
- `cargo test` passes.

---

### Task 3: Cache and deduplicate diagram block extraction (Findings 5, 6)

**Files:**
- Modify: `src/tui/app.rs` (add cache field)
- Modify: `src/tui/mod.rs` (use cached blocks)
- Modify: `src/tui/diagram.rs` (accept pre-extracted blocks)

**What to implement:**

**a) Add a diagram blocks cache to App (app.rs):**

Add a field `diagram_blocks_cache: Option<(PathBuf, u64, Vec<DiagramBlock>)>` that stores `(doc_path, body_hash, blocks)`. Invalidate when the selected doc or body changes.

**b) Main loop (mod.rs:160-168):** Use cached blocks:

```rust
if let Some(meta) = app.selected_doc_meta() {
    if let Some(body) = app.expanded_body_cache.get(&meta.path) {
        let body_hash = DiskCache::body_hash(body);
        let blocks = match &app.diagram_blocks_cache {
            Some((p, h, b)) if p == &meta.path && *h == body_hash => b.clone(),
            _ => {
                let b = diagram::extract_diagram_blocks(body);
                app.diagram_blocks_cache = Some((meta.path.clone(), body_hash, b.clone()));
                b
            }
        };
        for block in &blocks {
            app.request_diagram_render(block, &tx);
        }
    }
}
```

**c) Thread blocks through diagram rendering (diagram.rs):**

Change `build_preview_segments` to accept an optional `&[DiagramBlock]` parameter, avoiding re-extraction. Change `inject_fallback_hints` to accept `&[DiagramBlock]` instead of re-calling `extract_diagram_blocks`.

```rust
pub fn build_preview_segments(
    body: &str,
    blocks: &[DiagramBlock],  // pre-extracted
    cache: &DiagramCache,
    protocol: TerminalImageProtocol,
    tools: &ToolAvailability,
) -> Vec<PreviewSegment>
```

Update callers in `ui.rs` to pass the blocks through.

**How to verify:**
- Open a document with d2/mermaid blocks. Diagrams still render.
- `cargo test` passes.

---

### Task 4: Cache filtered_docs and pre-compute search index (Findings 4, 7)

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

**a) Cache filtered_docs (Finding 7):**

Add a field `filtered_docs_cache: Option<Vec<PathBuf>>` to `App`. Populate it lazily in `filtered_docs()`. Invalidate (set to `None`) whenever `filter_status`, `filter_tag`, or the store changes (in `refresh_validation`, `handle_app_event` for `FileChange`, etc.).

```rust
pub fn filtered_docs(&mut self) -> &[PathBuf] {
    if self.filtered_docs_cache.is_none() {
        let mut docs = self.store.list(&Filter {
            doc_type: None,
            status: self.filter_status.clone(),
            tag: self.filter_tag.clone(),
        });
        docs.sort_by(|a, b| a.path.cmp(&b.path));
        self.filtered_docs_cache = Some(docs.into_iter().map(|d| d.path.clone()).collect());
    }
    self.filtered_docs_cache.as_deref().unwrap()
}

fn invalidate_filtered_docs(&mut self) {
    self.filtered_docs_cache = None;
}
```

Call `invalidate_filtered_docs()` from: `refresh_validation`, `handle_app_event(FileChange)`, filter toggle methods, and store reload paths.

Note: this changes `filtered_docs` from `&self` to `&mut self`. Callers that currently borrow immutably will need minor adjustment (e.g. collecting the paths before passing to draw functions, or splitting borrows).

**b) Pre-compute lowercase search index (Finding 4):**

Add a field `search_index: Vec<SearchEntry>` where:

```rust
struct SearchEntry {
    path: PathBuf,
    searchable: String, // pre-lowercased "title\0tag1\0tag2\0path"
}
```

Rebuild `search_index` when the store changes (same invalidation points as filtered_docs). Then `update_search` becomes:

```rust
pub fn update_search(&mut self) {
    if self.search_query.is_empty() {
        self.search_results.clear();
        self.search_selected = 0;
        return;
    }
    let query = self.search_query.to_lowercase();
    let mut results: Vec<_> = self.search_index.iter()
        .filter(|e| e.searchable.contains(&query))
        .map(|e| e.path.clone())
        .collect();
    results.sort();
    self.search_results = results;
    self.search_selected = 0;
}
```

This eliminates per-keystroke allocations. The single `contains` call on a pre-built string replaces three separate `to_lowercase().contains()` chains.

**How to verify:**
- Type in search mode. Results should appear correctly and without lag.
- Toggle filters. Document list should update correctly.
- `cargo test` passes.

## Test Plan

### Test 1: Input thread uses blocking read (Task 1)

Verify the input thread calls `crossterm::event::read()` directly (not wrapped in `poll`). This is a structural/code review check since the input thread isn't directly testable in unit tests without mocking crossterm.

> Tradeoff: sacrifices Isolated/Deterministic for Predictive. Manual verification is needed since crossterm's event system requires a real terminal.

### Test 2: Draw path never calls get_body_raw (Task 2)

Add a unit test that constructs an App with an empty `expanded_body_cache` for a selected document, then verifies that calling the body-resolution logic returns an empty/default string rather than attempting disk I/O. Use the existing `make_test_app` pattern.

### Test 3: request_expansion doesn't block main thread (Task 2)

Verify that `request_expansion` returns immediately by checking it sets `expansion_in_flight` without reading from disk. The spawned thread handles the read. Test by calling `request_expansion` on an App with a non-existent file path and confirming it doesn't error (the thread handles the error).

### Test 4: Diagram blocks cache hits on unchanged body (Task 3)

Construct an App, populate `expanded_body_cache` with a body containing a diagram block. Call the cache logic twice with the same body. Verify `extract_diagram_blocks` would only be called once (check that `diagram_blocks_cache` is `Some` with matching hash after first call).

### Test 5: Search index produces same results as old implementation (Task 4)

Using `make_test_app`, add docs with known titles/tags. Build the search index. Run `update_search` with various queries and verify results match what the old `all_docs().filter()` approach would produce.

### Test 6: filtered_docs cache invalidation (Task 4)

Construct an App with filters. Call `filtered_docs()` twice, verify same results. Change filter, call again, verify updated results. This confirms the cache invalidates correctly.

## Notes

- Task 2 changes `request_expansion` significantly. The `DiskCache` needs to be either `Clone` or wrapped in `Arc`. Check the current definition before implementing.
- Task 4's `filtered_docs` signature change (`&self` -> `&mut self`) will ripple through callers in `ui.rs` where `app` is borrowed. The draw functions take `&mut App` already, so this should be straightforward.
- The 16ms recv_timeout in Task 1 is a starting point. If idle CPU is too high, it can be bumped to 32ms (still well under the old 100ms).
