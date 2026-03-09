---
name: lazy
description: Use when starting new work, planning a feature, or deciding what to implement next. Detects existing RFCs, Specs, and Plans to determine the right starting point. Supports lightweight paths for bug fixes and small tweaks.
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

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<NEVER>
- Do NOT use /write-rfc, create-spec, or create-plan without user approval of the direction first.
</NEVER>

# Lazy

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build

lazy.style.fill: "#4A9EFF"
lazy.style.font-color: "#FFFFFF"
write-rfc.style.opacity: 0.4
create-spec.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-plan.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
User describes work -> Detect existing artifacts -> Classify work

Classify work -> New feature (full pipeline)
Classify work -> Bug fix / small tweak (lightweight)

New feature (full pipeline) -> Determine entry point

Determine entry point -> Nothing exists: Brainstorm
Determine entry point -> RFC exists, no Spec: Brainstorm slices
Determine entry point -> Spec exists, no Plan: Resolve context
Determine entry point -> Plan with tasks: Ready to build

Nothing exists: Brainstorm -> User approves direction? -> Use /create-spec skill: yes
User approves direction? -> Revise: no
Revise -> Nothing exists: Brainstorm

Nothing exists: Brainstorm -> Heavy/cross-cutting? -> Use /write-rfc skill: yes (optional)

RFC exists, no Spec: Brainstorm slices -> Use /create-spec skill
Spec exists, no Plan: Resolve context -> Use /resolve-context skill
Plan with tasks: Ready to build -> Use /build skill

Bug fix / small tweak (lightweight) -> Related Spec exists?
Related Spec exists? -> Create plan against it: yes
Related Spec exists? -> Create standalone plan: no

Create plan against it -> Use /create-plan skill
Create standalone plan -> Use /create-plan skill

Use /write-rfc skill.shape: double_circle
Use /create-spec skill.shape: double_circle
Use /resolve-context skill.shape: double_circle
Use /create-plan skill.shape: double_circle
Use /build skill.shape: double_circle
```

## Preflight

1. Run `lazyspec status --json` to get all documents, relationships, and validation in one call
2. Search for topic-specific matches: `lazyspec search "<topic>" --json`
3. Present findings to the user before choosing a direction
4. Classify the work (new feature, bug fix, tweak, refactor) before selecting a pipeline

## Subagent Dispatch

| Operation                 | Agent Type | Tier   | Context to provide                        |
| ------------------------- | ---------- | ------ | ----------------------------------------- |
| Search existing artifacts | Explore    | Medium | Topic keywords, document types to search  |
| Classify work complexity  | _(inline)_ | -      | No subagent needed, main agent classifies |

## Steps

### 1. Detect existing artifacts

Get the full project state and search for related work:

```
lazyspec status --json
lazyspec search "<topic>" --json
lazyspec context DOC-XXX --json
```

### 2. Present findings

Tell the user what you found:

- Which RFCs, Specs, Plans already exist for this work
- Their current status (draft, accepted, etc.)
- What relationships exist between them

### 3. Classify the work

Not all work needs the full pipeline. Before determining entry point, classify what the user is asking for:

| Classification  | Criteria                                                              | Pipeline                    |
| --------------- | --------------------------------------------------------------------- | --------------------------- |
| **New feature** | Adds new capability or behavior. Even small features need a Spec.     | Spec -> Plan (RFC optional) |
| **Bug fix**     | Corrects existing behavior that doesn't match intent.                 | Plan only                   |
| **Small tweak** | Minor adjustment to existing behavior (config change, copy, styling). | Plan only                   |
| **Refactor**    | Restructures code without changing behavior.                          | Plan only                   |

> [!NOTE]
> When unsure, ask the user. The classification determines how much ceremony the work gets.

**New features** always need a Spec. RFCs are optional, reserved for heavy or cross-cutting designs that affect multiple areas. Most features go straight to Spec.

**Project RFCs** should be linked if available.

**Bug fixes, small tweaks, and refactors** skip RFC and Spec creation entirely. They go straight to `create-plan`, optionally linked to an existing Spec if one is related.

### 4. Determine entry point

**For new features** (full pipeline):

| State                                | Action                                              |
| ------------------------------------ | --------------------------------------------------- |
| Nothing exists                       | Brainstorm the design, then use `/create-spec`      |
| Nothing exists (heavy/cross-cutting) | Brainstorm the design, then use `/write-rfc`        |
| RFC exists, no Specs                 | Brainstorm contract slices, then use `/create-spec` |
| Spec exists, no Plan                 | Use `/resolve-context` (chains to `/create-plan`)   |
| Plan exists with task breakdown      | Use `/build`                                        |
| Plan exists without task breakdown   | Use `/create-plan` to add tasks                     |

**For bug fixes, tweaks, and refactors** (lightweight pipeline):

| State                            | Action                                  |
| -------------------------------- | --------------------------------------- |
| Related Spec exists              | Use `/create-plan` linked to that Spec  |
| No related Spec (standalone fix) | Use `/create-plan` as a standalone plan |
| Plan already exists with tasks   | Use `/build`                            |

### 5. Brainstorm (when needed)

Brainstorming is fractal -- it applies at whatever level you're entering:

**Spec level (nothing exists, or RFC exists):**

- Ask clarifying questions about the problem (one at a time)
- Propose 2-3 design approaches with trade-offs
- Present your recommendation
- If an RFC exists, read it to understand intent and propose contract slices
- Get user approval before invoking create-spec

**RFC level (heavy/cross-cutting work only):**

- Only when the work spans multiple areas, teams, or requires significant architectural decisions
- Ask clarifying questions about the problem (one at a time)
- Propose 2-3 design approaches with trade-offs
- Present your recommendation
- Get user approval before invoking write-rfc

**Plan level (Spec exists, no Plan):**

- This is handled by create-plan, which generates the task breakdown
- Propose 2-3 design approaches with trade-offs
- Use /resolve-context skill, which chains to create-plan

**Lightweight plan (bug fix / tweak):**

- Confirm the problem or change with the user
- If a related Spec exists, confirm linking to it
- Propose 2-3 design approaches with trade-offs
- Use /create-plan skill directly (no resolve-context needed for standalone plans)

### 6. Use the appropriate skill

After determining the entry point and brainstorming (if needed), use the Skill tool to invoke the next skill (e.g. `/write-rfc`, `/create-spec`, `/create-plan`, `/build`). Each skill chains to its successor -- follow the chain, don't skip ahead.

## Red Flags

| Red Flag                                 | Reality                                                  |
| ---------------------------------------- | -------------------------------------------------------- |
| "Let me just start coding"               | Code without a plan = rework. Plan first.                |
| "I already know what to build"           | Then the plan should be quick. Still do it.              |
| "This is too small to plan"              | Small work still gets a plan. The plan can be small too. |
| "I'll figure out the design as I code"   | That's not design. That's hoping.                        |
| "This bug fix needs an RFC"              | No it doesn't. Classify the work correctly.              |
| "Let me create a Spec for this typo fix" | Overkill. Bug fixes and tweaks skip Spec creation.       |

## Checklist

Before invoking any downstream skill:

- [ ] Have you searched for existing artifacts? (`lazyspec search --json`, `lazyspec list --json`)
- [ ] Have you presented findings to the user?
- [ ] Has the user approved the direction?
- [ ] Are you invoking the correct skill for the work classification?

If any answer is "no", stop. Complete the missing step.

## Rules

- New features need Specs. RFCs are optional (for heavy/cross-cutting work). Bug fixes, tweaks, and refactors do not.
- One question at a time during brainstorming
- Get user approval before invoking the next skill
- Never skip directly to build without a Plan with tasks
