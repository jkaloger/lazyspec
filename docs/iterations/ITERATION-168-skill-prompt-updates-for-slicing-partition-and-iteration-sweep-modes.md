---
title: Skill prompt updates and project-plan scaffolding
type: iteration
status: accepted
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
- Project-level orchestration distinct from feature-level routing.

Four skill prompts touched. Three existing (refinements). One new (`/project-plan`).

Workflow stack post-iteration:
- `/project-plan` (new) — project-level orchestrator. SOW → set of RFCs.
- `/plan-work` (refined) — feature-level router. Routes to /write-rfc, /create-story, /create-iteration.
- `/create-story` (refined) — partition mode produces user-facing Stories only.
- `/create-iteration` (refined) — sweep mode generates M:N Iteration plan across an RFC's Stories.

## Test Plan

Skill prompts = markdown. No code tests. Manual verification:

1. Read each skill post-edit. Confirm:
   - `/project-plan`: SOW input → capability list w/ MoSCoW priority → parallel /write-rfc dispatches → cross-RFC blocks edges. Out-of-scope sections explicit (no estimation, no discovery).
   - `/create-story`: partition mode generates 2-3 candidate partitions, user-facing only, references slice categories, says dev-only work → Iteration not Story.
   - `/create-iteration`: sweep mode triggered by RFC + Stories input, generates M:N `implements` + `blocks` edges, walks slice categories.
   - `/plan-work`: new entry "RFC + Stories exist, no Iterations" → sweep. References /project-plan as upstream.
2. Dry-run `/plan-work` against this branch (RFC-041 + child Stories): confirm new entry point fires.
3. Dry-run `/project-plan` against TAC020-shaped SOW prose: confirm capability extraction + dispatch shape.
4. Spot-check skill cross-refs: /project-plan → /write-rfc → /plan-work → /create-story / /create-iteration. Vocabulary consistent across all four.

## Changes

### Task 1: create-story partition refinements

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
- Add upstream reference: project-level work routes via /project-plan first.

Edit `## Brainstorming`:
- Update "Iteration level" subsection: distinguish single-Story (use /resolve-context) vs RFC sweep (use /create-iteration sweep mode).
- Add "Project level": defer to /project-plan if SOW/multi-RFC scope.

### Task 4: scaffold /project-plan skill

File: `skills/project-plan/SKILL.md` (new)

Frontmatter: `name: project-plan`, description = orchestrate project-level planning above /plan-work.

Sections:
- HARD-GATE: don't dispatch /write-rfc subagents without user approval of capability list + priority distribution.
- NEVER: don't write document files directly; don't estimate (out of scope, PM tool concern); don't capture discovery notes (out of scope).
- Workflow d2: SOW input → capability extraction → priority distribution (must/should/could) → user approval → parallel /write-rfc dispatch → cross-RFC blocks edges → project summary.
- Preflight: lazyspec status --json, search existing RFCs to avoid duplication.
- Steps:
  1. Capture project name + SOW/brief input.
  2. Extract capabilities (one per major user-facing area). Each = future RFC title.
  3. Distribute MoSCoW priority across capabilities. must = NEED (contractual), should/could = WANT (budget headroom).
  4. Identify cross-RFC dependencies (will become blocks edges).
  5. Present capability list + priorities + dependencies to user. Wait for approval.
  6. Dispatch one /write-rfc subagent per capability in parallel. Each gets: capability scope, priority, sibling capability boundaries, project context.
  7. Collect RFCs. Wire cross-RFC blocks edges via lazyspec link.
  8. Emit project summary: must/should/could counts, suggested sequencing, total RFC count.
- Out of scope (explicit): estimation, discovery, Story partition (delegated to /create-story), Iteration sweep (delegated to /create-iteration).
- Rules: project skill produces RFCs only. Stories + Iterations come later via downstream skills.

Caveman lite for skill prose.

### Task 5: cross-skill consistency check

Read all four skills (3 refined + 1 new). Verify:
- Slice category vocabulary matches across skills.
- M:N Story:Iteration model stated consistently.
- Standalone-Iteration reservation stated identically in /plan-work and /create-iteration.
- RFC-041 reference present where priority/blocks DAG mentioned.
- /project-plan and /plan-work cross-reference each other (project-plan as upstream of plan-work).
- Out-of-scope statements (estimation, discovery) consistent in /project-plan and convention.

No file edit unless inconsistency found. Adjust as needed.

## Notes

- Engine-clean: no doc type changes, no relationship type changes, no engine code touched. Skill markdown only.
- RFC-041 is the dependency: priority field + `blocks` DAG must land for sweep mode + project-plan edge-wiring to validate. If RFC-041 unstable, both modes fall back to no `blocks` edges and humans wire later.
- Existing `/create-story` partition flow already multi-slice; this refines it rather than rewriting.
- Existing `/create-iteration` AC-grouping already dispatches subagents per group; sweep mode is the RFC-level analog. /project-plan is the project-level analog.
- /project-plan added under Q2 path A (template-only extension model) — no engine changes, just a new skill markdown file.
- Three concrete uses of "decompose into parallel units + dispatch subagents" pattern earned the abstraction: /create-story partition, /create-iteration sweep, /project-plan capability extraction. Pattern boundary: human approval gate before dispatch.
