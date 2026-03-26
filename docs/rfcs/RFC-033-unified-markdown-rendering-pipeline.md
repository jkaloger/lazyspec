---
title: Unified markdown rendering pipeline
type: rfc
status: draft
author: jkaloger
date: 2026-03-24
tags:
- refactor
- tui
- markdown
related:
- related-to: RFC-017
- related-to: STORY-067
---




## Problem

The TUI preview pipeline parses markdown content multiple times through overlapping mechanisms:

1. `gfm/parse.rs` runs pulldown-cmark with GFM flags to extract tables, admonitions, and footnotes as structured segments
2. `gfm/render.rs` passes remaining `Markdown(text)` segments to `tui_markdown::from_str`, which internally runs pulldown-cmark _again_ to produce `Vec<Line>`
3. `diagram.rs` uses a hand-rolled byte scanner to find `d2`/`mermaid` fenced code blocks, duplicating what pulldown-cmark's `CodeBlock` events already provide
4. `engine/refs/code_fence.rs` uses another hand-rolled byte scanner to detect fenced code blocks for `@ref` suppression

The result: the same text gets parsed by pulldown-cmark at least twice for rendering, and two separate byte-level scanners re-implement fenced code detection that pulldown-cmark handles natively.

`tui-markdown` contributes a single call site (`gfm/render.rs:172`) where it converts plain markdown to styled `Line`s. It handles headings, bold, italic, code spans, lists, and blockquotes. These are all standard pulldown-cmark events that we could render directly.

## Intent

Consolidate parsing into a single pulldown-cmark pass per document body. Render all markdown elements directly to ratatui `Line`/`Span` types without the `tui-markdown` intermediary. Replace the byte-level diagram scanner with pulldown-cmark's native `CodeBlock` event detection.

This is a refactor. No user-visible behavior changes.

## Design

### Single-pass event renderer

Replace the current two-stage approach (extract GFM segments, then delegate to `tui-markdown`) with a single-pass pulldown-cmark event walker that produces `Vec<Line<'static>>` directly.

The walker handles all event types in one pass:

- `Tag::Heading` -- apply bold + color styling per level (matching current `tui-markdown` output)
- `Tag::Emphasis` / `Tag::Strong` -- push modifier onto a style stack
- `Tag::CodeBlock(kind)` -- detect language, route diagram blocks to existing diagram pipeline, render other code blocks with syntect highlighting (the `highlight-code` feature we currently get from `tui-markdown` uses syntect internally -- we'd use it directly)
- `Tag::Table` / `Tag::BlockQuote(Some(kind))` / `Tag::FootnoteDefinition` -- render with existing custom logic from `gfm/render.rs` (this code stays largely unchanged)
- `Tag::List` / `Tag::Item` -- render with bullet/number prefixes
- `Tag::Paragraph` -- handle line breaks and spacing
- `Tag::Link` -- render link text with underline, optionally show URL
- `Text` / `Code` / `SoftBreak` / `HardBreak` -- append to current line with active style

```
@draft MarkdownRenderer {
    lines: Vec<Line<'static>>,
    style_stack: Vec<Style>,
    list_stack: Vec<ListContext>,
    current_spans: Vec<Span<'static>>,
}
```

The style stack handles nesting (e.g. bold inside italic inside a list item) by composing modifiers as tags open and close.

### Diagram block detection via pulldown-cmark

Replace `extract_diagram_blocks()` in `diagram.rs` with a function that accepts pulldown-cmark `Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))` events and checks the language tag against `d2` and `mermaid`. The byte ranges come from pulldown-cmark's offset iterator, so the existing `PreviewSegment` split logic continues to work.

This eliminates the hand-rolled line scanner while preserving the same `DiagramBlock` output type.

### Code fence detection for @ref suppression

`code_fence.rs` exists solely to tell the @ref expander "don't expand inside fenced code." This can be replaced with pulldown-cmark's offset iterator as well, since the parser already tracks code block byte ranges. However, this is in the engine layer and the refactor is optional since it's small, correct, and has no dependency on `tui-markdown`.

### Syntect for code highlighting

`tui-markdown` uses syntect internally for its `highlight-code` feature. After dropping `tui-markdown`, we add `syntect` as a direct dependency. The highlighting logic is straightforward: load a syntax set, find the syntax by language tag, highlight line by line, map syntect styles to ratatui `Style`.

### Migration path

The existing GFM extractor state machines (`TableExtractor`, `AdmonitionExtractor`, `FootnoteExtractor`) fold naturally into the single-pass walker. Instead of running as separate extractors with byte-range tracking and reassembly, they become match arms in the walker's event loop. The rendering functions (`render_table`, `render_admonition`, `render_footnotes`) stay as-is since they operate on structured data, not pulldown-cmark events.

## Interface sketches

### Event walker

```
@draft fn render_markdown(body: &str, max_width: u16) -> (Vec<Line<'static>>, Vec<DiagramBlock>)
```

Returns both the rendered lines and any diagram blocks found during the parse. The caller uses the diagram blocks to drive the existing async rendering pipeline.

### Style stack

```
@draft struct StyleStack {
    stack: Vec<Style>,
    fn push(&mut self, modifier: Modifier),
    fn pop(&mut self),
    fn current(&self) -> Style,
}
```

### Code highlighter

```
@draft struct CodeHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
    fn highlight_block(&self, code: &str, language: &str) -> Vec<Line<'static>>,
}
```

## Stories

1. Single-pass markdown renderer -- replace `tui_markdown::from_str` with a pulldown-cmark event walker that renders directly to `Vec<Line<'static>>`. Drop the `tui-markdown` dependency. Subsumes the GFM extraction and rendering into one pass.

2. Diagram detection via pulldown-cmark -- replace the byte scanner in `diagram.rs` with pulldown-cmark `CodeBlock` event detection. Same output type, fewer moving parts.
