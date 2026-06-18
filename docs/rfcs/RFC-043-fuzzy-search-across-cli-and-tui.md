---
title: Fuzzy search across CLI and TUI
type: rfc
status: accepted
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-042
---

## Problem

Search is substring-only. Engine `search()` (`src/engine/store.rs:308`) lowercases query + target and calls `.contains()`, returning one result per doc in nondeterministic HashMap-iteration order. TUI filter uses the same approach over a pre-lowercased `searchable` field (`src/tui/state/app.rs:27`, filter at `app.rs:794`). No subsequence matching, no typo tolerance, no ranking. Finding a doc requires recalling an exact contiguous substring.

## Intent

Replace substring match with fuzzy subsequence matching + relevance ranking, shared by CLI `search` and TUI filter. Matcher lives in the engine layer so CLI and TUI consume one implementation (dependency flow inward — neither surface owns search logic).

## Sketch

- Add the full `nucleo` crate (not only `nucleo-matcher`): an fzf-style streaming matcher with an injector, background worker pool, and per-frame ranked snapshots. Pure Rust, used by helix/zed.
- Engine `search()` returns scored, rank-sorted results with a deterministic tie-break (by path) and a score floor that drops non-matches.
- `@ref src/cli/search.rs` — `--json` output gains a `score` field; results ordered by score desc; no result cap.
- TUI filter consumes the same scorer; live results sorted by score, with matched characters highlighted in rows.
- Match surface: title + tags + path + **body**. The TUI sources body lazily through the `nucleo` injector to keep startup fast (see ADR-013, which supersedes ADR-002).

## Stories

- STORY-129 — Engine fuzzy matcher + ranked `search()` results (foundation).
- STORY-130 — TUI filter consumes fuzzy scorer (replace `.contains()` path), body coverage, matched-char highlight.
- STORY-131 — CLI search ranking + `score` in `--json`.

## Decisions

Resolved during planning (RFC open questions closed):

- **Body-text fuzzy match: yes.** Match over body in addition to title/tags/path. For the TUI this means body must reach the matcher; ADR-013 supersedes ADR-002 (frontmatter-only indexing) and specifies lazy body load streamed through the `nucleo` injector, preserving fast startup.
- **Result cap / score floor:** engine applies a score floor that excludes non-matches; no hard result cap on CLI `--json` (agents may want all); TUI is bounded by the viewport.
- **Matched-char highlight in TUI: yes** (STORY-130), using the matcher's match indices.

