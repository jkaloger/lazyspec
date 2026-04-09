---
name: create-iteration
description: Use when planning an iteration against a Story or as a standalone iteration for bug fixes, tweaks, and refactors. Creates Iteration documents with task breakdown and test plan. Supports parallel subagent dispatch for Stories with multiple AC groups.
---

```
PLAN THE WORK, THEN CONFIRM BEFORE BUILDING
```

This skill creates the iteration document. It does NOT write code.

<HARD-GATE>
Do NOT write test code or production code. Plan tests and tasks only.
For feature work linked to a Story, use `/resolve-context` first if you haven't already.
Standalone iterations (bug fixes, tweaks, refactors) do not require a parent Story.
After identifying multiple AC groups, partition upfront and get user approval before dispatching subagents.
After completion: present the iteration to the user. Only use `/build` after explicit confirmation.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT write test or production code. This skill produces a plan document only.
- Do NOT dispatch subagents without user approval of the AC grouping.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

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

0. Load convention context: `lazyspec convention --tags iteration,testing --json`. Use non-empty results to inform task breakdown and test plan.
1. If linked to a Story: `lazyspec context <story-id> --json`, then `lazyspec show <story-id> --json` for ACs
2. `lazyspec status --json` to check no existing iteration covers the same ACs
3. Read relevant documents with `lazyspec show --json` before modifying anything

## AC Grouping (multi-iteration stories)

1. List all ACs from the story
2. Group into iteration-sized coherent chunks (consider dependencies)
3. Verify each AC belongs to exactly one group (no overlap, no gaps)
4. Present grouping table to user (title, ACs, rationale per group)
5. Wait for explicit approval. Revise if requested.
6. Dispatch one subagent (general-purpose, Opus) per iteration in parallel

Each subagent receives: full Story body, RFC design intent, its AC group, other groups' boundaries, and instructions to use `lazyspec create iteration`, `lazyspec link`, Explore agents for code discovery, and `lazyspec validate --json`.

## Single-iteration / Standalone Path

1. **Gather context:** `lazyspec status --json`, `lazyspec search "<keyword>" --json`. For Stories, use `/resolve-context` if not already done. For standalone work, gather context from the codebase directly.
2. **Discover code:** Use `lazyspec search --json` and read referenced file paths. Task breakdowns must reference real, verified file paths.
3. **Create:** `lazyspec create iteration "<title>" --author agent`
4. **Link (if applicable):** `lazyspec link <iteration-path> implements <story-path>` (standalone iterations skip this)
5. **Plan tests:** For each AC, describe the verifying test in `## Test Plan`. Do NOT write test code. Note tradeoffs between test properties (isolated, deterministic, behavioral, readable, specific, etc.) and present significant tradeoffs to the user.
6. **Write task breakdown** in `## Changes` as a numbered list. Each task must be self-contained for a zero-context subagent: ACs addressed, exact file paths, complete implementation description, verification steps.
7. **Document:** Add discoveries to `## Notes`. Create ADRs for significant decisions.
8. **Validate:** `lazyspec validate --json`
9. **Present to user.** Do NOT use `/build` until the user approves.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "Let me just start coding" | This skill plans. `/build` writes code. |
| "I'll write the tests now" | Plan tests here, write them during build. |
| "I'll use /build right after" | Present to user. Wait for confirmation. |
| "I don't need user approval for the grouping" | Always get approval. AC grouping affects iteration scope. |

## Verification

- [ ] `lazyspec validate --json` passes
- [ ] Each AC belongs to exactly one iteration (no overlap)
- [ ] Each task has file paths, ACs, implementation detail, verification steps
- [ ] `## Test Plan` documents planned tests
- [ ] Iteration presented to user; user confirmed before `/build`
- [ ] No test code or production code written
