---
title: Subagent Orchestration in create-iteration
type: iteration
status: draft
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-042-subagent-orchestration-in-create-iteration.md
---


## Changes

### Task 1: Rewrite create-iteration skill with subagent orchestration

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Modify: `skills/create-iteration/SKILL.md`

**What to implement:**

Rewrite the `create-iteration` SKILL.md to add subagent orchestration for multi-iteration creation. The existing single-iteration workflow becomes the subagent's job. The orchestrator handles AC grouping and dispatch.

The updated skill should have this structure:

**Frontmatter:** Update description to mention multi-iteration orchestration.

**Hard gate:** Keep existing gates (no code in this skill, resolve-context first for features). Add: after identifying multiple AC groups, partition upfront and get user approval before dispatching subagents.

**Workflow d2 diagram:** Replace the current flow with:
```
Context resolved? -> Gather context: no
Context resolved? -> Read Story ACs: yes

Gather context.shape: hexagon

Read Story ACs -> Multiple iteration groups?

Multiple iteration groups?.shape: diamond
Multiple iteration groups? -> Define AC groups -> User approves groups? -> Dispatch N subagents: yes
Multiple iteration groups? -> Create single iteration (inline): no

User approves groups?.shape: diamond
User approves groups? -> Revise groups: no
Revise groups -> Define AC groups

Dispatch N subagents -> Collect results -> Validate -> Present to user
Create single iteration (inline) -> Link to story -> Plan tests -> Write task breakdown -> Present to user

Present to user.shape: diamond
Present to user -> User confirms -> Use /build skill: approved
Present to user -> Revise: changes requested

Use /build skill.shape: double_circle
```

**New section -- AC Grouping:**

Before dispatching subagents, the orchestrator must:
1. Read the story with `lazyspec show <story-id> --json`
2. List all ACs
3. Group ACs into iteration-sized chunks. Each group should be a coherent unit of work (not arbitrary splits).
4. Verify each AC belongs to exactly one group (no overlap, no gaps)
5. Present the grouping table to the user for approval

**New section -- Subagent Dispatch:**

Add a subagent dispatch table:

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Create iteration | general-purpose | Heavy | Story context, AC group, other group boundaries, RFC intent |

Each subagent receives a prompt containing:
- The full Story body (not a file reference)
- The RFC design intent (1-2 paragraphs)
- Its specific AC group (which ACs to cover)
- The AC assignments of all other groups (so it knows what to exclude)
- Instructions to: create the iteration with `lazyspec create iteration`, link to story with `lazyspec link`, plan tests for its ACs, write task breakdown with file paths
- The lazyspec CLI reference block
- Instructions to discover relevant code with `lazyspec search` and Explore agents before writing the task breakdown

Subagents are dispatched in parallel using the Agent tool.

**Single-iteration fallback (AC-6):** When all ACs fit in a single iteration, skip grouping and subagent dispatch. Create the iteration inline using the existing workflow.

**Collect and validate:** After all subagents complete, run `lazyspec validate --json` and present all created iterations to the user. Each iteration must be individually reviewable.

**Keep existing sections:** Preserve the Red Flags, Verification, Rules, and Guardrails sections. Update Verification to include:
- Each AC belongs to exactly one iteration
- All iterations link to the parent story
- Each iteration has a task breakdown with file paths

**How to verify:**
- `skills/create-iteration/SKILL.md` contains AC grouping workflow, subagent dispatch table, and single-iteration fallback
- Skill follows same conventions as `skills/build/SKILL.md` for subagent dispatch
- Existing single-iteration workflow is preserved as the fallback path

## Test Plan

Manual verification (skill files are markdown):

- **AC-1/2:** Invoke `/create-iteration` on a story with 6+ ACs that split naturally into 2-3 groups. Confirm the skill groups ACs and presents the grouping for approval.
- **AC-3:** Confirm each subagent prompt includes story context, its AC group, and other group boundaries.
- **AC-4:** After parallel dispatch, verify each AC appears in exactly one iteration and each iteration links to the story.
- **AC-5:** Run `lazyspec validate --json` after creation and confirm no errors.
- **AC-6:** Invoke `/create-iteration` on a story with 2-3 ACs. Confirm it creates one iteration inline without subagent dispatch.

## Notes

Key difference from ITERATION-033 (create-story orchestration): iteration subagents need to do more work per document. Each iteration needs a task breakdown with real file paths, which means the subagent needs codebase access (Explore agents). Story subagents only need to write ACs from the RFC, which is lighter.

The existing `create-iteration` skill already has a subagent dispatch table for code discovery. The new orchestration adds a layer on top: the orchestrator dispatches iteration-creation subagents, and those subagents may in turn use Explore agents for code discovery.
