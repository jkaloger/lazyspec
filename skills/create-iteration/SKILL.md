---
name: create-iteration
description: Use when planning an iteration against a Story or as a standalone iteration for bug fixes, tweaks, and refactors. Creates Iteration documents with task breakdown and test plan. Supports parallel subagent dispatch for Stories with multiple AC groups.
---

```
PLAN THE WORK, THEN CONFIRM BEFORE BUILDING
```

This skill creates the iteration document. It does NOT write code.

<HARD-GATE>
Do NOT write test code or production code in this skill. Plan tests and
tasks only. For feature work linked to a Story, use `/resolve-context` first
if you haven't already. Standalone iterations (bug fixes, tweaks, refactors)
do not require a parent Story or resolve-context.
After identifying multiple AC groups, partition upfront and get user approval
before dispatching subagents.
After completion: present the iteration to the user for review.
Only use the `/build` skill after the user explicitly confirms.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT write test or production code. This skill produces a plan document only.
- Do NOT dispatch subagents without user approval of the AC grouping.
</NEVER>

## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.

Always pass `--json` when the command supports it. This gives you structured,
parseable output. Only omit `--json` when presenting output directly to the user.

If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying. Do not guess at fixes or retry the same
command blindly.

# Create Iteration

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build

create-iteration.style.fill: "#4A9EFF"
create-iteration.style.font-color: "#FFFFFF"
plan.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Context resolved? -> Gather context: no
Context resolved? -> Read Story ACs: yes

Gather context.shape: hexagon

Read Story ACs -> Multiple iteration groups?

Multiple iteration groups?.shape: diamond
Multiple iteration groups? -> Define AC groups -> User approves groups?: yes
Multiple iteration groups? -> Create single iteration (inline): no

User approves groups?.shape: diamond
User approves groups? -> Dispatch N subagents: yes
User approves groups? -> Revise groups: no
Revise groups -> Define AC groups

Dispatch N subagents -> Collect results -> Validate -> Present to user
Create single iteration (inline) -> Link to story -> Plan tests -> Write task breakdown -> Present to user

Present to user -> User confirms -> Use /build skill: approved
Present to user -> Revise: changes requested

Present to user.shape: diamond
Use /build skill.shape: double_circle
```

## Preflight

1. If linked to a Story: run `lazyspec context <story-id> --json` to see the chain, then `lazyspec show <story-id> --json` for the full ACs
2. Run `lazyspec status --json` to see all documents and check no existing iteration covers the same ACs
3. Read relevant documents using `lazyspec show --json` before modifying anything

## AC Grouping

Before dispatching subagents, the orchestrator must:

1. Read the story with `lazyspec show <story-id> --json`
2. List all ACs
3. Group ACs into iteration-sized chunks. Each group should be a coherent unit of work (not arbitrary splits). Consider dependencies between ACs.
4. Verify each AC belongs to exactly one group (no overlap, no gaps)
5. Present the grouping table to the user for approval

The grouping table should clearly show each iteration's title, which ACs it covers, and a brief rationale for why those ACs belong together. The user must approve or request revisions before any subagents are dispatched.

## Subagent Dispatch

| Tier | Model | Use for |
|------|-------|---------|
| Light | Haiku | Parsing frontmatter, extracting structured data, simple validation |
| Medium | Sonnet | Codebase exploration, searching for patterns, reading and summarizing documents |
| Heavy | Opus | Implementation, complex reasoning, multi-file changes, review |

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Create iteration | general-purpose | Heavy | Story context, AC group, other group boundaries, RFC intent |
| Discover relevant code | Explore | Medium | File paths and symbols from Story ACs |
| Validate file paths exist | Explore | Light | List of paths referenced in task breakdown |

Each subagent receives a prompt containing:
- The full Story body (not a file reference)
- The RFC design intent (1-2 paragraphs)
- Its specific AC group (which ACs to cover)
- The AC assignments of all other groups (so it knows what to exclude)
- Instructions to: create the iteration with `lazyspec create iteration`, link to story with `lazyspec link`, plan tests for its ACs, write task breakdown with real file paths
- The standard lazyspec CLI reference block
- Instructions to use Explore agents for code discovery before writing the task breakdown

Subagents are dispatched in parallel using the Agent tool.

### Subagent Prompt Template

```
IMPORTANT: You are working within the lazyspec workflow.
- Use `lazyspec` CLI commands for document operations. Do NOT write document files directly.
- Read files before editing them. Use the Read tool or `lazyspec show --json` before any modification.
- Implement ONLY what the task specifies. Do not add features, refactor surrounding code, or "improve" things not in the task.

You are creating a single Iteration document within the lazyspec workflow.

## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.

Always pass `--json` when the command supports it. This gives you structured,
parseable output. Only omit `--json` when presenting output directly to the user.

If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying. Do not guess at fixes or retry the same
command blindly.

## Story Context
[Full Story body]

## RFC Design Intent
[1-2 paragraphs from the RFC]

## Your AC Group
[List of ACs this iteration covers]

## Other AC Groups (for boundary awareness)
[List of other groups and their AC assignments]

## Instructions
1. Create the iteration: `lazyspec create iteration "<title>" --author agent`
2. Link to story: `lazyspec link <iteration-path> implements <story-path>`
3. Discover relevant code using `lazyspec search` and Explore subagents
4. Plan tests for each AC in your group
5. Write task breakdown with real, verified file paths
6. Validate: `lazyspec validate --json`
7. Report: iteration path, ACs covered, task count, any concerns

IMPORTANT: Use `lazyspec` CLI commands for document operations. Do NOT write document files directly.
Read files before editing them. Use the Read tool or `lazyspec show --json` before any modification.
```

## Steps

### Multi-iteration stories

1. **Gather context:** Run `lazyspec status --json` to see all documents at once, then `lazyspec search "<keyword>" --json` for topic-specific matches.
   - Run `lazyspec context <story-id> --json` to see the chain, then `lazyspec show <story-id> --json` to read the full ACs. If you haven't already resolved context, use `/resolve-context` first.

2. **Read Story ACs:** Extract all ACs from the story. Identify natural groupings based on coherence and dependencies.

3. **Group ACs into iteration-sized chunks:** Per the AC Grouping section. Each group should be a self-contained unit of deliverable work.

4. **Present grouping to user for approval:** Show the grouping table. Wait for explicit approval. If the user requests changes, revise and re-present.

5. **Dispatch N subagents in parallel:** One subagent per iteration, using the Agent tool with the prompt template above. Each receives the full Story body, RFC design intent, its AC group, and the boundaries of all other groups.

6. **Collect results:** Gather reports from all subagents. Run `lazyspec validate --json` to verify all iterations link correctly and pass validation.

7. **Present all created iterations to the user:** Show a summary of each iteration created, its ACs, task breakdown, and the validation result.

### Single-iteration stories (fallback) and standalone iterations

When all ACs fit in a single iteration, or for standalone iterations (bug fixes, tweaks, refactors), create the iteration directly without subagent dispatch:

1. **Gather context:** Run `lazyspec status --json` to see all documents at once, then `lazyspec search "<keyword>" --json` for topic-specific matches.
   - **If linked to a Story:** Run `lazyspec context <story-id> --json` to see the chain, then `lazyspec show <story-id> --json` to read the full ACs. If you haven't already resolved context, use `/resolve-context` first.
   - **If standalone (bug fix, tweak, refactor):** Gather context from the codebase directly. Understand the affected code and the problem being solved. No Story or resolve-context required.

2. **Discover relevant code:** Use `lazyspec search --json` to find specs that reference the modules and types you'll be working with. Read the referenced file paths from those specs to understand the existing code before planning tasks. Task breakdowns must reference real, verified file paths.

3. **Create the iteration:** Run `lazyspec help create` to confirm usage, then: `lazyspec create iteration "<title>" --author agent`

4. **Link to story (if applicable):** If this iteration implements a Story, run `lazyspec help link` to confirm usage, then: `lazyspec link <iteration-path> implements <story-path>`. Standalone iterations for bug fixes, tweaks, or refactors do not require a parent Story link.

5. **Plan tests:** For each AC this iteration covers, describe the test that will verify it. Document these in the iteration's `## Test Plan` section. Do NOT write test code or production code yet -- that happens during build.

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

   Apply standard design principles to test code: don't repeat yourself, don't
   build what you don't need yet. Shared fixtures and helpers should be planned
   when real duplication is visible, not preemptively.

6. **Write task breakdown:** The `## Changes` section must contain a numbered task list. Each task must be self-contained enough for a zero-context subagent to implement independently:

   ```markdown
   ### Task 1: [descriptive name]

   **ACs addressed:** AC-1, AC-3

   **Files:**
   - Create/Modify: `exact/path/to/file`
   - Test: `tests/exact/path/to/test`

   **What to implement:**
   [Complete description -- not "add validation" but the actual logic]

   **How to verify:**
   [Test commands and expected output]
   ```

   Each task should reference which Story ACs it addresses, include exact file paths, describe the implementation in enough detail that someone unfamiliar with the codebase can execute it, and specify how to verify correctness.

8. **Document:** Add any discoveries or decisions to `## Notes`. If a significant decision was made, run `lazyspec help create` to confirm usage, then: `lazyspec create adr "<decision>"`.

9. **Validate:** Run `lazyspec validate --json`.

10. **Present to user:** Show the user the complete iteration document (task breakdown, test plan, linked ACs). Ask for explicit confirmation before proceeding. Do NOT use `/build` until the user approves.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "Let me just start coding" | This skill plans. Build writes code. |
| "I'll write the tests now" | Plan the tests here, write them during build. |
| "I'll use /build right after" | Stop. Present to the user. Wait for confirmation. |
| "The user will probably approve" | Probably isn't confirmed. Ask. |
| "I'll create all iterations myself" | Subagents prevent context pollution. One agent per iteration. |
| "I don't need user approval for the grouping" | Always get approval. AC grouping affects iteration scope. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes
- [ ] If linked to a Story: all iterations link to Story correctly
- [ ] Each AC belongs to exactly one iteration (no overlap)
- [ ] Each iteration has a task breakdown with file paths in `## Changes`
- [ ] Each task references Story ACs
- [ ] `## Test Plan` section documents planned tests
- [ ] Iterations have been presented to the user
- [ ] User has explicitly confirmed before `/build` is used
- [ ] No test code or production code has been written

## Rules

- This skill produces a document, not code
- One iteration should cover a subset of Story ACs, not all of them (unless they all fit in one)
- If you discover a contract needs to change, emit an ADR
- Keep iterations small and committable
- Always present the iteration to the user and wait for confirmation before invoking build
- For multi-iteration stories, dispatch one subagent per iteration
- Always get user approval of the AC grouping before dispatching
- Each subagent receives full Story text, not file references
- Each AC must belong to exactly one iteration

## Guardrails

Before presenting the iteration to the user, verify:

- [ ] Is this a feature? Then confirm a parent Story exists and is linked.
- [ ] Have you read the Story ACs? (not assumed -- actually read with `lazyspec show --json`)
- [ ] Are the AC groups non-overlapping with no gaps?
- [ ] Has the user approved the grouping?
- [ ] Is each subagent receiving full Story text (not a file reference)?
- [ ] Does every task reference real, verified file paths? (not guessed)
- [ ] Is the task breakdown detailed enough for a zero-context subagent?

If any answer is "no", stop. Complete the missing step.
