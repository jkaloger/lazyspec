---
title: Depth-bounded related traversal with distance tagging
type: adr
status: draft
author: jkaloger
date: 2026-06-17
tags:
- context
- relationships
related:
- related-to: STORY-122
- related-to: RFC-007
---

## Summary

`lazyspec context` collects `related-to` records by a breadth-first traversal
bounded by a `--depth N` flag (default 1), and tags each surfaced document with
its relation type, hop distance, and the path it was reached through.

## Context

Related records are gathered one hop from the chain: for every chain member, its
direct `related-to` links in both directions. A document related to a record
that is itself related to the chain never surfaces. The motivating case is an
ADR related to an RFC that a Story relates to: a planning agent needs the
decisions constraining the RFC, but they sit two hops out and are invisible.

Different consumers need different breadth. A build agent implementing tasks
needs less surrounding context than a planning agent that must understand the
constraining decisions. Encoding that policy in the engine would calcify it;
the traversal depth belongs to the caller.

## Decision

- Add a `--depth N` flag to `context`. Default `1` reproduces current behavior
  exactly (backward compatible).
- For `N >= 2`, traverse `related-to` links breadth-first up to N hops from the
  chain, deduplicated by path. Documents beyond N hops do not appear.
- Tag every surfaced related (and forward) document in `--json` with:
  - `relation`: the link type it was reached by
  - `distance`: hop count from the chain
  - `via`: the path of the document it was reached through
- The depth knob lives in the engine/CLI; per-role policy (which depth a
  planning vs build agent requests) lives in the consuming skill, not here.

## Consequences

- Tagging lets a skill filter the surfaced set by distance without re-walking
  the graph, which is the seam that keeps fidelity policy out of the engine.
- Higher depths can surface large related sets; bounding output by fidelity
  (summary vs full text by distance) is a skill concern, deliberately not
  solved in the engine.
- The `--json` schema for related/forward entries gains three fields; the
  default-depth output otherwise matches today's.
