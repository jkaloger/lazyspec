---
title: Body content available for fuzzy match via lazy streaming index
type: adr
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- supersedes: ADR-002
- related-to: RFC-043
---

## Context

ADR-002 established frontmatter-only indexing: at startup the store reads only YAML frontmatter from each file, and body content is loaded lazily via `Store::get_body` when a document is previewed. The stated consequence was that TUI fuzzy search operates on frontmatter fields only (title, tags, author); body content is never cached. The rationale was fast startup regardless of corpus size or body length.

RFC-043 introduces fuzzy, ranked search shared by the CLI `search` command and the TUI filter, with the matcher living in the engine layer. During planning it was decided that the TUI filter must match body content, not just frontmatter, so that finding a document does not depend on recalling a metadata token. That requirement makes body text reachable by the TUI matcher, which directly contradicts ADR-002's frontmatter-only constraint for the TUI.

## Decision

Supersede ADR-002. Body content becomes available to fuzzy matching in both the CLI and the TUI. To preserve ADR-002's fast-startup invariant rather than abandon it, body is not loaded eagerly at startup. Instead:

- Adopt the full `nucleo` crate (not only `nucleo-matcher`). `nucleo` is a streaming, fzf-style matcher: an injector feeds candidates, a background worker pool scores them, and the UI reads a per-frame snapshot of ranked results.
- At startup, inject document metadata (title, tags, path) immediately, so the list is interactive instantly. Startup cost is unchanged from ADR-002.
- A background task reads document bodies lazily and injects them into the matcher as they load. Body matches stream into results without blocking the UI.
- Loaded bodies are cached (reusing the engine `DiskCache`) so warm runs skip the disk read; file-watch events invalidate cached bodies on change.

## Consequences

- TUI fuzzy search spans body content, not only frontmatter fields.
- Startup latency is preserved (metadata-only at boot). First body matches appear shortly after launch as the background read completes, the same perceived behavior as helix/zed.
- Steady-state memory grows by the size of cached bodies, bounded by cache policy. ADR-002's "body content is never cached in the store" no longer holds; bodies are now cached deliberately.
- CLI body search, already permitted under ADR-002 as an acceptable one-shot operation, now runs through the same engine scorer as the TUI.
- Adds the heavier `nucleo` dependency footprint over the matcher-only `nucleo-matcher` originally sketched in RFC-043.

Supersedes ADR-002.
