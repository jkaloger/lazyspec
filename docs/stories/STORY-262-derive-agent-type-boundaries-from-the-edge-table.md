---
title: Derive agent type boundaries from the edge table
type: story
status: in-progress
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
---

As an agent following the lazyspec workflow, I want type boundaries from one source, so that I stop deriving them from the union of two mechanisms that can disagree.

The `/lazy` and `/execute` skills currently instruct agents to read boundaries from `parent_type` edges *and* parent-child rules, taking the union, with explicit warnings that a config may encode the DAG entirely in one or the other. With the edge table that instruction collapses.

## Acceptance criteria

- Given the `/lazy` skill prose, when this story lands, then boundary derivation reads the edge table, and the union-of-two-sources instruction is gone.
- Given the `/execute` skill prose, when this story lands, then the same holds.
- Given `parent_type`, when the prose describes it, then it is described as containment — shared store backend and directory nesting — and explicitly not as a linkage constraint.
- Given an edge with a set-valued `to`, when an agent reports a boundary crossing, then it names every permitted target type, so the human is offered the real choice rather than one arbitrary member.
- Given a required edge with nothing satisfying it, when the agent reaches that boundary, then it reports the unsatisfied edge at the severity `required` gave it, and never as a status a parent must reach before a `create` is permitted.

## Notes

Depends on STORY-257: the prose should describe traversal as it actually behaves once edges own it.

The set-valued reporting criterion matters most. An agent that says "create a story" when a spike or bug would do is the same failure as a validation message that names one permitted type out of three.

The last criterion was written as "given a gated edge, when the gate is unmet, then the agent reports the gate rather than proposing a `create` that will be refused". No such edge exists: ADR-033 abandoned status-conditioned create gating outright and RFC-067 §Design gives the edge table one policy — a row is satisfied or unsatisfied, and every unsatisfied row is a validation finding, not a refusal. ITERATION-402 amended the criterion to name what the binary actually reports. The behaviour it asks for is unchanged; only its premise is.
