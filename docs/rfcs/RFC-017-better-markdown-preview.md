---
title: Better markdown preview
type: rfc
status: draft
author: "@jkaloger"
date: 2026-03-08
tags:
  - tui
  - markdown
  - d2
---

## Problem

The TUI preview panel renders markdown through `tui_markdown::from_str()` with default options. This means several common markdown features don't work:

- **GFM extensions are disabled.** Tables, footnotes, and admonition blocks (`> [!NOTE]`, `> [!WARNING]`) are not rendered. The underlying `pulldown-cmark` parser supports these via `ENABLE_TABLES` and `ENABLE_GFM` flags, but `tui-markdown` defaults don't enable them.
- **Diagram blocks render as plain code.** D2 and mermaid snippets appear as raw text with no visual representation of the diagram.
- **No way to open the current document in an editor.** Users have to leave the TUI, find the file path, and open it manually.

Syntax highlighting already works via `tui-markdown`'s `highlight-code` feature (syntect), so that item from the original notes is resolved.

## Intent

Upgrade the markdown preview to support GFM extensions, render d2/mermaid diagrams as inline images, and add an `e` keybinding to open the current document in `$EDITOR`.

## Design

### 1. Enable GFM extensions

`tui-markdown` exposes `from_str_with_options()` which accepts a pulldown-cmark `Options` bitflag set. Switch from `from_str()` to `from_str_with_options()` with at minimum:

- `ENABLE_TABLES`
- `ENABLE_GFM` (admonitions: `> [!NOTE]`, `> [!WARNING]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!CAUTION]`)
- `ENABLE_FOOTNOTES`
- `ENABLE_STRIKETHROUGH` (already on by default, keep it)
- `ENABLE_TASKLISTS` (already on by default, keep it)

This is a low-risk change since the parser already supports these features, they just need to be opted into.

### 2. Diagram rendering (d2 and mermaid)

Render fenced code blocks tagged with `d2` or `mermaid` as inline images using the terminal's image protocol.

**Pipeline:**

1. During markdown body processing, detect fenced code blocks with language `d2` or `mermaid`
2. Shell out to the appropriate CLI (`d2` for d2, `mmdc` for mermaid) to render the source to a PNG in a temp/cache directory
3. Display the rendered PNG inline using the terminal image protocol (sixel or kitty graphics protocol)
4. Cache rendered images keyed on a hash of the diagram source text, so re-renders only happen when content changes

**Terminal image protocol detection:**

Query the terminal for capability support at TUI startup:
- Check `$TERM_PROGRAM` / `$TERM` environment variables for known-good terminals (kitty, iTerm2, WezTerm)
- Alternatively, send a device attributes query and parse the response
- Store the detected protocol as an enum: `Sixel`, `KittyGraphics`, or `None`

**Fallback:** When image protocol is not available, or when the CLI tool is not installed, fall back to rendering the raw code block with syntax highlighting (current behaviour). Display a small hint like `[d2: install d2 CLI for diagram rendering]` or `[d2: terminal does not support inline images]`.

**Rendering happens asynchronously**, following the same pattern as `@ref` expansion: trigger in background, cache the result, redraw when ready. A loading indicator (`[rendering diagram...]`) displays while the external process runs.

> [!NOTE]
> This requires `d2` and/or `mmdc` (mermaid CLI) to be installed on the user's system. lazyspec does not bundle these tools.

### 3. Open in `$EDITOR`

Add an `e` keybinding in both the normal preview view and fullscreen view:

1. Read `$EDITOR` (fall back to `$VISUAL`, then `vi`)
2. Suspend the TUI (release the terminal)
3. Spawn the editor with the document's file path as argument
4. On editor exit, restore the TUI and reload the document (it may have been edited)

This follows the same suspend/restore pattern already used for other subprocess launches in the codebase.

## Interface sketches

### Terminal capability detection

```
@draft TerminalImageProtocol { Sixel, KittyGraphics, None }
```

### Diagram cache entry

```
@draft DiagramCacheEntry {
    source_hash: u64,
    image_path: PathBuf,
    protocol: TerminalImageProtocol,
    rendered_at: SystemTime,
}
```

### Diagram renderer trait

```
@draft DiagramRenderer {
    fn render(source: &str, lang: DiagramLanguage, output: &Path) -> Result<()>;
    fn is_available() -> bool;
}
```

Where `DiagramLanguage` is `D2 | Mermaid`.

## Stories

1. **GFM extensions** -- Enable GFM options in the markdown parser. Tables, admonitions, and footnotes render correctly in the preview.

2. **Diagram rendering pipeline** -- Detect d2/mermaid code blocks, shell out to render PNGs, display inline via terminal image protocol with async rendering and caching. Graceful fallback when tools or protocol are unavailable.

3. **Open in editor** -- `e` keybinding suspends TUI, opens document in `$EDITOR`, restores TUI and reloads on return.
