---
title: Adopt comment-less github issue body on ordinary relation merge
type: iteration
status: complete
author: jkaloger
date: 2026-07-24
tags: []
related:
- implements: STORY-246
- related-to: BUG-014
---

## Objective

`link`/`unlink` of ordinary relation succeed on github-issues doc with comment-less remote body — first write adopts issue, prose preserved.

## Satisfies

STORY-246 AC1–AC5 (single coupled slice: fallback + adopt + preserve).

## Context

- Story + design: STORY-246 §Design
- Root cause + repro: BUG-014
- Fix site: `merge_relation_to_remote` src/engine/store_dispatch.rs (hard `?` on `issue_body::deserialize`)
- Fallback precedent to mirror: `parse_issue` src/engine/issue_cache.rs:853 (deserialize fail → synthesized meta, whole body = prose)
- Deserialize/error origin: `extract_comment` src/engine/issue_body.rs:351
- Convention: principle 6 — extract shared fallback helper only if two call sites fall out naturally

## Tasks

1. Test-first (MockGhClient): remote body `null`/empty → link merges relation, `issue_edit` body carries lazyspec comment + relation (AC1).
2. Test: remote body prose-only, no comment → link succeeds, prose verbatim under new comment (AC3).
3. Test: unlink same relation on adopted issue → relation removed from comment + cache (AC2).
4. Impl: in `merge_relation_to_remote`, deserialize failure → fallback meta from remote issue fields (title/labels→type+tags, state→lifecycle status, created date, empty related), body = prose; then existing delta+serialize path.
5. Regression: native rel link/unlink tests untouched/green (AC4).

## Out of scope

- clickup merge path, `push_cache`/`check_lock` same-class audit — follow-up if affected (story §Design).
- Fetch side — already tolerant.

## Verification

Live repro from BUG-014: `lazyspec link PARENT-88 related-to PARENT-64` succeeds; relation in `context --json` + Relations tab. Full check: `cargo fmt --check`, `cargo clippy`, `cargo test`.

