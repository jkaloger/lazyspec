---
title: "TUI Content Rendering"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, rendering, markdown, diagrams]
related:
  - implements: "docs/architecture/ARCH-005-tui/rendering.md"
  - implements: "docs/stories/STORY-057-rendering-integration.md"
  - implements: "docs/stories/STORY-063-diagram-rendering-pipeline.md"
  - implements: "docs/stories/STORY-067-gfm-extensions-for-markdown-preview.md"
---

## Summary

The TUI content rendering pipeline transforms document bodies into styled terminal output. Rendering proceeds in distinct stages: GFM extraction, diagram processing, `@ref` expansion, and a two-pass display cycle (paragraph rendering followed by image overlay). The preview panel and fullscreen reader share the same rendering path, differing only in layout and scroll mechanics.

## GFM Extraction and Rendering

Markdown bodies pass through `extract_gfm_segments` before rendering. This function uses `pulldown-cmark` with GFM options enabled (tables, footnotes, strikethrough, task lists) and dispatches events to a set of extractors that implement the `GfmExtractor` trait.

@ref src/tui/content/gfm/parse.rs#GfmExtractor

Three extractors run in priority order: `FootnoteExtractor`, `AdmonitionExtractor`, `TableExtractor`. Each watches the event stream for its start tag and accumulates content until the matching end tag. The `extract_gfm_segments` function iterates the `pulldown-cmark` offset iterator once, feeding each event to the first active extractor. When no extractor is active, it tries to start one. The result is a `Vec<GfmSegment>` where non-extracted byte ranges become `GfmSegment::Markdown` segments and extracted ranges become their respective typed variants.

@ref src/tui/content/gfm.rs#GfmSegment

Footnotes receive special handling: they are collected separately via `ExtractorResult::FootnoteFinished` and appended at the end of the segment list regardless of their position in source. The `assemble_segments` function sorts extracted ranges by byte offset, fills gaps with `Markdown` segments, and appends footnotes last.

Rendering converts each `GfmSegment` into `Vec<Line>` via `render_gfm_segments`. Plain markdown segments delegate to `tui_markdown::from_str`. Tables render with column alignment (left, center, right), bold headers, and box-drawing separators. Admonitions display an uppercase label colored by kind (Note=blue, Warning=yellow, Tip=green, Important=magenta, Caution=red) followed by body lines prefixed with a colored `▌` gutter. Footnotes are collected across all segments and rendered at the bottom behind a horizontal rule.

@ref src/tui/content/gfm/render.rs#render_gfm_segments

## Diagram Rendering Pipeline

Fenced code blocks tagged `d2` or `mermaid` are identified by `extract_diagram_blocks`, which scans the body line-by-line. Blocks fenced with four or more backticks are skipped to avoid matching nested examples.

@ref src/tui/content/diagram.rs#DiagramBlock

Each detected block carries its `DiagramLanguage`, source text, and byte range within the document body. The `build_preview_segments` function splits the body into `PreviewSegment` variants: `Markdown` for text between diagrams, and `DiagramImage`, `DiagramText`, `DiagramLoading`, or `DiagramError` for diagram blocks depending on cache state and tool availability.

@ref src/tui/content/diagram.rs#PreviewSegment

Rendering is asynchronous. `request_diagram_render` checks `DiagramCache` first; if the source hash has no entry, it marks the hash as `Rendering` and spawns a background thread. That thread shells out to the `d2` CLI (at `--scale 2` for PNG mode) or produces ASCII text output when `ascii_diagrams` is true. Results arrive as `AppEvent::DiagramRendered` and are inserted into the in-memory cache.

@ref src/tui/state/expansion.rs#request_diagram_render

`DiagramCache` is a `HashMap<u64, DiagramCacheEntry>` keyed by content hash (`DefaultHasher` over the source string). The cache directory lives at `$TMPDIR/lazyspec-diagrams`. Cache invalidation is implicit: changed source text produces a different hash, so the old entry is never looked up again.

@ref src/tui/content/diagram.rs#DiagramCache

When the CLI tool is missing or the terminal lacks image protocol support, `fallback_hint` injects a bracketed message (e.g. `[d2: install d2 CLI for diagram rendering]`) after the code block in the markdown text. Tool availability is probed once at startup via `ToolAvailability::detect`, which runs `d2 --version`.

## @ref Expansion Display

`request_expansion` runs `@ref` directive expansion on a background thread. It reads the document file, extracts the body, and checks whether the body contains any `@ref ` strings. If none exist, the raw body is sent back immediately with its hash.

@ref src/tui/state/expansion.rs#request_expansion

When refs are present, the function first checks `DiskCache` using a body hash. On a cache miss, it creates a `RefExpander` and calls `expand_cancellable`, which supports cooperative cancellation via an `AtomicBool`. If the user navigates to a different document before expansion completes, the previous cancellation flag is set and a new thread is spawned.

Expanded bodies are stored in `app.expanded_body_cache` keyed by document path. Both the preview panel and fullscreen reader display `[expanding refs...]` as a yellow status indicator while expansion is in flight.

## Two-Pass Preview Rendering

The preview panel (`render_document_preview`) and fullscreen reader (`render_fullscreen_document`) both use a two-pass approach. The first pass calls `render_markdown_segment`, which iterates `PreviewSegment` variants and builds a flat `Vec<Line>`. For `DiagramImage` segments, it inserts blank placeholder lines sized to the image dimensions. The function returns a `SegmentLines` struct containing the lines, image segment metadata (hash, path, height), and the total wrapped height.

@ref src/tui/views/panels.rs#render_markdown_segment

The first pass output is rendered as a `Paragraph` widget with `Wrap { trim: false }`. The second pass, `render_diagram_overlays`, walks the segments again to compute y-offsets for each image and calls `render_image_overlay` to paint the actual image at the correct position. Images are decoded once via `image::open` and cached in `app.image_states` as `StatefulProtocol` instances for `ratatui-image`.

@ref src/tui/views/panels.rs#render_image_overlay

## Fullscreen Reader

Pressing Enter on a selected document opens the fullscreen reader. It renders a header line (title, status, type, author) followed by the document body using the same `render_markdown_segment` and diagram overlay pipeline. Scroll state is tracked via `app.scroll_offset` (a `u16`) and the `Paragraph` widget's `scroll()` method.

Navigation uses vim-style keys: `j`/`k` for single-line scroll, `Ctrl-D`/`Ctrl-U` for half-page jumps (computed as `fullscreen_height / 2`), `g` to jump to top, `G` to jump to bottom. A vertical scrollbar appears when content exceeds the viewport, rendered via `ScrollbarState` with cyan thumb and dark gray track.

@ref src/tui/views/panels.rs#render_fullscreen_document

## Search Overlay

The search overlay is activated by `/` and renders a text input at the top of the screen with results listed below. The search index is built by `rebuild_search_index`, which concatenates each document's title, tags, and file path (lowercased, null-separated) into a single `searchable` string per entry.

@ref src/tui/state/app.rs#rebuild_search_index

Matching is substring-based: `update_search` lowercases the query and filters the index with `contains`. Results display as a list with title, status (colored), type, and git gutter indicators. The index is rebuilt whenever the document store changes (on file watch events, manual refresh, or store mutations).
