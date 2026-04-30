---
title: "Skill prompt updates for slicing partition and iteration sweep modes"
type: iteration
status: draft
author: "jkaloger"
date: 2026-04-30
tags: []
related: []
---

## Context

Grilling session resolved planning conventions:
- Story = user-facing slice (G/W/T ACs).
- Story:Iteration is M:N. Iteration can `implements` multiple Stories.
- Stub/interface/infra Iterations attach to Stories via `implements`. NOT standalone.
- Standalone Iteration reserved for lightweight pipeline only (tiny tweaks, bug fixes, refactors).
- Slice categories: route stub / interface stub / data integration / UI presentation / functional / polish / cleanup.
- Priority + DAG via RFC-041 (in flight).

Three skill prompts encode workflow. Need updates so agents apply conventions.

## Test Plan

Skill prompts = markdown. No code tests. Manual verification:

1. Read each skill post-edit. Confirm:
   - `/create-story`: partition mode generates 2-3 candidate partitions, user-facing only, references slice categories, says dev-only work → Iteration not Story.
   - `/create-iteration`: sweep mode triggered by RFC + Stories input, generates M:N `implements` + `blocks` edges, walks slice categories.
   - `/plan-work`: new entry "RFC + Stories exist, no Iterations" → sweep.
2. Dry-run `/plan-work` against this branch (RFC-041 + child Stories): confirm new entry point fires.
3. Spot-check skill cross-refs: `/plan-work` references sweep mode; `/create-iteration` sweep references slice taxonomy.

## Changes

### Task 1: create-story sweep refinements

File: `skills/create-story/SKILL.md`

Edit `## Multi-slice RFCs` section + `## Rules`:
- Generate 2-3 candidate partitions (not one). User compares, picks, remixes. Anchoring mitigation.
- Emphasize: Stories are user-facing slices only. G/W/T required.
- Add slice category guidance: each Story tagged route-stub / data-integration / ui-presentation / functional / polish.
- Add explicit note: dev-only work (interface stubs, infra, cleanup) goes to Iterations w/ `implements` to multiple Stories. Do NOT create Stories for dev-only work.
- Reference RFC-041 priority field (must/should/could) per Story.

Verify: skill output produces only user-facing Stories. Dev-only work deferred to /create-iteration sweep.

### Task 2: create-iteration sweep mode

File: `skills/create-iteration/SKILL.md`

Add `## Sweep Mode (RFC + multiple Stories)` section after existing AC Grouping:
- Trigger: given RFC w/ 2+ child Stories and no Iterations.
- Walk all child Stories. Identify:
  - Per-Story Iterations (1+ per Story)
  - Shared-contract Iterations (interface stubs, types, schemas) → `implements` to multiple Stories
  - Cross-cutting infra Iterations (data pipeline, migrations) → `implements` to consumer Stories
  - Polish/cleanup Iterations → `implements` relevant Stories
- Wire `blocks` edges (RFC-041): contract Iter blocks consumer Iter; infra Iter blocks data-using Iter.
- Output: 2-3 candidate Iteration plans varying contract granularity (one fat interfaces Iter vs several small).
- Present to user. Wait for approval. Then dispatch subagents per Iter (parallel).

Update `## Workflow` d2 diagram: add sweep mode branch.

Update `<HARD-GATE>`: standalone Iterations reserved for lightweight pipeline only. Substantial dev work `implements` Stories.

Caveman ultra format for the new sweep mode prose.

### Task 3: plan-work entry point

File: `skills/plan-work/SKILL.md`

Edit `## Entry Points` table (New features):
- Add row: "RFC exists, Stories exist, no Iterations → Use `/create-iteration` sweep mode"
- Existing "Story exists, no Iteration" row stays (single-Story path)

Edit `## Workflow` d2:
- Add branch: "RFC + Stories, no Iterations → Use /create-iteration sweep mode"

Edit `## Brainstorming`:
- Update "Iteration level" subsection: distinguish single-Story (use /resolve-context) vs RFC sweep (use /create-iteration sweep mode).

### Task 4: cross-skill consistency check

Read all three updated skills. Verify:
- Slice category vocabulary matches across skills.
- M:N Story:Iteration model stated consistently.
- Standalone-Iteration reservation stated identically in /plan-work and /create-iteration.
- RFC-041 reference present where priority/blocks DAG mentioned.

No file edit unless inconsistency found. Adjust as needed.

## Notes

- Engine-clean: no doc type changes, no relationship type changes, no engine code touched. Skill markdown only.
- RFC-041 is the dependency: priority field + `blocks` DAG must land for sweep mode's edge-wiring to validate. If RFC-041 unstable, sweep mode falls back to no `blocks` edges and humans wire later.
- Existing `/create-story` partition flow already multi-slice; this refines it rather than rewriting.
- Existing `/create-iteration` AC-grouping already dispatches subagents per group; sweep mode is the RFC-level analog.

