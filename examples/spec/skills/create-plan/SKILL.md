---
name: create-plan
description: Use when planning implementation against a Spec or as a standalone plan for bug fixes, tweaks, and refactors. Creates Plan documents with task breakdown and test plan. Supports parallel subagent dispatch for Specs with multiple contract groups.
---

```
PLAN THE WORK, THEN CONFIRM BEFORE BUILDING
```

This skill creates the plan document. It does NOT write code.

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<HARD-GATE>
Do NOT write test code or production code in this skill. Plan tests and
tasks only. For feature work linked to a Spec, use `/resolve-context` first
if you haven't already. Standalone plans (bug fixes, tweaks, refactors)
do not require a parent Spec or resolve-context.
After identifying multiple contract groups, partition upfront and get user approval
before dispatching subagents.
After completion: present the plan to the user for review.
Only use the `/build` skill after the user explicitly confirms.
</HARD-GATE>

<NEVER>
- Do NOT write test or production code. This skill produces a plan document only.
- Do NOT dispatch subagents without user approval of the contract grouping.
</NEVER>

# Create Plan

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build

create-plan.style.fill: "#4A9EFF"
create-plan.style.font-color: "#FFFFFF"
lazy.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-spec.style.opacity: 0.4
resolve-context.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Context resolved? -> Gather context: no
Context resolved? -> Read Spec contracts: yes

Gather context.shape: hexagon

Read Spec contracts -> Multiple plan groups?

Multiple plan groups?.shape: diamond
Multiple plan groups? -> Define contract groups -> User approves groups?: yes
Multiple plan groups? -> Create single plan (inline): no

User approves groups?.shape: diamond
User approves groups? -> Dispatch N subagents: yes
User approves groups? -> Revise groups: no
Revise groups -> Define contract groups

Dispatch N subagents -> Collect results -> Validate -> Present to user
Create single plan (inline) -> Link to spec -> Plan tests -> Write task breakdown -> Present to user

Present to user -> User confirms -> Use /build skill: approved
Present to user -> Revise: changes requested

Present to user.shape: diamond
Use /build skill.shape: double_circle
```

## Preflight

1. If linked to a Spec: run `lazyspec context <spec-id> --json` to see the chain, then `lazyspec show <spec-id> --json` for the full contracts
2. Run `lazyspec status --json` to see all documents and check no existing plan covers the same contracts
3. Read relevant documents using `lazyspec show --json` before modifying anything

## Contract Grouping

Before dispatching subagents, the orchestrator must:

1. Read the spec with `lazyspec show <spec-id> --json`
2. List all contract sections
3. Group contracts into plan-sized chunks. Each group should be a coherent unit of work (not arbitrary splits). Consider dependencies between contracts.
4. Verify each contract belongs to exactly one group (no overlap, no gaps)
5. Present the grouping table to the user for approval

The grouping table should clearly show each plan's title, which contracts it covers, and a brief rationale for why those contracts belong together. The user must approve or request revisions before any subagents are dispatched.

## Subagent Dispatch

| Operation                   | Agent Type      | Tier   | Context to provide                                                   |
| --------------------------- | --------------- | ------ | -------------------------------------------------------------------- |
| Create plan                 | general-purpose | Heavy  | Spec context, contract group, other group boundaries, RFC intent     |
| Discover relevant code      | Explore         | Medium | File paths and symbols from Spec contracts                           |
| Validate file paths exist   | Explore         | Light  | List of paths referenced in task breakdown                           |

Each subagent receives: full Spec body (not a file reference), RFC design intent (if exists), its contract group, and the boundaries of all other groups. Read the prompt template from `prompts/planner.md` (relative to this skill directory).

Subagents are dispatched in parallel using the Agent tool.

## Steps

### Multi-plan specs

1. **Gather context:** Run `lazyspec status --json` to see all documents at once, then `lazyspec search "<keyword>" --json` for topic-specific matches.
   - Run `lazyspec context <spec-id> --json` to see the chain, then `lazyspec show <spec-id> --json` to read the full contracts. If you haven't already resolved context, use `/resolve-context` first.

2. **Read Spec contracts:** Extract all contract sections from the spec. Identify natural groupings based on coherence and dependencies.

3. **Group contracts into plan-sized chunks:** Per the Contract Grouping section. Each group should be a self-contained unit of deliverable work.

4. **Present grouping to user for approval:** Show the grouping table. Wait for explicit approval. If the user requests changes, revise and re-present.

5. **Dispatch N subagents in parallel:** One subagent per plan, using the Agent tool with the prompt template. Each receives the full Spec body, RFC design intent, its contract group, and the boundaries of all other groups.

6. **Collect results:** Gather reports from all subagents. Run `lazyspec validate --json` to verify all plans link correctly and pass validation.

7. **Present all created plans to the user:** Show a summary of each plan created, its contracts, task breakdown, and the validation result.

### Single-plan specs (fallback) and standalone plans

When all contracts fit in a single plan, or for standalone plans (bug fixes, tweaks, refactors), create the plan directly without subagent dispatch:

1. **Gather context:** Run `lazyspec status --json` to see all documents at once, then `lazyspec search "<keyword>" --json` for topic-specific matches.
   - **If linked to a Spec:** Run `lazyspec context <spec-id> --json` to see the chain, then `lazyspec show <spec-id> --json` to read the full contracts. If you haven't already resolved context, use `/resolve-context` first.
   - **If standalone (bug fix, tweak, refactor):** Gather context from the codebase directly. Understand the affected code and the problem being solved. No Spec or resolve-context required.

2. **Discover relevant code:** Use `lazyspec search --json` to find documents that reference the modules and types you'll be working with. Read the referenced file paths from those documents to understand the existing code before planning tasks. Task breakdowns must reference real, verified file paths.

3. **Create the plan:** Run `lazyspec help create` to confirm usage, then: `lazyspec create plan "<title>" --author agent`

4. **Link to spec (if applicable):** If this plan implements a Spec, run `lazyspec help link` to confirm usage, then: `lazyspec link <plan-path> implements <spec-path>`. Standalone plans for bug fixes, tweaks, or refactors do not require a parent Spec link.

5. **Plan tests:** For each contract this plan covers, describe the test that will verify it. Document these in the plan's `## Test Plan` section. Do NOT write test code or production code yet -- that happens during build.

   Each planned test should be evaluated against these properties:

   | Property              | Meaning                                                   |
   | --------------------- | --------------------------------------------------------- |
   | Isolated              | Same results regardless of execution order                |
   | Composable            | Run 1 or 1,000,000 and get the same results               |
   | Fast                  | Cheap to run                                              |
   | Inspiring             | Passing builds confidence in production readiness         |
   | Writable              | Cheap to write relative to the code under test            |
   | Readable              | Motivation for the test is obvious to the reader          |
   | Behavioral            | Sensitive to changes in behavior, not implementation      |
   | Structure-insensitive | Result unchanged by structural refactoring                |
   | Deterministic         | Same result when nothing changes                          |
   | Predictive            | All passing implies production-suitable                   |
   | Specific              | Failure cause is obvious                                  |

   These properties conflict. When planning a test that trades one for another
   (e.g. an integration test that sacrifices Fast for Predictive), note the
   tradeoff in the test plan and present it to the collaborator for guidance.

6. **Write task breakdown:** The `## Changes` section must contain a numbered task list. Each task must be self-contained enough for a zero-context subagent to implement independently:

   ```markdown
   ### Task 1: [descriptive name]

   **Contracts addressed:** [which spec contracts this implements]

   **Files:**
   - Create/Modify: `exact/path/to/file`
   - Test: `tests/exact/path/to/test`

   **What to implement:**
   [Complete description -- not "add validation" but the actual logic]

   **How to verify:**
   [Test commands and expected output]
   ```

   Each task should reference which Spec contracts it addresses, include exact file paths, describe the implementation in enough detail that someone unfamiliar with the codebase can execute it, and specify how to verify correctness.

7. **Document:** Add any discoveries or decisions to `## Notes`. If a significant decision was made, run `lazyspec help create` to confirm usage, then: `lazyspec create adr "<decision>"`.

8. **Validate:** Run `lazyspec validate --json`.

9. **Present to user:** Show the user the complete plan document (task breakdown, test plan, linked contracts). Ask for explicit confirmation before proceeding. Do NOT use `/build` until the user approves.

## Authoring Principles

1. **One plan per deliverable slice** -- a plan covers a coherent vertical slice implementable and verifiable independently. If a spec has 6 contracts spanning layout, search, and CMS, that's 2-3 plans.
2. **Tasks are ordered and verifiable** -- each task lists files, what to implement, how to verify. Sequential execution without backtracking.
3. **Test plans map to ACs** -- every AC should have at least one test. The mapping is explicit (e.g. "AC1, AC3"). If an AC can't be tested automatically, say so.
4. **Implementation detail belongs here** -- file paths, code snippets, component structure, CSS classes, import paths. This is plan territory.
5. **Notes capture discoveries** -- don't scatter rationale inline. Collect in the Notes section.
6. **Under 100 lines** -- two focused 60-line plans are better than one sprawling 150-line plan.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "Let me just start coding" | This skill plans. Build writes code. |
| "I'll write the tests now" | Plan the tests here, write them during build. |
| "I'll use /build right after" | Stop. Present to the user. Wait for confirmation. |
| "The user will probably approve" | Probably isn't confirmed. Ask. |
| "This plan is 150 lines but it covers everything" | Split by vertical slice. Two focused plans > one sprawling plan. |

## Checklist

Before presenting the plan to the user:

- [ ] `lazyspec validate --json` passes
- [ ] If linked to a Spec: all plans link to Spec correctly
- [ ] Have you read the Spec contracts? (not assumed -- actually read with `lazyspec show --json`)
- [ ] Each contract belongs to exactly one plan (no overlap)
- [ ] Each plan has a task breakdown with file paths in `## Changes`
- [ ] Each task references Spec contracts
- [ ] `## Test Plan` section documents planned tests
- [ ] Does every task reference real, verified file paths? (not guessed)
- [ ] Is the task breakdown detailed enough for a zero-context subagent?
- [ ] No test code or production code has been written
- [ ] User has explicitly confirmed before `/build` is used

## Rules

- This skill produces a document, not code
- Keep plans small and committable
- Always present the plan to the user and wait for confirmation before invoking build
- If you discover a contract needs to change, emit an ADR
- For multi-plan specs, always get user approval of the grouping before dispatching
- Plans under 100 lines. If longer, the slice is too big -- split it.
