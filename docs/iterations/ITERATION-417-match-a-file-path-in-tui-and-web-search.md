---
title: Match a file path in TUI and web search
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-269
---

## Objective

A file path typed into existing search lists the documents whose globs match it, additive to the text matches.

## Satisfies

STORY-269 AC3, AC4, AC5. Closes the story.

## Context

- Story + ACs: STORY-269
- Overloading search rather than adding a screen, and results being additive: RFC-068 §Design "Surfaces", §Risks "Search overloading", §Non-goals
- Matching goes through `governing` (ITERATION-408). Neither surface reimplements glob matching -- CONVENTION principle 3.
- Touch:
  - `src/engine/store.rs:350` `Store::search` and `:498` `SearchCorpus::search` -- the shared seam. Adding path matches there covers both surfaces; check which one each surface actually calls before choosing.
  - TUI: search state in `src/tui/state/app.rs` around `:3618`, fuzzy-match tests from `:5599`.
  - Web: `src/web/routes.rs:274` `search`, calling `Store::search` at `:286`; `templates/search_fragment.html`.
- Additive means union, deduped -- not fallback. Do not gate the path lookup on the text search coming back empty (AC5).
- A query is a path candidate whenever `governing` matches it. No sigil, no slash heuristic, no prefix.

## Tasks

1. Test-first at the shared seam: a query equal to a governed file path returns the governing documents; a title query returns the title match; a query matching both returns both, deduped (AC5).
2. Implement in the shared search path.
3. Test-first in `app.rs`: TUI `/` with a file path lists the governing documents (AC3).
4. Test-first in the web integration tests: `/search?q=<path>` renders them (AC4).
5. Wire whichever surface does not already route through the seam.

## Out of scope

- Ranking path matches against text matches, or labelling which kind a row is. That is a new column; RFC-068 §Non-goals. `why` is the precise answer.
- Fuzzy path matching. Globs are exact.
- Detail rendering -- ITERATION-416.

## Principles/conventions

`cargo run --quiet -- convention`. Principle 3: the engine matches, the surfaces call.

## Verification

With this repo's pins, TUI `/` and `/search?q=src/cli/show.rs` both list exactly what `cargo run --quiet -- why src/cli/show.rs --json` names.
