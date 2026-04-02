---
name: fix-bug
description: Use when investigating and fixing a bug, regression, or behavioral mismatch. Structures the fix around observed vs desired behavior and creates a focused iteration.
---

```
REPRODUCE FIRST. DOCUMENT THE GAP. THEN PLAN.
```

Dedicated workflow for bug fixes. Where `/plan-work` routes features through RFC and Story, this skill handles the bug-specific path: investigate, document the behavioral gap, produce an iteration linked to the relevant Story.

<HARD-GATE>
Do NOT skip to code. Reproduce the bug, document observed vs desired behavior, then create the iteration.
Every bug fix iteration MUST link to a Story. Find the relevant Story or ask the user which Story this bug belongs to.
For feature work, use `/plan-work` instead.
After completion: present the iteration to the user. Only use `/build` after explicit confirmation.
</HARD-GATE>

<NEVER>
- Do NOT create standalone iterations. Bugs always trace to a Story.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT write test or production code. This skill produces a plan only.
- Do NOT assume the bug's cause before investigating.
</NEVER>

Use `lazyspec create` and `lazyspec link` for document creation and linking. Edit iteration files directly (with `Edit` or `Write`) for content like `## Bug`, `## Changes`, and `## Test Plan`.

Always pass `--json`. Run `lazyspec help <subcommand>` before unfamiliar commands.

## Workflow

```d2
User reports bug -> Search for related artifacts -> Reproduce the bug

Reproduce the bug -> Reproducible? {shape: diamond}
Reproducible? -> Investigate codebase: yes
Reproducible? -> Gather more context: no
Gather more context -> Reproduce the bug

Investigate codebase -> Document observed vs desired -> Find related Story

Find related Story -> Story found? {shape: diamond}
Story found? -> Create iteration linked to Story: yes
Story found? -> Ask user which Story this belongs to: no
Ask user which Story this belongs to -> Create iteration linked to Story

Create iteration linked to Story -> Write task breakdown -> Present to user

Present to user {shape: diamond}
Present to user -> Use /build skill: approved {shape: double_circle}
Present to user -> Revise: changes requested
Revise -> Write task breakdown
```

## Preflight

```
lazyspec status --json
lazyspec search "<bug topic>" --json
```

Search for Stories, Iterations, or RFCs that relate to the affected area. Present findings to the user before proceeding.

## Reproduce

Before documenting anything, reproduce the bug:

1. Identify the trigger (command, input, sequence of actions).
2. Run it. Record the actual output or behavior.
3. If not reproducible, gather more context: read logs, check related code, ask the user for reproduction steps.

Do not proceed to documentation until the bug is reproducible or the user confirms it's intermittent and provides enough context to characterise it.

## Investigate

After reproducing, understand the code involved before planning a fix:

1. Search for the relevant code paths (`lazyspec search --json`, ast-grep, Grep).
2. Read the functions and modules involved in the buggy behavior.
3. Identify the root cause or narrow it to a specific area.

This step produces the context needed to write a meaningful task breakdown. Without it, tasks will be vague.

## Document: Observed vs Desired

Every bug fix iteration includes a `## Bug` section with two subsections:

**### Observed Behavior** — What the system actually does. Include the trigger (command, input), the output or behavior, and why it's wrong.

**### Desired Behavior** — What the system should do instead. Reference the Story AC, spec, or stated intent that defines correctness.

The gap between these two is the bug. The iteration's task breakdown closes that gap.

## Find the Related Story

Every bug is a violation of intended behavior, and that intent lives in a Story. The iteration must link to one.

1. Check preflight results for Stories in the affected area.
2. If a Story clearly covers the buggy behavior, use it.
3. If multiple Stories could apply, ask the user which one.
4. If no Story exists, ask the user: "Which Story does this bug belong to?" If the user wants a new Story created, use `/create-story` first, then link the iteration to it. Do not proceed without a Story link.

## Create Iteration

1. `lazyspec create iteration "<title>" --author agent`
2. `lazyspec link <iteration-path> implements <story-path>`
3. Edit the iteration file: write the `## Bug` section (observed vs desired).
4. Write `## Changes` as a numbered task list. Each task: files, what to implement, how to verify.
5. Write `## Test Plan`: at minimum, a regression test asserting the desired behavior.
6. `lazyspec validate --json`
7. Present to user. Do NOT use `/build` until approved.

## Task Breakdown

Each task in `## Changes`:

- **Files**: paths to modify or create
- **What to implement**: the specific change that moves from observed to desired
- **How to verify**: command to run

Keep tasks small. A bug fix rarely needs more than 2-3 tasks: the fix itself, and a regression test.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I know what's wrong, let me just fix it" | Reproduce first. Document the gap. Then plan. |
| "The fix is obvious, no iteration needed" | Even obvious fixes benefit from observed/desired documentation. |
| "I'll add the test later" | Test plan is part of the iteration. Plan it now. |
| "This is too small for a document" | The document takes 2 minutes. Skipping it means no paper trail. |
| "No Story covers this, I'll go standalone" | Every bug violates some intended behavior. Ask the user which Story it belongs to. |
| "Creating a Story link is overhead for a small fix" | The link takes one command. It's how you trace what broke and why. |

## Verification

- [ ] Bug reproduced or characterised with user input
- [ ] `## Bug` section documents observed and desired behavior
- [ ] Iteration linked to a Story (no standalone bug iterations)
- [ ] `lazyspec validate --json` passes
- [ ] Each task has file paths, implementation detail, verification steps
- [ ] `## Test Plan` includes a regression test
- [ ] Iteration presented to user; user confirmed before `/build`
- [ ] No test code or production code written
