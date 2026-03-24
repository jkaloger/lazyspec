---
title: "TUI Content Rendering"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, rendering, markdown, diagrams]
related: []
---

## Acceptance Criteria

### AC: gfm-table-extraction

Given a document body containing a pipe-delimited GFM table with a header separator row
When `extract_gfm_segments` processes the body
Then the result contains a `GfmSegment::Table` with headers, alignments, and data rows matching the source

### AC: gfm-table-column-alignment

Given a `GfmTable` with left, center, and right aligned columns
When `render_table` produces terminal lines
Then header cells are bold, columns are padded according to their alignment, and box-drawing characters (`│`, `─`, `┼`) separate cells

### AC: gfm-admonition-rendering

Given a document body containing `> [!WARNING]\n> Be careful here.`
When the body is extracted and rendered through the GFM pipeline
Then the output contains a bold yellow "WARNING" label followed by body lines prefixed with a colored `▌` gutter

### AC: gfm-footnote-collection

Given a document body with footnote definitions at arbitrary positions
When `extract_gfm_segments` processes the body
Then footnote definitions do not appear inline as markdown text, and all footnotes are appended at the end of the segment list

### AC: gfm-footnote-rendering

Given a segment list containing `GfmSegment::Footnote` entries
When `render_gfm_segments` produces terminal lines
Then a horizontal rule separator precedes the footnotes, and each footnote displays as `[^label]: definition` with a bold label

### AC: diagram-block-detection

Given a document body containing a fenced code block with language tag `d2`
When `extract_diagram_blocks` scans the body
Then the block is returned as a `DiagramBlock` with `DiagramLanguage::D2`, the source text between the fences, and a byte range covering the entire fenced block

### AC: diagram-four-backtick-skip

Given a document body containing a code block fenced with four or more backticks
When `extract_diagram_blocks` scans the body
Then the block is not treated as a diagram block

### AC: diagram-cache-hit

Given a `DiagramCache` containing an `Image` entry for a source hash
When `build_preview_segments` encounters a diagram block with the same source hash
Then the result contains a `PreviewSegment::DiagramImage` pointing to the cached path, and no background render thread is spawned

### AC: diagram-fallback-missing-tool

Given the `d2` CLI tool is not installed
When a d2 diagram block is encountered during preview rendering
Then a fallback hint `[d2: install d2 CLI for diagram rendering]` is injected into the markdown text after the code block

### AC: ref-expansion-background-thread

Given a document containing `@ref` directives
When `request_expansion` is called
Then expansion runs on a background thread and the preview displays `[expanding refs...]` until the result arrives

### AC: ref-expansion-cancellation

Given an expansion is in flight for document A
When the user navigates to document B and `request_expansion` is called for B
Then the cancellation flag for document A's thread is set to true, and a new expansion thread is spawned for B

### AC: fullscreen-scroll-navigation

Given the fullscreen reader is open with content taller than the viewport
When the user presses `Ctrl-D`
Then the scroll offset increases by half the viewport height (`fullscreen_height / 2`)

### AC: two-pass-image-overlay

Given a document with a rendered diagram image
When the preview panel draws the content
Then the first pass renders a `Paragraph` with blank placeholder lines at the image position, and the second pass calls `render_image_overlay` to paint the image at the computed y-offset

### AC: search-index-substring-match

Given the search index contains a document titled "Architecture Overview" with tag "tui"
When the user types "arch" in the search overlay
Then the document appears in the search results because the lowercased query is a substring of the searchable string
