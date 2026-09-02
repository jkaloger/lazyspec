---
title: Declare edges from the configure-type write steps
type: story
status: draft
author: Jack Kaloger
date: 2026-09-02
tags: []
related:
- implements: RFC-067
---

As a DAG designer running `/configure-type`, I want the skill's write steps to declare the new type's edges, so that a type it creates participates in the DAG instead of shipping with no edges.

## Acceptance criteria

- Given the interview, when the user names a type the new one points at, then the write steps emit a `config add-edge` call with `from`, `to`, `via` and `required` drawn from the answers.
- Given a type that points at nothing, when the interview ends, then the skill says so and writes no edge rather than inventing one.
- Given the verification checklist, when it runs, then it reads `edges` from `config --json` and confirms the declared rows are present.

## Notes

Recorded by ITERATION-400 and confirmed by the STORY-262 review: `skills/configure-type/SKILL.md` tells the agent that a DAG constraint is a separate `[[edges]]` row and then never instructs it to write one. `config add-edge` exists since ITERATION-392.
