---
title: "Cache and Template"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, engine, cache]
related: []
---

# Cache

`DiskCache` stores expanded @ref content to avoid repeated git + tree-sitter work.

@ref src/engine/cache.rs#DiskCache

Location: `~/.lazyspec/cache/`

Cache keys combine three components:
- Path hash (identifies the document)
- Body hash (invalidates on content change)
- Cache version constant (invalidates on format change)

Format: `v{VERSION}_{PATH_HASH}_{BODY_HASH}`

Operations:
- `read(path, body_hash)` -- returns cached expansion or None
- `write(path, body_hash, expanded)` -- stores expansion
- `invalidate(path)` -- removes all cache entries for a path
- `clear()` -- removes all cache entries

# Template

The template module handles filename generation for new documents.

@ref src/engine/template.rs#resolve_filename

`resolve_filename(pattern, doc_type, title, dir)`:
1. Slugify the title (lowercase, replace non-alphanumeric with `-`, dedupe)
2. Find next sequential number by scanning the directory for existing prefixes
3. Substitute pattern variables (`{type}`, `{n:03}`, `{title}`, `{date}`)

@ref src/engine/template.rs#render_template

`render_template(template_content, vars)` does simple `{key}` -> `value` substitution
on template file content.
