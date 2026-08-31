---
title: Gate document creation on the target's status
type: story
status: rejected
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
- related-to: ADR-033
---

**Withdrawn 2026-08-31 — see ADR-033.** `create` takes no target document, so this story's gate could only be an existence check across the project: "does any story sit at `accepted`?" That is a hard wall on an empty project and permanently satisfied once one story has ever been accepted. The acceptance criteria below describe per-document targeting the command does not have, and the refusal contradicts RFC-067's own rule that an unsatisfied edge is a finding, never a refused command. Status-conditioned `create` gating is abandoned rather than re-carried; the implementation was reverted. Criteria retained below as the record of what was asked for.

---

As a DAG designer, I want `create` refused until the intended parent reaches a named status, so that planning settles before delivery work starts.

ADR-022 established status-conditioned gating over a phase axis, hanging the gate on the parent-child rule as a scalar `require_parent_status`. That decision stands; only its carrier moves. A scalar cannot gate a set whose members run different lifecycles.

## Acceptance criteria

- Given an edge `from = "iteration"`, `to = ["story", "bug"]` with `require_to_status = { story = "accepted", bug = "triaged" }`, when the author runs `create iteration` against a story at `draft`, then the command is refused and names the required status.
- Given the same edge, when the author creates against a bug at `triaged`, then creation succeeds — the gate is read per target type, not globally.
- Given the same edge, when the author creates against a bug at `reported`, then the command is refused. `bug` has no `accepted` state, so a scalar gate could not have expressed this.
- Given an edge whose `require_to_status` omits a member of `to`, when the author creates against that member, then creation succeeds — an absent key means ungated for that target type.
- Given a `require_to_status` key naming a status absent from that type's lifecycle, when the config loads, then load fails naming the type, the status, and the edge.
- Given any refusal, when `--json` is passed, then the refusal is machine-readable and carries the edge name, target type, current status, and required status.

## Notes

Replaces the scalar gate at `src/engine/ops/create.rs:87-99`.

The load-time lifecycle check is the interesting part: a typo in a status name should fail at load, not silently gate nothing at create time.
