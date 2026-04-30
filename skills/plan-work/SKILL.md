---
name: plan-work
description: Use when starting new work, planning a feature, or deciding what to implement next. Detects existing RFCs, Stories, and Iterations to determine the right starting point. Supports lightweight paths for bug fixes and small tweaks.
---

```
NO WORK WITHOUT A PLAN
```

If you're about to write code without knowing where you are in the workflow, stop. Plan first.

Use the `lazyspec` cli tool. This is in the user's path.

<HARD-GATE>
Do NOT skip to implementation. Detect existing artifacts, classify the work,
and use the right skill.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT use /write-rfc, /create-story, or /create-iteration without user approval of the direction first.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.

Always pass `--json` when the command supports it. Only omit `--json` when
presenting output directly to the user.

If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying.

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
Determine entry point -> RFC + Stories, no Iterations: Sweep iterations
Determine entry point -> Story exists, no Iteration: Resolve context
Determine entry point -> Iteration with tasks: Ready to build

Project-level work (SOW / multi-RFC) -> Use /project-plan skill
Project-level work (SOW / multi-RFC) -> Determine entry point

No RFC: Brainstorm design -> User approves direction? -> Use /write-rfc skill: yes
User approves direction? -> Revise: no
Revise -> No RFC: Brainstorm design

RFC exists, no Story: Brainstorm slices -> Use /create-story skill
RFC + Stories, no Iterations: Sweep iterations -> Use /create-iteration sweep mode
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
Use /create-iteration sweep mode.shape: double_circle
Use /project-plan skill.shape: double_circle
Use /build skill.shape: double_circle
```

## Preflight

Run these before choosing a direction:

```
lazyspec status --json
lazyspec search "<topic>" --json
```

Present findings to the user: which RFCs, Stories, Iterations exist, their status, and relationships. Get alignment before proceeding.

## Classify the Work

| Classification  | Criteria                                                           | Pipeline            |
| --------------- | ------------------------------------------------------------------ | ------------------- |
| **New feature** | Adds new capability or behavior. Even small features need a Story. | Full (RFC optional) |
| **Bug fix**     | Corrects existing behavior that doesn't match intent.              | Iteration only      |
| **Small tweak** | Minor adjustment (config change, copy, styling).                   | Iteration only      |
| **Refactor**    | Restructures code without changing behavior.                       | Iteration only      |

When unsure, ask the user.

## Entry Points

**New features** (full pipeline):

| State                                   | Action                                                 |
| --------------------------------------- | ------------------------------------------------------ |
| Nothing exists                          | Brainstorm the design, then use `/write-rfc`           |
| RFC exists, no Stories                  | Brainstorm vertical slices, then use `/create-story`   |
| RFC exists, Stories exist, no Iterations | Use `/create-iteration` sweep mode                    |
| Story exists, no Iteration              | Use `/resolve-context` (chains to `/create-iteration`) |
| Iteration exists with task breakdown    | Use `/build`                                           |
| Iteration exists without task breakdown | Use `/create-iteration` to add tasks                   |

**Bug fixes, tweaks, refactors** (lightweight pipeline). Standalone Iterations RESERVED for this lightweight pipeline only: bug fixes, tiny tweaks, refactors. Substantial dev work MUST `implements` one or more Stories.

| State                               | Action                                            |
| ----------------------------------- | ------------------------------------------------- |
| Related Story exists                | Use `/create-iteration` linked to that Story      |
| No related Story (standalone fix)   | Use `/create-iteration` as a standalone iteration |
| Iteration already exists with tasks | Use `/build`                                      |

## Brainstorming

Brainstorming is fractal -- it applies at whatever level you're entering:

**RFC level (no RFC exists):**
- Ask clarifying questions about the problem (one at a time)
- Propose 2-3 design approaches with trade-offs
- Present your recommendation
- Get user approval before invoking `/write-rfc`

**Story level (RFC exists, no Stories):**
- Read the RFC to understand intent
- Propose vertical slices with scope discussion
- Get user approval before invoking `/create-story`

**Iteration level (Story exists, no Iteration):**
- Single-Story path: use `/resolve-context` skill, which chains to `/create-iteration`
- RFC sweep path (RFC + multiple Stories, no Iterations): use `/create-iteration` sweep mode to walk all Stories and propose per-Story / shared-contract / cross-cutting / polish Iterations as a batch

**Lightweight iteration (bug fix / tweak):**
- Confirm the problem or change with the user
- If a related Story exists, confirm linking to it
- Use `/create-iteration` skill directly

**Project level (SOW / multi-RFC scope):**
- If the work spans multiple RFCs or starts from a SOW / project brief, defer to `/project-plan` first. It decomposes the project into RFCs, then each RFC re-enters this skill at the RFC level.

## Guardrails

Before invoking any downstream skill, verify:

- [ ] Searched for existing artifacts (`lazyspec status --json`, `lazyspec search --json`)
- [ ] Presented findings to the user
- [ ] User approved the direction
- [ ] Invoking the correct skill for the work classification
- [ ] Not skipping directly to `/build` without an Iteration with tasks

If any answer is "no", stop and complete the missing step.
