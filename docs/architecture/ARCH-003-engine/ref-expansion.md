---
title: "Ref Expansion"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, engine, refs]
related:
  - related-to: "docs/rfcs/RFC-019-inline-type-references-with-ref.md"
  - related-to: "docs/stories/STORY-055-ref-parsing-and-expansion-pipeline.md"
  - related-to: "docs/stories/STORY-058-ref-expansion-hardening-and-performance.md"
---

# Ref Expansion

The `RefExpander` resolves `@ref` directives by shelling out to `git show` to
fetch file content at a given revision. Specified in
[RFC-019: Inline type references with @ref](../../rfcs/RFC-019-inline-type-references-with-ref.md).

@ref src/engine/refs.rs#RefExpander

## Expansion Pipeline

```d2
direction: down

input: "@ref src/store.rs#Store@abc123" {
  shape: parallelogram
}

parse: "Parse directive" {
  path: "src/store.rs"
  symbol: "Store"
  sha: "abc123"
}

git: "git show abc123:src/store.rs" {
  shape: hexagon
}

extract: "Extract symbol" {
  decision: "Symbol type?" {
    shape: diamond
  }
  line_num: "Line number -> single line"
  named: "Named -> tree-sitter extract"
  whole: "None -> whole file"
}

truncate: "Truncate to max_lines"

output: "Fenced code block with caption" {
  shape: parallelogram
}

input -> parse -> git -> extract
extract.decision -> extract.line_num: "digits"
extract.decision -> extract.named: "identifier"
extract.decision -> extract.whole: "none"
extract.line_num -> truncate
extract.named -> truncate
extract.whole -> truncate
truncate -> output
```

The expander:
- Skips refs inside existing fenced code blocks (prevents recursion)
- Falls back to `> [unresolved: path#symbol]` on failure
- Supports cancellation via `AtomicBool` for async TUI expansion
- Resolves HEAD SHA once per expansion pass for consistent captions

See [STORY-058: Ref expansion hardening and performance](../../stories/STORY-058-ref-expansion-hardening-and-performance.md)
for planned improvements.
