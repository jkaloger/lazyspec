---
title: Subagent Orchestration in create-story
type: iteration
status: draft
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-041-subagent-orchestration-in-create-story.md
---


## Changes

### Task 1: Rewrite create-story skill with subagent orchestration

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Modify: `skills/create-story/SKILL.md`

**What to implement:**

Rewrite the `create-story` SKILL.md to add subagent orchestration for multi-story creation. The existing single-story workflow becomes the subagent's job. The orchestrator handles partitioning and dispatch.

The updated skill should have this structure:

**Frontmatter:** Update description to mention multi-story orchestration.

**Hard gate:** Keep existing gate (no Story without RFC). Add: after identifying multiple slices, partition upfront and get user approval before dispatching subagents.

**Workflow d2 diagram:** Replace the current linear flow with:
```
Find parent RFC -> RFC exists?

RFC exists?.shape: diamond
RFC exists? -> Read RFC and extract slices: yes
RFC exists? -> Use /write-rfc skill: no

Use /write-rfc skill.shape: hexagon

Read RFC and extract slices -> Multiple slices?

Multiple slices?.shape: diamond
Multiple slices? -> Define partitions -> User approves partitions? -> Dispatch N subagents: yes
Multiple slices? -> Create single story (inline): no

User approves partitions?.shape: diamond
User approves partitions? -> Revise partitions: no
Revise partitions -> Define partitions

Dispatch N subagents -> Collect results -> Validate -> Present to user
Create single story (inline) -> Write ACs -> Link to RFC -> Validate -> Use /create-iteration skill

Use /create-iteration skill.shape: double_circle
```

**New section -- Partitioning:**

Before dispatching subagents, the orchestrator must:
1. Read the RFC with `lazyspec show <rfc-id> --json`
2. Extract the identified vertical slices from the Stories section
3. For each slice, define: title, scope boundary (in/out), which RFC sections it addresses
4. Verify slices are non-overlapping (no shared scope)
5. Present the partition table to the user for approval

**New section -- Subagent Dispatch:**

Add a subagent dispatch table following `build`'s conventions:

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Create story | general-purpose | Heavy | RFC context, slice definition, adjacent slice boundaries |

Each subagent receives a prompt containing:
- The full RFC body (not a file reference)
- Its specific slice definition (title, in-scope, out-of-scope)
- The scope boundaries of all other slices (so it knows what to exclude)
- Instructions to: create the story with `lazyspec create story`, write given/when/then ACs, link to RFC with `lazyspec link`, define scope sections
- The lazyspec CLI reference block (same as other skills)

Subagents are dispatched in parallel using the Agent tool.

**Single-slice fallback (AC-6):** When the RFC identifies only one slice, skip partitioning and subagent dispatch. Create the story inline using the existing workflow (this preserves the current behavior for simple cases).

**Collect and validate:** After all subagents complete, run `lazyspec validate --json` and present all created stories to the user.

**Keep existing sections:** Preserve the Red Flags, Verification, and Rules sections. Update Verification to include:
- All created stories link to the parent RFC
- No overlapping scope between stories
- Each story has given/when/then ACs

**How to verify:**
- `skills/create-story/SKILL.md` contains partitioning workflow, subagent dispatch table, and single-slice fallback
- Skill follows same conventions as `skills/build/SKILL.md` for subagent dispatch
- Existing single-story workflow is preserved as the fallback path

## Test Plan

Manual verification (skill files are markdown):

- **AC-1/2:** Invoke `/create-story` on an RFC with 3+ identified slices. Confirm the skill extracts slices and presents a partition table for approval.
- **AC-3:** Confirm each subagent prompt includes RFC context, its slice definition, and adjacent slice boundaries.
- **AC-4:** After parallel dispatch, verify each story has non-overlapping scope and links to the RFC.
- **AC-5:** Run `lazyspec validate --json` after creation and confirm no errors.
- **AC-6:** Invoke `/create-story` on an RFC with one slice. Confirm it creates the story inline without subagent dispatch.

## Notes

The `build` skill (`skills/build/SKILL.md`) is the reference implementation for subagent orchestration. Key patterns to mirror:
- Subagents receive full text, not file references
- Subagent dispatch table with tier classification
- Per-subagent prompt template in the skill
- Sequential dispatch is used by `build` (one task at a time) but here we use parallel dispatch since stories are independent and don't conflict
