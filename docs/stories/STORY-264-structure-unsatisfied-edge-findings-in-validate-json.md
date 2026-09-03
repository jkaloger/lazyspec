---
title: Structure unsatisfied-edge findings in validate --json
type: story
status: draft
author: Jack Kaloger
date: 2026-09-02
tags: []
related:
- implements: RFC-067
---

As an agent assembling a boundary report, I want `validate --json` findings to carry the edge's structure, so that I read the edge name, the admitted types and the severity as fields instead of parsing a sentence.

## Acceptance criteria

- Given an unsatisfied edge, when `validate --json` reports it, then the finding carries `edge`, `from`, `to`, `via`, `severity` and the document id as separate fields alongside the rendered message.
- Given `/lazy`'s Stop-at-Type-Boundary, when this lands, then its prose reads the set from the finding and drops the instruction to cross-reference `config --json` for it.
- Given the human-readable output, when nothing else changes, then its wording is unchanged.

## Notes

Recorded by ITERATION-401 and ITERATION-402. The shipped workaround, "do not string-parse the finding, read `config --json` instead", is prose in `skills/lazy/SKILL.md` and stays wrong-shaped until this lands. `UnsatisfiedEdge` in `src/engine/validation.rs` already holds the fields; the JSON renderer flattens them.
