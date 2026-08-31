---
title: Status-conditioned create gating is abandoned, not relocated
type: adr
status: accepted
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- related-to: RFC-067
- supersedes: ADR-022
---

## Context

ADR-022 chose status-conditioned gating over a phase axis and hung it on the parent-child rule as `require_parent_status`, with the explicit consequence that "`create <child>` refuses when the gate is unmet". RFC-067 carried that forward onto the edge table as `require_to_status`, a per-target-type map, and STORY-255 shipped it in `create`.

Implementation exposed what the gate actually is. `create` takes no target document -- `--parent` controls directory nesting only -- so the gate could only ask "does *any* document of that type sit at the required status?" On an empty project that is a hard wall with no escape hatch; on a project where one story has ever reached `accepted` it is permanently satisfied and never fires again. A one-time speed bump, not a workflow gate. STORY-255's acceptance criteria ("create against a story at `draft` is refused") describe per-document targeting the command does not have.

The refusal also contradicted the rest of the same RFC. RFC-067 §Design states "an edge absent from the table is a finding, never a refused command", and STORY-262 already wanted the agent to *report* an unmet gate rather than propose a create that would fail. Dictum 1 puts the tool's job at producing, validating and serving structured markdown: validation reports findings, it does not police authoring.

## Decision

No lazyspec command refuses a create because of another document's status. `require_to_status` is removed from `EdgeDef`, the config schema and the README; the scalar `require_parent_status` gate in `create` is left to die with `[[rules]]` in STORY-259 and gets no successor. Status-conditioned gating is abandoned, not relocated -- this supersedes ADR-022 rather than moving its carrier again. Edges describe the DAG and drive validation findings only.

## Consequences

Authoring is never blocked by the state of some other document, so a fresh project can write its first iteration without first dragging a story to `accepted`, and no `--force` escape hatch has to be invented. The edge table gains one coherent policy where it had two: every edge condition is a finding.

The planning-to-delivery handoff loses its config enforcement. ADR-022 rested non-aggression on two facts together -- the gate, and the skill never auto-crossing a type boundary. Only the second remains, so an agent that ignores its instructions can now author an iteration against a `draft` story and nothing stops it; the misordering surfaces in `validate` after the fact rather than at the moment of authoring. Teams that wanted the hard gate have no way to express it.

## Revisit when

- Agents are observed crossing the planning-to-delivery boundary in practice, and a `validate` finding after the fact proves too weak to correct it.
- `create` gains a real target argument, making a gate a check on one named document rather than an existence query across the project -- the defect that killed this one would no longer apply.
