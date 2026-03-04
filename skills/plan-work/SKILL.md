---
name: plan-work
description: Use when starting new work, planning a feature, or deciding what to implement next. Detects existing RFCs, Stories, and Iterations to determine the right starting point.
---

```
NO WORK WITHOUT A PLAN
```

If you're about to write code without knowing where you are in the workflow, stop. Plan first.

<HARD-GATE>
Do NOT skip to implementation. Detect existing artifacts, brainstorm at the
appropriate level, and invoke the right skill.
</HARD-GATE>

# Plan

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build

plan.style.fill: "#4A9EFF"
plan.style.font-color: "#FFFFFF"
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
User describes work -> Detect existing artifacts -> Determine entry point

Determine entry point -> No RFC: Brainstorm design
Determine entry point -> RFC exists, no Story: Brainstorm slices
Determine entry point -> Story exists, no Iteration: Resolve context
Determine entry point -> Iteration with tasks: Ready to build

No RFC: Brainstorm design -> User approves direction? -> Invoke write-rfc: yes
User approves direction? -> Revise: no
Revise -> No RFC: Brainstorm design

RFC exists, no Story: Brainstorm slices -> Invoke create-story
Story exists, no Iteration: Resolve context -> Invoke resolve-context
Iteration with tasks: Ready to build -> Invoke build

Invoke write-rfc.shape: double_circle
Invoke create-story.shape: double_circle
Invoke resolve-context.shape: double_circle
Invoke build.shape: double_circle
```

## Steps

### 1. Detect existing artifacts

Search for work related to what the user described:

```
lazyspec search "<topic>"
lazyspec list rfc
lazyspec list story
lazyspec list iteration --status draft
```

### 2. Present findings

Tell the user what you found:

- Which RFCs, Stories, Iterations already exist for this work
- Their current status (draft, accepted, etc.)
- What relationships exist between them

### 3. Determine entry point

Based on what exists:

| State                                   | Action                                                  |
| --------------------------------------- | ------------------------------------------------------- |
| Nothing exists                          | Brainstorm the design, then invoke `write-rfc`          |
| RFC exists, no Stories                  | Brainstorm vertical slices, then invoke `create-story`  |
| Story exists, no Iteration              | Invoke `resolve-context` (chains to `create-iteration`) |
| Iteration exists with task breakdown    | Invoke `build`                                          |
| Iteration exists without task breakdown | Invoke `create-iteration` to add tasks                  |

### 4. Brainstorm (when needed)

Brainstorming is fractal -- it applies at whatever level you're entering:

**RFC level (no RFC exists):**

- Ask clarifying questions about the problem (one at a time)
- Propose 2-3 design approaches with trade-offs
- Present your recommendation
- Get user approval before invoking write-rfc

**Story level (RFC exists, no Stories):**

- Read the RFC to understand intent
- Propose vertical slices
- Discuss scope of each slice
- Get user approval before invoking create-story

**Iteration level (Story exists, no Iteration):**

- This is handled by create-iteration, which generates the task breakdown
- Invoke resolve-context, which chains to create-iteration

### 5. Invoke the appropriate skill

After determining the entry point and brainstorming (if needed), invoke the skill. Each skill chains directly to its successor.

## Red Flags

| Red Flag                               | Reality                                      |
| -------------------------------------- | -------------------------------------------- |
| "Let me just start coding"             | Code without a plan = rework. Plan first.    |
| "I already know what to build"         | Then the plan should be quick. Still do it.  |
| "This is too small to plan"            | Small unplanned work causes the most rework. |
| "I'll figure out the design as I code" | That's not design. That's hoping.            |

## Rules

- Always search for existing artifacts before creating new ones
- Present findings to the user before deciding direction
- Brainstorm at the appropriate level (RFC, Story, or Iteration)
- One question at a time during brainstorming
- Get user approval before invoking the next skill
- Never skip directly to build without an Iteration with tasks
