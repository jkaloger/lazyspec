---
title: Status-conditioned gates instead of a phase axis
type: adr
status: superseded
author: jkaloger
date: 2026-06-21
tags:
- status
- gates
- workflow
- config
related:
- related-to: RFC-048
---

## Context

RFC-048 must let a planning phase settle before work moves into building: teams write planning docs (`spec`/`rfc`/`adr`), refine into intermediate docs (`feature`/`story`), and only then break a story into an `iteration`. The skills must not aggressively drive from planning into delivery.

Options for representing this: an explicit ordered `phase` axis with gated boundaries, a per-type `handoff` flag, or status-conditioned gates on the existing parent-child rules. A `phase` axis is a new first-class concept; the real handoff in practice is status-conditioned (you don't refine an RFC into stories until it is `ratified`).

## Decision

No phase/banding axis. The parent-child validation rule gains an optional `require_parent_status`: a child type is creatable only once its parent reaches a named status. `create <child>` refuses when the gate is unmet.

Non-aggression is structural from two facts together: the gate makes downstream creation ineligible until upstream approval, and the skill never auto-crosses a type boundary (spawning a child of a different type is always a human-initiated step; only within-doc progression flows automatically). Banding is implicit in the gates plus the existing DAG topology.

## Consequences

- No new axis to learn or maintain; gates reuse the parent-child rule and the status DAG (ADR-021/023).
- The planning→delivery handoff is enforced by config, not by prompt-begging the agent.
- A coarse "phase" view, if ever wanted for display, is derivable from topology + gates rather than stored.

Locked for RFC-048.
