---
name: plan-work
description: Use when starting new work, planning a feature, or deciding what to implement next. Detects existing RFCs, Stories, and Iterations to determine the right starting point. Supports lightweight paths for bug fixes and small tweaks.
---

```
NO WORK WITHOUT A PLAN
```

If you're about to write code without knowing where you are in the workflow, stop. Plan first.

use the `lazyspec` cli tool. this is in the user's path.

<HARD-GATE>
Do NOT skip to implementation. Detect existing artifacts, classify the work,
and use the right skill.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT use /write-rfc, create-story, or create-iteration without user approval of the direction first.
</NEVER>

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
User describes work -> Detect existing artifacts -> Classify work

Classify work -> New feature (full pipeline)
Classify work -> Bug fix / small tweak (lightweight)

New feature (full pipeline) -> Determine entry point

Determine entry point -> No RFC: Brainstorm design
Determine entry point -> RFC exists, no Story: Brainstorm slices
Determine entry point -> Story exists, no Iteration: Resolve context
Determine entry point -> Iteration with tasks: Ready to build

No RFC: Brainstorm design -> User approves direction? -> Use /write-rfc skill: yes
User approves direction? -> Revise: no
Revise -> No RFC: Brainstorm design

RFC exists, no Story: Brainstorm slices -> Use /create-story skill
Story exists, no Iteration: Resolve context -> Use /resolve-context skill
Iteration with tasks: Ready to build -> Use /build skill

Bug fix / small tweak (lightweight) -> Related Story exists?
Related Story exists? -> Create iteration against it: yes
Related Story exists? -> Create standalone iteration: no

Create iteration against it -> Use /create-iteration skill
Create standalone iteration -> Use /create-iteration skill

Use /write-rfc skill.shape: double_circle
Use /create-story skill.shape: double_circle
Use /resolve-context skill.shape: double_circle
Use /create-iteration skill.shape: double_circle
Use /build skill.shape: double_circle
```

## Preflight

1. Run `lazyspec status --json` to get all documents, relationships, and validation in one call
2. Search for topic-specific matches: `lazyspec search "<topic>"`
3. Present findings to the user before choosing a direction
4. Classify the work (new feature, bug fix, tweak, refactor) before selecting a pipeline

## Subagent Dispatch

| Tier   | Model  | Use for                                                                         |
| ------ | ------ | ------------------------------------------------------------------------------- |
| Light  | Haiku  | Parsing frontmatter, extracting structured data, simple validation              |
| Medium | Sonnet | Codebase exploration, searching for patterns, reading and summarizing documents |
| Heavy  | Opus   | Implementation, complex reasoning, multi-file changes, review                   |

| Operation                 | Agent Type | Tier   | Context to provide                        |
| ------------------------- | ---------- | ------ | ----------------------------------------- |
| Search existing artifacts | Explore    | Medium | Topic keywords, document types to search  |
| Classify work complexity  | _(inline)_ | -      | No subagent needed, main agent classifies |

## Steps

### 1. Detect existing artifacts

Get the full project state and search for related work:

```
lazyspec status --json
lazyspec search "<topic>"
```

### 2. Present findings

Tell the user what you found:

- Which RFCs, Stories, Iterations already exist for this work
- Their current status (draft, accepted, etc.)
- What relationships exist between them

### 3. Classify the work

Not all work needs the full pipeline. Before determining entry point, classify what the user is asking for:

| Classification  | Criteria                                                              | Pipeline            |
| --------------- | --------------------------------------------------------------------- | ------------------- |
| **New feature** | Adds new capability or behavior. Even small features need a Story.    | Full (RFC optional) |
| **Bug fix**     | Corrects existing behavior that doesn't match intent.                 | Iteration only      |
| **Small tweak** | Minor adjustment to existing behavior (config change, copy, styling). | Iteration only      |
| **Refactor**    | Restructures code without changing behavior.                          | Iteration only      |

> [!NOTE]
> When unsure, ask the user. The classification determines how much ceremony the work gets.

**New features** always need a Story (and an RFC if the design is non-trivial or cross-cutting). This is the full pipeline.

**Bug fixes, small tweaks, and refactors** skip RFC and Story creation entirely. They go straight to `create-iteration`, optionally linked to an existing Story if one is related.

### 4. Determine entry point

**For new features** (full pipeline):

| State                                   | Action                                                 |
| --------------------------------------- | ------------------------------------------------------ |
| Nothing exists                          | Brainstorm the design, then use `/write-rfc`           |
| RFC exists, no Stories                  | Brainstorm vertical slices, then use `/create-story`   |
| Story exists, no Iteration              | Use `/resolve-context` (chains to `/create-iteration`) |
| Iteration exists with task breakdown    | Use `/build`                                           |
| Iteration exists without task breakdown | Use `/create-iteration` to add tasks                   |

**For bug fixes, tweaks, and refactors** (lightweight pipeline):

| State                               | Action                                            |
| ----------------------------------- | ------------------------------------------------- |
| Related Story exists                | Use `/create-iteration` linked to that Story      |
| No related Story (standalone fix)   | Use `/create-iteration` as a standalone iteration |
| Iteration already exists with tasks | Use `/build`                                      |

### 5. Brainstorm (when needed)

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
- Propose 2-3 slice approaches with trade-offs
- Get user approval before invoking create-story

**Iteration level (Story exists, no Iteration):**

- This is handled by create-iteration, which generates the task breakdown
- Propose 2-3 design approaches with trade-offs
- Use /resolve-context skill, which chains to create-iteration

**Lightweight iteration (bug fix / tweak):**

- Confirm the problem or change with the user
- If a related Story exists, confirm linking to it
- Propose 2-3 design approaches with trade-offs
- Use /create-iteration skill directly (no resolve-context needed for standalone iterations)

### 6. Use the appropriate skill

After determining the entry point and brainstorming (if needed), use the Skill tool to invoke the next skill (e.g. `/write-rfc`, `/create-story`, `/create-iteration`, `/build`). Each skill chains to its successor -- follow the chain, don't skip ahead.

## Red Flags

| Red Flag                                  | Reality                                                             |
| ----------------------------------------- | ------------------------------------------------------------------- |
| "Let me just start coding"                | Code without a plan = rework. Plan first.                           |
| "I already know what to build"            | Then the plan should be quick. Still do it.                         |
| "This is too small to plan"               | Small work still gets an iteration. The iteration can be small too. |
| "I'll figure out the design as I code"    | That's not design. That's hoping.                                   |
| "This bug fix needs an RFC"               | No it doesn't. Classify the work correctly.                         |
| "Let me create a Story for this typo fix" | Overkill. Bug fixes and tweaks skip Story creation.                 |

## Rules

- Always search for existing artifacts before creating new ones
- Present findings to the user before deciding direction
- Classify the work before choosing a pipeline
- New features need Stories. Bug fixes, tweaks, and refactors do not.
- Brainstorm at the appropriate level (RFC, Story, or Iteration)
- One question at a time during brainstorming
- Get user approval before invoking the next skill
- Never skip directly to build without an Iteration with tasks

## Guardrails

Before invoking any downstream skill, verify:

- [ ] Have you searched for existing artifacts? (`lazyspec search`, `lazyspec list`)
- [ ] Have you presented findings to the user?
- [ ] Has the user approved the direction?
- [ ] Are you invoking the correct skill for the work classification?

If any answer is "no", stop. Complete the missing step.
