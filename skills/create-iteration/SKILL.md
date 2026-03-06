---
name: create-iteration
description: Use when planning an iteration against a Story or as a standalone iteration for bug fixes, tweaks, and refactors. Creates an Iteration document with task breakdown and test plan, then presents to user for confirmation before build.
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
After completion: present the iteration to the user for review.
Only use the `/build` skill after the user explicitly confirms.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT write test or production code. This skill produces a plan document only.
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
Context resolved? -> Create iteration doc: yes

Gather context.shape: hexagon

Create iteration doc -> Link to story -> Plan tests for ACs -> Write task breakdown

Write task breakdown -> Update iteration doc -> Validate -> Present to user for review

Present to user for review -> User confirms: approved
Present to user for review -> Revise: changes requested
Revise -> Write task breakdown

User confirms -> Use /build skill

Present to user for review.shape: diamond
Use /build skill.shape: double_circle
```

## Preflight

1. If linked to a Story: run `lazyspec context <story-id> --json` to see the chain, then `lazyspec show <story-id> --json` for the full ACs
2. Run `lazyspec status --json` to see all documents and check no existing iteration covers the same ACs
3. Read relevant documents using `lazyspec show --json` before modifying anything

## Subagent Dispatch

| Tier | Model | Use for |
|------|-------|---------|
| Light | Haiku | Parsing frontmatter, extracting structured data, simple validation |
| Medium | Sonnet | Codebase exploration, searching for patterns, reading and summarizing documents |
| Heavy | Opus | Implementation, complex reasoning, multi-file changes, review |

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Discover relevant code | Explore | Medium | File paths and symbols from Story ACs |
| Validate file paths exist | Explore | Light | List of paths referenced in task breakdown |

## Steps

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

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes
- [ ] If linked to a Story: iteration links to Story correctly
- [ ] `## Changes` section contains a numbered task breakdown
- [ ] Each task includes file paths and describes implementation. If linked to a Story, tasks reference Story ACs.
- [ ] `## Test Plan` section documents planned tests
- [ ] Iteration has been presented to the user
- [ ] User has explicitly confirmed before `/build` is used
- [ ] No test code or production code has been written

## Rules

- This skill produces a document, not code
- One iteration should cover a subset of Story ACs, not all of them
- If you discover a contract needs to change, emit an ADR
- Keep iterations small and committable
- Always present the iteration to the user and wait for confirmation before invoking build

## Guardrails

Before presenting the iteration to the user, verify:

- [ ] Is this a feature? Then confirm a parent Story exists and is linked.
- [ ] Have you read the parent Story's ACs? (not assumed -- actually read with `lazyspec show --json`)
- [ ] Does every task reference real, verified file paths? (not guessed)
- [ ] Is the task breakdown detailed enough for a zero-context subagent?

If any answer is "no", stop. Complete the missing step.
