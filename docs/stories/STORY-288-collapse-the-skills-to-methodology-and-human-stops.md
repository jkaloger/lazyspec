---
title: Collapse the skills to methodology and human stops
type: story
status: draft
author: Jack Kaloger
date: 2026-09-12
tags: []
related:
- implements: RFC-071
---

## Context

As an agent reading a skill, I want it to carry only methodology and the stops that stay human, so that the rules that remain are salient instead of buried under repetitions of rules the binary now refuses.

`lazy` restates the boundary rule in five places across 219 lines. Roughly 70 `Do NOT` lines exist across the set and about 8 are backed by anything. Once `next` answers the routing and the hooks refuse the edits, the rest is drift waiting to happen. The measurement ships inside this slice: a baseline recorded against the current skills is worth nothing on its own, and a collapse with no before-reading cannot be judged.

## Acceptance Criteria

- **Given** the `evals/` harness and its `.claude-plugin` manifest entry (neither exists today), **when** I run `claude plugin eval`, **then** cases for boundary crossing, direct document edit, and advance without approval run and report per-case.
- **Given** those cases run against the current skills **before** any collapse, **then** the outcome is recorded and committed as the baseline — recorded, not asserted as a pass.
- **Given** the collapsed `lazy`, **then** it is under 80 lines, its flow is `next` -> present -> stop when `requires_approval` -> dispatch -> `validate --id`, and its frontmatter description is triggers only.
- **Given** the skill set, **then** no NEVER or BODY-CONTENT block appears in more than one skill, and no skill restates a rule the binary refuses. `lazy` keeps one line: "Always `--json`."
- **Given** the draft-to-review advance, **then** `generate` and `co-write` own it, `lazy` names it as the single exception to the first-mutation approval stop, and the two no longer contradict.
- **Given** the single-unit path, **then** `execute` owns the work-open advance and `lazy` names one commit after `review-work` and `/advance`.
- **Given** `create-audit`, **then** it reads its type and relation from config rather than hardcoding `audit` and `related-to`.
- **Given** the RED-FLAGS table, **then** it keeps the four approval rows and drops "nothing refuses the create".
- **Given** one line in the collapsed set, **then** it names `--body-file` as the route for a body too large for `--body`.
- **Given** a test in `src/cli/skills.rs`, **then** it asserts no skill contains the jq reverse-lookup block; skill pinning tests are updated.
- **Given** the suite re-run against the collapsed skills, **then** the three documented failures pass where the baseline did not.
- **Given** `skills/README.md`, **then** it says: the binary answers, the hook delivers, the skill decides.

## Scope

### In Scope

- `evals/` harness, manifest entry, three cases, recorded baseline.
- Collapse of every skill, `skills/README.md`, `agents-md` regeneration, pinning tests, eval re-run.

### Out of Scope

- Changing the `agents-md` runtime beyond regenerating from the collapsed skills.
- Any new mechanism. This slice only removes prose the mechanisms replaced.
- Cases for behaviour beyond the three documented failures.
