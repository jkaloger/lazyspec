---
title: "Apply document authoring principles to spec example"
type: iteration
status: accepted
author: "agent"
date: 2026-03-11
tags: []
related: []
---


## Changes

### Task 1: Restructure spec template to front-load ACs and scope

**Files:**
- Modify: `examples/spec/.lazyspec/templates/spec.md`

**What to implement:**

Reorder the spec template sections so ACs and Scope appear early, not buried at the bottom. The new order:

1. Summary (unchanged)
2. Scope (In Scope / Out of Scope) -- moved up from bottom
3. Acceptance Criteria -- uncommented, promoted to first-class section
4. Data Models
5. API Surface
6. Validation Rules
7. Error Handling
8. Edge Cases

Add a HTML comment at the top noting the 100-line target: `<!-- Target: under 100 lines. If longer, extract implementation detail into a plan. -->`

Update section guidance text:
- Scope: "Unambiguous boundaries. No hedging (may include, if time permits)."
- ACs: "The core of this spec. Express requirements as given/when/then. Everything else supports these."
- Data Models: "Show shape, not wiring. Use @draft for new types, @ref for existing. Don't show how components consume them."

**How to verify:**
Read the template. Confirm ACs and Scope are in the top half. Confirm the 100-line comment exists.

### Task 2: Update plan template with principles guidance

**Files:**
- Modify: `examples/spec/.lazyspec/templates/plan.md`

**What to implement:**

Add a HTML comment at the top noting the 100-line target: `<!-- Target: under 100 lines. If longer, split into multiple plans per vertical slice. -->`

Update the Test Plan section guidance: "Each test must map to at least one spec AC. Tag tests with the AC(s) they cover (e.g. AC1, AC3). If an AC can't be tested automatically, describe the manual verification."

Add guidance to the Notes section: "Collect discoveries, surprises, and decision rationale here. Don't scatter notes inline through the tasks."

**How to verify:**
Read the template. Confirm the 100-line comment, updated test plan guidance, and notes guidance.

### Task 3: Add authoring principles to create-spec skill

**Files:**
- Modify: `examples/spec/skills/create-spec/SKILL.md`

**What to implement:**

Add a `## Authoring Principles` section after the Contract Sections section. Include these rules (derived from the principles document):

1. **ACs are the core** -- they appear early in the spec, not after pages of implementation context.
2. **Describe behaviour, not implementation** -- "the page renders a hero header with CMS-driven content" not "create a server component at `apps/foo/page.tsx`". File paths and JSX belong in plans.
3. **No open questions in accepted specs** -- phrases like "evaluate during implementation" are design decisions being deferred. Resolve before the spec leaves draft.
4. **Scope is a sharp boundary** -- if someone reads only the Scope section, they know exactly what this work delivers and doesn't.
5. **Under 100 lines** -- if a spec exceeds this, extract implementation detail into plan(s).
6. **Data models show shape, not wiring** -- `@ref` existing types, `@draft` new types. Don't show how components consume them or where they're imported from.
7. **Error handling and edge cases are first-class** -- these are behavioural requirements, not afterthoughts.

Also add to the Rules section at the bottom:
- "Specs under 100 lines. If longer, implementation detail is leaking in."
- "No open questions in accepted specs."

Add to Red Flags table:
- `"We can decide this during implementation"` | `Design decision being deferred. Resolve it now.`
- `"This spec is 150 lines but it's all necessary"` | `Implementation detail is leaking in. Extract to a plan.`

**How to verify:**
Read the skill. Confirm the Authoring Principles section exists with all 7 rules. Confirm Rules and Red Flags are updated.

### Task 4: Add authoring principles to create-plan skill

**Files:**
- Modify: `examples/spec/skills/create-plan/SKILL.md`

**What to implement:**

Add a `## Authoring Principles` section after the Steps section (before Red Flags). Include:

1. **One plan per deliverable slice** -- a plan covers a coherent vertical slice implementable and verifiable independently. If a spec has 6 contracts spanning layout, search, and CMS, that's 2-3 plans.
2. **Tasks are ordered and verifiable** -- each task lists files, what to implement, how to verify. Sequential execution without backtracking.
3. **Test plans map to ACs** -- every AC should have at least one test. The mapping is explicit (e.g. "AC1, AC3"). If an AC can't be tested automatically, say so.
4. **Implementation detail belongs here** -- file paths, code snippets, component structure, CSS classes, import paths. This is plan territory.
5. **Notes capture discoveries** -- don't scatter rationale inline. Collect in the Notes section.
6. **Under 100 lines** -- two focused 60-line plans are better than one sprawling 150-line plan.

Add to Red Flags table:
- `"This plan is 150 lines but it covers everything"` | `Split by vertical slice. Two focused plans > one sprawling plan.`

Add to Rules:
- "Plans under 100 lines. If longer, the slice is too big -- split it."

**How to verify:**
Read the skill. Confirm the Authoring Principles section exists. Confirm Rules and Red Flags updated.

### Task 5: Add anti-patterns reference to _common.md

**Files:**
- Modify: `examples/spec/skills/_common.md`

**What to implement:**

Add an `## Anti-patterns` section after Status Promotion. This is a quick-reference for all skills:

| Pattern | Problem | Fix |
|---|---|---|
| Code blocks in specs | Couples contract to implementation | Move to plan; spec references types only |
| ACs at the bottom | Buried under implementation detail | ACs go in the top half |
| "Evaluate during implementation" | Unresolved design decision | Decide during spec review |
| One plan for the whole spec | Too much in flight | Split by vertical slice |
| Tests without AC mapping | No traceability | Tag each test with AC(s) it covers |
| Notes scattered inline | Hard to find rationale | Collect in Notes section |
| Spec > 100 lines | Doing the plan's job | Extract implementation detail |

**How to verify:**
Read the file. Confirm Anti-patterns table exists with all 7 rows.

## Test Plan

This is a documentation-only refactor. No automated tests apply. Verification is manual:

- Read each modified file and confirm the changes match the task descriptions
- Run `lazyspec validate --json` to confirm document integrity is preserved
- Confirm no functional changes to the lazyspec tool itself

## Notes

The authoring principles document provided by the user is the source of truth. The principles are being embedded into the skills and templates so they're available at the point of use, rather than requiring a separate reference document.

The spec template reordering (Task 1) is the most impactful change. Moving ACs and Scope to the top half changes the authoring flow -- writers will fill these in first rather than treating them as afterthoughts.
