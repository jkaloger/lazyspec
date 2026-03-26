---
title: Diagram image rendering and caching
type: iteration
status: accepted
author: agent
date: 2026-03-14
tags: []
related:
- implements: STORY-063
---




## Context

Iterations 065-067 implemented detection (terminal caps probe, diagram block extraction, fallback hints, tool availability caching). This iteration builds the actual rendering pipeline: shell out to `d2`/`mmdc`, display images inline via `ratatui-image`, cache results, and fall back to ASCII text output when the terminal doesn't support images.

**ACs addressed:** AC3 (async rendering), AC4 (inline image display), AC5 (caching), AC6 (cache invalidation)

**Key constraint:** d2 0.7.1+ supports `.txt` output for ASCII diagrams, used as the fallback when terminal image protocol is `Unsupported`.

## Changes

### Task 1: Add dependencies and diagram render function

**ACs addressed:** AC3

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/tui/diagram.rs`

**What to implement:**

Add `ratatui-image` and `image` crates to `Cargo.toml`.

In `diagram.rs`, add a `render_diagram` function that:
1. Takes a `DiagramBlock` and an output directory path
2. Computes a content hash of `block.source` (reuse `crate::engine::cache::hash_string`)
3. Shells out to `d2` (writing source to a temp `.d2` file, outputting to `<hash>.png`)
4. For mermaid, shells out to `mmdc -i <input> -o <hash>.png`
5. Returns `Result<PathBuf>` pointing to the rendered PNG

Also add `render_diagram_text` for the ASCII fallback path:
1. Same hash-based approach but outputs to `<hash>.txt` using d2's `.txt` format
2. Returns `Result<String>` with the ASCII content

Both functions are synchronous -- they'll be called from a background thread.

**How to verify:**
```
cargo test diagram
```

### Task 2: Diagram cache with content-hash keying

**ACs addressed:** AC5, AC6

**Files:**
- Modify: `src/tui/diagram.rs`

**What to implement:**

Add a `DiagramCache` struct:
```rust
pub struct DiagramCache {
    cache_dir: PathBuf,
    entries: HashMap<u64, DiagramCacheEntry>,
}

pub enum DiagramCacheEntry {
    Rendering,                    // in-flight
    Image(PathBuf),               // rendered PNG path
    Text(String),                 // ASCII fallback content
    Failed(String),               // error message
}
```

- `cache_dir` uses `std::env::temp_dir().join("lazyspec-diagrams")`
- `get(source_hash) -> Option<&DiagramCacheEntry>` for cache lookup
- `insert(source_hash, entry)` for storing results
- `mark_rendering(source_hash)` sets `Rendering` state (prevents duplicate dispatches)
- Cache invalidation is implicit: changed source text produces a different hash, so old entries are just orphaned

**How to verify:**
```
cargo test diagram
```

### Task 3: Async rendering dispatch and event plumbing

**ACs addressed:** AC3

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/mod.rs`

**What to implement:**

Add `DiagramCache` field to `App` struct, initialized in `App::new`.

Add a new `AppEvent` variant:
```rust
AppEvent::DiagramRendered { source_hash: u64 }
```

Add `App::request_diagram_render(&mut self, block: &DiagramBlock, tx: &Sender<AppEvent>)`:
1. Compute source hash
2. If cache has an entry (any state), return early
3. Mark as `Rendering` in cache
4. Clone necessary data, spawn `std::thread::spawn`
5. In the thread: call `render_diagram` (PNG) or `render_diagram_text` (ASCII) based on `self.terminal_image_protocol`
6. Send `AppEvent::DiagramRendered { source_hash }` on completion
7. Handle the event in the event loop to trigger redraw

**How to verify:**
```
cargo test -- app
```

### Task 4: Split preview into text/image segments

**ACs addressed:** AC4

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/diagram.rs`

**What to implement:**

This is the core rendering change. Instead of passing the full body through `tui_markdown::from_str()` as one block, split it around diagram blocks.

Add a `PreviewSegment` enum:
```rust
enum PreviewSegment {
    Markdown(String),             // text content to pass through tui_markdown
    DiagramImage(PathBuf),        // rendered PNG to display via ratatui-image
    DiagramText(String),          // ASCII fallback text
    DiagramLoading,               // "[rendering diagram...]" placeholder
    DiagramError(String),         // render failure message
}
```

Add `build_preview_segments(body: &str, cache: &DiagramCache, protocol: TerminalImageProtocol, tools: &ToolAvailability) -> Vec<PreviewSegment>`:
1. Extract diagram blocks from body
2. Split body text around diagram block byte ranges
3. For each diagram block, check cache state and produce the appropriate segment
4. For non-diagram text between blocks, produce `Markdown` segments

In `draw_preview_content` and `draw_fullscreen`, replace the current single-paragraph approach:
1. Call `build_preview_segments`
2. Trigger `request_diagram_render` for any blocks not yet cached
3. Render each segment into sub-areas using vertical `Layout::default()` with `Constraint::Min` for text and `Constraint::Length` for images
4. For `DiagramImage` segments, use `ratatui_image::StatefulImage` widget
5. For `DiagramText` segments, render as a `Paragraph` with the ASCII content
6. For `DiagramLoading`, render `[rendering diagram...]` styled yellow

Store `ratatui_image::picker::Picker` and per-image `StatefulProtocol` states in `App` for the stateful widget rendering.

**How to verify:**
```
cargo run
# Navigate to RFC-021 which has a d2 block
# Should see rendered diagram (image or ASCII depending on terminal)
```

## Test Plan

### Unit: render_diagram produces PNG

Call `render_diagram` with a simple `a -> b` DiagramBlock. Assert returned path exists, is a PNG (check magic bytes), and file size is non-zero. *Requires d2 installed -- skip with `#[ignore]` if not available.*

Trade-off: not `Fast` (shells out to d2), but `Predictive` -- this is the actual rendering path.

### Unit: render_diagram_text produces ASCII

Call `render_diagram_text` with `a -> b`. Assert returned string is non-empty and contains expected node labels.

### Unit: DiagramCache insert and lookup

Insert entries with different hashes. Verify `get` returns correct entries. Verify different hash returns `None` (implicit invalidation).

### Unit: DiagramCache mark_rendering prevents duplicates

Call `mark_rendering(hash)`, verify `get(hash)` returns `Some(Rendering)`. Call `insert(hash, Image(...))`, verify it overwrites.

### Unit: build_preview_segments splits correctly

Given a body with text, a d2 block, and more text, with a cache containing a rendered image for that block's hash, assert the segments are `[Markdown, DiagramImage, Markdown]`.

### Unit: build_preview_segments shows loading for uncached

Same body but empty cache. Assert the diagram segment is `DiagramLoading`.

### Unit: fallback to text when protocol is Unsupported

Given `Unsupported` protocol and a cached `Text` entry, assert segment is `DiagramText`.

## Notes

- d2 0.7.1+ supports `.txt` output for ASCII diagrams (used as fallback)
- Mermaid has no native text output; mermaid fallback remains hint-only for now
- `ratatui-image` handles kitty and sixel protocol differences internally
- The segmented layout approach means scrolling in fullscreen view needs to account for image heights -- `ratatui-image`'s `StatefulImage` handles sizing within its constraint
