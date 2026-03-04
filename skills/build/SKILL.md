---
name: build
description: Use when an Iteration has a task breakdown and is ready for implementation. Dispatches subagent per task with review between tasks.
---

```
NO IMPLEMENTATION WITHOUT A PLAN
```

If the Iteration doesn't have a numbered task breakdown in `## Changes`, you can't build yet. Invoke create-iteration first.

<HARD-GATE>
Do NOT begin implementation without a complete Iteration document with numbered
task breakdown. Each task must have enough detail for a zero-context subagent.
</HARD-GATE>

# Build

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build

build.style.fill: "#4A9EFF"
build.style.font-color: "#FFFFFF"
plan.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
```

## Workflow

```d2
Read iteration -> Extract all tasks -> Create task tracking

Per task {
  Dispatch implementer subagent -> Questions? -> Answer, re-dispatch: yes
  Questions? -> Implementer works + self-reviews: no
  Implementer works + self-reviews -> Dispatch reviewer subagent
  Dispatch reviewer subagent -> AC compliance passes?
  AC compliance passes? -> Implementer fixes -> Dispatch reviewer subagent: no
  AC compliance passes? -> Code quality passes?: yes
  Code quality passes? -> Implementer fixes quality -> Dispatch reviewer subagent: no
  Code quality passes? -> Mark task complete: yes
}

Mark task complete -> More tasks?

More tasks?.shape: diamond
More tasks? -> Per task: yes
More tasks? -> Final full review: no

Final full review -> All Story ACs met?

All Story ACs met?.shape: diamond
All Story ACs met? -> Done: yes
All Story ACs met? -> Fix gaps: no

Done.shape: double_circle
```

## Setup

1. **Read the iteration:** Run `lazyspec show <iteration-id>` to get the full document with task breakdown.
2. **Read the parent story:** Follow the `implements` link. Run `lazyspec show <story-id>` to get the ACs.
3. **Read the RFC:** Follow the Story's `implements` link. Run `lazyspec show <rfc-id>` for design intent.
4. **Extract all tasks** from the iteration's `## Changes` section. Copy the full text of each task -- subagents receive text, not file references.
5. **Create task tracking** using TodoWrite with one entry per task.

## Per-Task Loop

For each task in the iteration:

### 6. Dispatch implementer subagent

Use the Agent tool with `subagent_type: "general-purpose"`. Provide:

- **Full task text** (copied from iteration, not a reference to the file)
- **Scene-setting context:** RFC intent (1-2 sentences), Story ACs relevant to this task, what prior tasks accomplished
- **Self-review checklist:** Completeness, quality, discipline, testing
- **Working directory**

Include in the prompt:

```
Before you begin: if you have questions about requirements, approach,
dependencies, or anything unclear -- ask them now. Don't guess.

To find relevant files and prior work, use the lazyspec CLI:
- `lazyspec search "<query>"` to find documents by keyword
- `lazyspec show <path>` to read a document and its links
- `lazyspec list iteration` to see existing iterations

Use lazyspec to discover related specs before grepping the codebase.

Your job:
1. Implement exactly what the task specifies
2. Write tests (TDD: failing test first, then implementation)
3. Run tests, verify they pass
4. Self-review your work against these criteria:
   - Completeness: does it satisfy the task ACs?
   - Quality: is the code clear and correct?
   - YAGNI: did you build only what was asked for?
   - DRY: is there real duplication to extract? (three instances, not two)
   - Test properties: are your tests behavioral (not implementation-coupled),
     isolated (no order dependence), deterministic, readable (motivation
     obvious), and specific (failure cause obvious)?
   - Tradeoffs: if you traded a test property for another (e.g. speed for
     predictiveness in an integration test), note it.
5. Report: what you implemented, test results, files changed, concerns
```

### 7. Handle implementer questions

If the implementer asks questions, answer them. Provide additional context if needed. Don't rush them into implementation.

### 8. Dispatch reviewer subagent

After the implementer reports back, dispatch a **separate** reviewer subagent using the Agent tool with `subagent_type: "general-purpose"`.

The reviewer runs review-iteration adapted for per-task scope:

```
You are reviewing a task implementation for spec compliance and code quality.

## What Was Requested
[Full task text from iteration]

## Story ACs This Task Addresses
[Relevant ACs from the parent Story]

## What Implementer Claims
[From implementer's report]

## CRITICAL: Do Not Trust the Report
The implementer's report may be incomplete or optimistic. Verify independently.

## Stage 1: AC Compliance
- Run the test suite. Show full output.
- For each AC this task claims to address: verify the test exists and passes.
- Check for missing requirements the implementer skipped.
- Check for extra work not in the spec.
- If any AC is not satisfied: report ❌ with specifics.

## Stage 2: Code Quality (only if Stage 1 passes)
- Review code for correctness and clarity
- Verify no unnecessary complexity (YAGNI -- only what was asked for)
- Check for real duplication worth extracting (DRY -- three instances, not two)
- Check for security issues
- Evaluate tests against these properties:
  - Behavioral: tests assert on behavior, not implementation details
  - Structure-insensitive: a refactor that preserves behavior shouldn't break tests
  - Isolated: no order dependence between tests
  - Deterministic: no flaky results from timing, randomness, or shared state
  - Readable: the motivation for each test is obvious
  - Specific: when a test fails, the cause is obvious
  - If a property was traded for another (e.g. Fast for Predictive), the
    tradeoff should be noted and justified

Report:
- Stage 1: ✅ or ❌ with specifics
- Stage 2: ✅ or ❌ with specifics (only if Stage 1 passed)
```

### 9. Handle review failures

If the reviewer reports issues:
- Dispatch a fresh implementer subagent with the specific issues to fix
- After fixes, dispatch reviewer again
- Repeat until both stages pass

### 10. Mark task complete

Update task tracking. Proceed to next task.

## Final Review

### 11. Dispatch final reviewer

After all tasks complete, dispatch a reviewer subagent for a full review-iteration:

```
You are performing a final review of the complete implementation.

## Full Story ACs
[All ACs from the parent Story]

## Iteration Task Summary
[Summary of all tasks and what was implemented]

## Stage 1: AC Compliance
- Run the FULL test suite fresh. Show output.
- For EVERY Story AC: verify a passing test exists.
- Any unmet AC = ❌.

## Stage 2: Code Quality
- Review the full implementation holistically
- Check for consistency across tasks
- Verify no duplication or conflicting patterns (DRY)
- Verify no unnecessary abstractions or features (YAGNI)
- Evaluate test quality: behavioral, structure-insensitive, isolated,
  deterministic, readable, specific. Flag unjustified property tradeoffs.
```

### 12. Update document statuses

If final review passes, mark documents as accepted:

```bash
# Mark the iteration as accepted
lazyspec update <iteration-path> --status accepted

# If ALL Story ACs are now covered by accepted iterations,
# mark the Story as accepted
lazyspec update <story-path> --status accepted

# If ALL Stories under an RFC are accepted,
# mark the RFC as accepted
lazyspec update <rfc-path> --status accepted
```

Check each level: iteration -> story -> RFC. Only promote a parent document when all its children are complete.

Run `lazyspec validate` after all updates.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I'll review all tasks at the end" | Per-task review catches issues early. Fixing one task is cheaper than fixing five. |
| "The implementer self-reviewed, that's enough" | Self-review is necessary but not sufficient. The reviewer is a separate subagent. |
| "I'll skip spec compliance and just do code quality" | Spec compliance FIRST. Beautiful code that doesn't meet the spec is wrong code. |
| "Let me implement two tasks before reviewing" | One task, one review. No batching. |
| "I'll provide a file reference instead of the full text" | Subagents receive full text. File references require them to read and parse, wasting context. |

## Rules

- Fresh subagent per task (no context pollution)
- Reviewer is always a separate subagent from implementer
- Stage 1 (AC compliance) MUST pass before Stage 2 (code quality)
- Subagent receives full task text, not file references
- Answer implementer questions before letting them proceed
- One task, one review cycle. No batching tasks.
- Do not dispatch implementation subagents in parallel (conflicts)
- Always update document statuses after a successful final review
- Promote parent documents (Story, RFC) only when all children are accepted
