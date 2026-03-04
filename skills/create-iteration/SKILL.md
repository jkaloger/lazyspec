---
name: create-iteration
description: Use when planning an iteration against a Story. Creates an Iteration document with task breakdown and test plan, then presents to user for confirmation before build.
---

```
PLAN THE WORK, THEN CONFIRM BEFORE BUILDING
```

This skill creates the iteration document. It does NOT write code.

<HARD-GATE>
Do NOT write test code or production code in this skill. Plan tests and
tasks only. If you haven't resolved context, invoke resolve-context first.
After completion: present the iteration to the user for review.
Only invoke build after the user explicitly confirms.
</HARD-GATE>

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

User confirms -> Invoke build

Present to user for review.shape: diamond
Invoke build.shape: double_circle
```

## Steps

1. **Gather context:** Run `lazyspec show <story-id>` to read the Story and its ACs. Check existing iterations: `lazyspec list iteration`. Use `lazyspec search "<keyword>"` to find related documents, ADRs, and prior work. If you haven't already resolved context, invoke resolve-context first.

2. **Discover relevant code:** Use `lazyspec search` to find specs that reference the modules and types you'll be working with. Read the referenced file paths from those specs to understand the existing code before planning tasks. Task breakdowns must reference real, verified file paths.

3. **Create the iteration:** Run `lazyspec create iteration "<title>" --author agent`

4. **Link to story:** Run `lazyspec link <iteration-path> implements <story-path>`

5. **Plan tests:** For each AC this iteration covers, describe the test that will verify it. Document these in the iteration's `## Test Plan` section. Do NOT write test code or production code yet -- that happens during build.

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

8. **Document:** Add any discoveries or decisions to `## Notes`. If a significant decision was made, create an ADR: `lazyspec create adr "<decision>"`.

9. **Validate:** Run `lazyspec validate`.

10. **Present to user:** Show the user the complete iteration document (task breakdown, test plan, linked ACs). Ask for explicit confirmation before proceeding. Do NOT invoke build until the user approves.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "Let me just start coding" | This skill plans. Build writes code. |
| "I'll write the tests now" | Plan the tests here, write them during build. |
| "I'll invoke build right after" | Stop. Present to the user. Wait for confirmation. |
| "The user will probably approve" | Probably isn't confirmed. Ask. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate` passes (iteration links to story)
- [ ] `## Changes` section contains a numbered task breakdown
- [ ] Each task references Story ACs, includes file paths, and describes implementation
- [ ] `## Test Plan` section documents planned tests for each AC
- [ ] Iteration has been presented to the user
- [ ] User has explicitly confirmed before build is invoked
- [ ] No test code or production code has been written

## Rules

- This skill produces a document, not code
- One iteration should cover a subset of Story ACs, not all of them
- If you discover a contract needs to change, emit an ADR
- Keep iterations small and committable
- Always present the iteration to the user and wait for confirmation before invoking build
