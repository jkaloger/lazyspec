---
title: "Ref Directives"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, data-model, refs]
related:
  - related-to: "docs/rfcs/RFC-019-inline-type-references-with-ref.md"
  - related-to: "docs/stories/STORY-055-ref-parsing-and-expansion-pipeline.md"
---

# @ref Directives

Documents can reference source code inline using `@ref` directives. These are
expanded on display (not stored expanded). Specified in
[RFC-019: Inline type references with @ref](../../rfcs/RFC-019-inline-type-references-with-ref.md),
implemented via [STORY-055: Ref parsing and expansion pipeline](../../stories/STORY-055-ref-parsing-and-expansion-pipeline.md).

## Syntax

The regex that drives ref parsing:

@ref src/engine/refs.rs#REF_PATTERN

```
@ref <path>                    # entire file
@ref <path>#<symbol>           # named symbol (struct, function, etc.)
@ref <path>#<line>             # specific line number
@ref <path>#<symbol>@<sha>     # symbol at specific git commit
```

## Expansion

Refs are expanded into fenced code blocks with a caption showing the file path,
git SHA, and symbol name. Long expansions are truncated (default 25 lines) with
a language-appropriate comment indicating remaining lines.

Refs inside existing fenced code blocks are not expanded (prevents recursive expansion).
