---
title: Body content available for fuzzy match via lazy streaming index
type: adr
status: accepted
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

- Both surfaces call the engine's `Store::search`, which owns all matching. The engine uses `nucleo`'s `Pattern`/`Matcher` to fuzzy-score each document across title, tags, path, and body, returning ranked results with a score floor. The TUI reuses this exact path so no scoring logic leaks into the TUI layer (principle 3).
- At startup nothing reads a body: `Store` holds an empty `body_cache` and the list is interactive from frontmatter immediately. Startup cost is unchanged from ADR-002.
- Bodies are read lazily on first search and memoized in an in-memory `body_cache` on `Store`, so later keystrokes score from memory. File-watch events (`reload_file`/`remove_file`) drop the changed path's cached body, keeping the cache coherent.

### Deviation from the original streaming design (implemented in ITERATION-341)

The design first drafted here — the full `nucleo` streaming injector with a background worker pool and per-frame snapshots, bodies streamed in as they load, cached via the engine `DiskCache` — was **not** built. Driving a `Nucleo<T>` instance would require the TUI to hold matcher state and aggregate snapshots itself, duplicating the engine scorer and weakening the engine-owns-matching boundary. Routing the TUI through `Store::search` with a memoized in-memory body cache delivers the substantive goals (body coverage, ranking, score floor, metadata-only fast startup, cached bodies with invalidation) while keeping 100% of matching in the engine.

Accepted tradeoff: the first-ever search keystroke reads every document body from disk synchronously on the UI thread (once), and each subsequent keystroke re-scores cached bodies on the UI thread — O(corpus body size) per keystroke, single-threaded. This is imperceptible at lazyspec's scope (a simple structured-markdown doc tool, small corpora) and is the reason the background worker pool was not warranted. On a multi-thousand large-doc corpus it would stall; that is a scope-bounded acceptance, not a defect. `DiskCache` was likewise not reused: raw bodies already live on disk as the source files, so disk-caching them is redundant.

## Consequences

- TUI fuzzy search spans body content, not only frontmatter fields.
- Startup latency is preserved (metadata-only at boot; `body_cache` empty).
- Steady-state memory grows by the size of cached bodies, held in the in-memory `body_cache`. ADR-002's "body content is never cached in the store" no longer holds; bodies are now cached deliberately.
- CLI body search, already permitted under ADR-002 as an acceptable one-shot operation, now runs through the same engine scorer as the TUI.
- The `nucleo` dependency is used only for its `Pattern`/`Matcher` scoring, not the streaming injector; the lighter `nucleo-matcher` would have sufficed, but the full crate is retained for parity with RFC-043 and future streaming if corpus scale ever demands it.

Supersedes ADR-002.
