---
title: "Engine"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, engine]
related:
  - related-to: "docs/rfcs/RFC-001-my-first-rfc.md"
  - related-to: "docs/rfcs/RFC-008-project-health-awareness.md"
  - related-to: "docs/rfcs/RFC-019-inline-type-references-with-ref.md"
  - related-to: "docs/stories/STORY-001-document-model-and-store.md"
---

# Engine

The engine module (`src/engine/`) contains all domain logic. It has no knowledge
of CLI arguments, terminal rendering, or user interaction. Both the CLI and TUI
depend on it.

The engine implements the core from [RFC-001](../../rfcs/RFC-001-my-first-rfc.md),
with capabilities added across several RFCs:

- [RFC-008: Project Health Awareness](../../rfcs/RFC-008-project-health-awareness.md) drove validation
- [RFC-013: Custom document types](../../rfcs/RFC-013-custom-document-types.md) made the type system configurable
- [RFC-015: Lenient frontmatter loading](../../rfcs/RFC-015-lenient-frontmatter-loading-with-warnings-and-fix-command.md) added error tolerance
- [RFC-019: Inline type references with @ref](../../rfcs/RFC-019-inline-type-references-with-ref.md) added code references

## Component Diagram

```d2
direction: down

config: Config {
  style.fill: "#fff3e0"
  load: "load(.lazyspec.toml)"
  types: "Vec<TypeDef>"
  rules: "Vec<ValidationRule>"
}

store: Store {
  style.fill: "#e8f0fe"
  docs: "HashMap<PathBuf, DocMeta>"
  forward_links: "HashMap<PathBuf, Vec<(Rel, Path)>>"
  reverse_links: "HashMap<PathBuf, Vec<(Rel, Path)>>"
  children: "HashMap<PathBuf, Vec<PathBuf>>"
  parent_of: "HashMap<PathBuf, PathBuf>"
}

document: Document {
  style.fill: "#e8f5e9"
  DocMeta: "DocMeta"
  split_fm: "split_frontmatter()"
  rewrite: "rewrite_frontmatter()"
}

validation: Validation {
  style.fill: "#fce4ec"
  result: "ValidationResult"
  issues: "Vec<ValidationIssue>"
}

refs: RefExpander {
  style.fill: "#f3e5f5"
  expand: "expand(content)"
  resolve_ref: "resolve_ref()"
}

symbols: Symbols {
  style.fill: "#e0f2f1"
  trait_def: "SymbolExtractor trait"
  ts: "TypeScriptSymbolExtractor"
  rust: "RustSymbolExtractor"
}

cache: DiskCache {
  style.fill: "#f5f5f5"
  read: "read(path, hash)"
  write: "write(path, hash, body)"
}

template: Template {
  style.fill: "#fffde7"
  render: "render_template()"
  filename: "resolve_filename()"
}

config -> store: "type dirs to scan"
store -> document: "parse each .md"
store -> validation: "validate_full()"
refs -> symbols: "extract_symbol()"
refs -> cache: "cache expanded output"
template -> store: "next_number() from dir"
```

Children cover each subsystem:
- **store**: Document index, links, hot reload, shorthand resolution, search
- **validation**: Rule engine, issue types, hierarchy checks
- **ref-expansion**: @ref directive resolution via git + tree-sitter
- **symbol-extraction**: Tree-sitter based code symbol extraction
- **cache-and-template**: Disk caching and filename generation
