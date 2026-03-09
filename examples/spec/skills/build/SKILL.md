---
name: build
description: Use when a Plan has a task breakdown and is ready for implementation. Dispatches subagent per task with review between tasks.
---

```
NO IMPLEMENTATION WITHOUT A PLAN
```

If the Plan doesn't have a numbered task breakdown in `## Changes`, you can't build yet. Use the `/create-plan` skill first.

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<HARD-GATE>
Do NOT begin implementation without a complete Plan document with numbered
task breakdown. Each task must have enough detail for a zero-context subagent.
ALWAYS use subagents for development.
</HARD-GATE>

<NEVER>
- Do NOT implement tasks yourself. Dispatch a subagent per task. Do NOT dispatch parallel implementers.
</NEVER>

# Build

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build

build.style.fill: "#4A9EFF"
build.style.font-color: "#FFFFFF"
lazy.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-spec.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-lazy.style.opacity: 0.4
```

## Workflow

```d2
Read plan -> Extract all tasks -> Create task tracking

Per task {
  Dispatch implementer subagent -> Questions? -> Answer, re-dispatch: yes
  Questions? -> Implementer works + self-reviews: no
  Implementer works + self-reviews -> Dispatch reviewer subagent
  Dispatch reviewer subagent -> Contract compliance passes?
  Contract compliance passes? -> Implementer fixes -> Dispatch reviewer subagent: no
  Contract compliance passes? -> Code quality passes?: yes
  Code quality passes? -> Implementer fixes quality -> Dispatch reviewer subagent: no
  Code quality passes? -> Mark task complete: yes
}

Mark task complete -> More tasks?

More tasks?.shape: diamond
More tasks? -> Per task: yes
More tasks? -> Final full review: no

Final full review -> All Spec contracts met?

All Spec contracts met?.shape: diamond
All Spec contracts met? -> Done: yes
All Spec contracts met? -> Fix gaps: no

Done.shape: double_circle
```

## Preflight

1. Resolve the full chain with `lazyspec context <plan-id> --json` to see the document chain
2. Read the plan body with `lazyspec show <plan-id> --json` to get the task breakdown
3. Read the parent Spec body with `lazyspec show <spec-id> --json` to get the contracts
4. If an RFC exists in the chain, read it with `lazyspec show <rfc-id> --json` for design intent
5. Extract all tasks from `## Changes` before dispatching any subagent

## Subagent Dispatch

| Operation                        | Agent Type      | Tier   | Context to provide                                       |
| -------------------------------- | --------------- | ------ | -------------------------------------------------------- |
| Implement task                   | general-purpose | Heavy  | Full task text, RFC intent, Spec contracts, prior task results |
| Review task (contract compliance)| general-purpose | Heavy  | Task text, Spec contracts, implementer report            |
| Review task (code quality)       | general-purpose | Medium | Changed files, test output, quality criteria             |
| Final review                     | general-purpose | Heavy  | All Spec contracts, full implementation summary          |

> This skill follows the [obra/superpowers](https://github.com/obra/superpowers) subagent-driven development pattern: dispatch a fresh subagent per task, with two-stage review (spec compliance first, code quality second). Each subagent starts with zero prior context to prevent pollution. The reviewer is always a separate agent from the implementer.

## Setup

1. **Resolve the chain:** Run `lazyspec context <plan-id> --json` to see the full document chain at a glance.
2. **Read the documents:** Run `lazyspec show <plan-id> --json` and `lazyspec show <spec-id> --json` to get the full bodies (task breakdown, contracts). If an RFC exists in the chain, also run `lazyspec show <rfc-id> --json` for design intent.
3. **Extract all tasks** from the plan's `## Changes` section. Copy the full text of each task -- subagents receive text, not file references.
4. **Create task tracking** using TodoWrite with one entry per task.

## Per-Task Loop

For each task in the plan:

### 6. Dispatch implementer subagent

Use the Agent tool with `subagent_type: "general-purpose"`. Provide:

- **Full task text** (copied from plan, not a reference to the file)
- **Scene-setting context:** RFC intent (1-2 sentences), Spec contracts relevant to this task, what prior tasks accomplished
- **Self-review checklist:** Completeness, quality, discipline, testing
- **Working directory**

Read the prompt template from `prompts/implementer.md` (relative to this skill directory) and include it in the subagent prompt.

### 7. Handle implementer questions

If the implementer asks questions, answer them. Provide additional context if needed. Don't rush them into implementation.

### 8. Dispatch reviewer subagent

After the implementer reports back, dispatch a **separate** reviewer subagent using the Agent tool with `subagent_type: "general-purpose"`.

Read the prompt template from `prompts/reviewer.md` (relative to this skill directory) and include it in the subagent prompt, filling in the placeholders with the actual task text, spec contracts, and implementer report.

### 9. Handle review failures

If the reviewer reports issues:

- Dispatch a fresh implementer subagent with the specific issues to fix
- After fixes, dispatch reviewer again
- Repeat until both stages pass

### 10. Mark task complete

Update task tracking. Proceed to next task.

### 10a. Context refresh (every 2 tasks)

After completing tasks 2, 4, 6, etc., re-read the chain to prevent context drift:

1. Run `lazyspec context <plan-id> --json` to verify the chain is intact
2. Run `lazyspec show <plan-id> --json` to refresh the task list and status
3. Run `lazyspec show <spec-id> --json` to refresh the contracts
4. Verify you are still following the task breakdown as written (not improvising)

## Final Review

### 10b. Pipeline checkpoint

Before the final review, verify the workflow is intact:

1. Re-read the plan document: `lazyspec show <plan-id> --json`
2. Confirm all tasks in `## Changes` have been completed
3. Check that no work was done outside the task breakdown
4. Run `lazyspec validate --json` to check document integrity

If anything is out of alignment, fix it before proceeding to final review.

### 11. Dispatch final reviewer

After all tasks complete, dispatch a reviewer subagent. Read the prompt template from `prompts/final-reviewer.md` and include it in the subagent prompt, filling in the spec contracts and task summary.

### 12. Update document statuses

If final review passes, follow the status promotion steps in `_common.md`.

## Red Flags

| Red Flag                                                 | Reality                                                                                       |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| "I'll review all tasks at the end"                       | Per-task review catches issues early. Fixing one task is cheaper than fixing five.            |
| "The implementer self-reviewed, that's enough"           | Self-review is necessary but not sufficient. The reviewer is a separate subagent.             |
| "I'll skip contract compliance and just do code quality" | Contract compliance FIRST. Beautiful code that doesn't meet the spec is wrong code.           |
| "Let me implement two tasks before reviewing"            | One task, one review. No batching.                                                            |
| "I'll provide a file reference instead of the full text" | Subagents receive full text. File references require them to read and parse, wasting context. |

## Checklist

Before dispatching any implementer subagent:

- [ ] Does a Plan document exist with a numbered task breakdown?
- [ ] Have you read the Plan and Spec documents? (and RFC if one exists)
- [ ] Are you dispatching tasks sequentially (not in parallel)?
- [ ] Is the subagent receiving full task text (not a file reference)?

Before claiming build is complete:

- [ ] All tasks reviewed and passing (both stages)
- [ ] Final review dispatched and passing
- [ ] Document statuses updated per `_common.md`
- [ ] `lazyspec validate --json` passes
