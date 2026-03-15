---
title: "Configuration"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, data-model, config]
related:
  - related-to: "docs/stories/STORY-037-config-driven-type-definitions.md"
  - related-to: "docs/stories/STORY-038-config-driven-validation-rules.md"
---

# Configuration

Configuration lives in `.lazyspec.toml` at the project root. Every field is optional;
sensible defaults apply when omitted. See [STORY-037: Config-driven type definitions](../../stories/STORY-037-config-driven-type-definitions.md)
and [STORY-038: Config-driven validation rules](../../stories/STORY-038-config-driven-validation-rules.md).

@ref src/engine/config.rs#Config

## Full Schema

```toml
# Document types (repeatable)
[[types]]
name = "rfc"           # lowercase type name
plural = "rfcs"        # plural form for display
dir = "docs/rfcs"      # directory to scan
prefix = "RFC"         # filename prefix for numbering
icon = "●"             # optional TUI icon

# Validation rules (repeatable)
[[rules]]
shape = "parent-child"         # or "relation-existence"
name = "stories-need-rfcs"     # human-readable rule name
child = "story"                # child type (parent-child only)
parent = "rfc"                 # parent type (parent-child only)
link = "implements"            # required relation type
severity = "warning"           # "error" or "warning"

[[rules]]
shape = "relation-existence"
name = "adrs-need-relations"
type = "adr"
require = "any-relation"
severity = "error"

[templates]
dir = ".lazyspec/templates"    # template directory

[naming]
pattern = "{type}-{n:03}-{title}.md"  # filename pattern

[tui]
ascii_diagrams = false         # force ASCII diagram rendering
```

## Type Definition

@ref src/engine/config.rs#TypeDef

## Validation Rule Shapes

@ref src/engine/config.rs#ValidationRule

## Default Validation Rules

Three rules ship by default:

@ref src/engine/config.rs#default_rules

## Template Variables

Templates in `.lazyspec/templates/` support variable substitution:

| Variable | Value |
|---|---|
| `{title}` | Document title |
| `{author}` | Author name |
| `{date}` | Current date (YYYY-MM-DD) |
| `{type}` | Document type (uppercase) |

## Naming Pattern Variables

| Variable | Value |
|---|---|
| `{type}` | Type prefix (uppercase) |
| `{title}` | Slugified title |
| `{date}` | Current date |
| `{n}` | Next sequential number |
| `{n:03}` | Next number, zero-padded to 3 digits |
