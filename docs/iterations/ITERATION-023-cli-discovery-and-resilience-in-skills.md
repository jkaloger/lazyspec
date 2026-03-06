---
title: CLI Discovery and Resilience in Skills
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related: []
validate-ignore: true
---



## Changes

### Task 1: Add CLI discovery preamble to all skills

**Files:**
- Modify: `.claude/skills/plan-work/SKILL.md`
- Modify: `.claude/skills/write-rfc/SKILL.md`
- Modify: `.claude/skills/create-story/SKILL.md`
- Modify: `.claude/skills/create-iteration/SKILL.md`
- Modify: `.claude/skills/resolve-context/SKILL.md`
- Modify: `.claude/skills/build/SKILL.md`
- Modify: `.claude/skills/review-iteration/SKILL.md`

**What to implement:**

Add a `## CLI Reference` section immediately after the `## Forbidden Actions` block in every skill. This section should instruct agents to discover the CLI before using it:

```markdown
## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.
```

This replaces the assumption that agents know the CLI interface. The section should be identical across all skills for consistency.

**How to verify:**
- Every skill file under `.claude/skills/*/SKILL.md` contains the `## CLI Reference` section
- The section appears after `## Forbidden Actions` and before the skill's main heading

### Task 2: Default to --json everywhere

**Files:**
- Modify: `.claude/skills/plan-work/SKILL.md`
- Modify: `.claude/skills/write-rfc/SKILL.md`
- Modify: `.claude/skills/create-story/SKILL.md`
- Modify: `.claude/skills/create-iteration/SKILL.md`
- Modify: `.claude/skills/resolve-context/SKILL.md`
- Modify: `.claude/skills/build/SKILL.md`
- Modify: `.claude/skills/review-iteration/SKILL.md`

**What to implement:**

Add a rule to the `## CLI Reference` section (from Task 1):

```markdown
Always pass `--json` when the command supports it. This gives you structured,
parseable output. Only omit `--json` when presenting output directly to the user.
```

Then audit every inline command example across all skills. Where a command supports `--json` but the example omits it, add `--json`. Specifically:
- `lazyspec list rfc` -> `lazyspec list rfc --json`
- `lazyspec context <id>` -> `lazyspec context <id> --json`
- `lazyspec show <id>` -> `lazyspec show <id> --json`
- `lazyspec validate` -> `lazyspec validate --json`
- `lazyspec search "<query>"` -> `lazyspec search "<query>" --json`

Do NOT add `--json` to mutation commands (`create`, `link`, `unlink`, `update`, `delete`) or to `lazyspec help`.

**How to verify:**
- Grep all skill files for lazyspec commands without `--json` that should have it
- The only commands without `--json` should be: `create`, `link`, `unlink`, `update`, `delete`, `help`, and cases where output is explicitly for user presentation

### Task 3: Add error recovery guidance

**Files:**
- Modify: `.claude/skills/plan-work/SKILL.md`
- Modify: `.claude/skills/write-rfc/SKILL.md`
- Modify: `.claude/skills/create-story/SKILL.md`
- Modify: `.claude/skills/create-iteration/SKILL.md`
- Modify: `.claude/skills/resolve-context/SKILL.md`
- Modify: `.claude/skills/build/SKILL.md`
- Modify: `.claude/skills/review-iteration/SKILL.md`

**What to implement:**

Add an error recovery rule to the `## CLI Reference` section (from Task 1):

```markdown
If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying. Do not guess at fixes or retry the same
command blindly.
```

**How to verify:**
- Every skill file contains the error recovery guidance within `## CLI Reference`

### Task 4: Replace hardcoded command signatures with help-first pattern

**Files:**
- Modify: `.claude/skills/write-rfc/SKILL.md`
- Modify: `.claude/skills/create-story/SKILL.md`
- Modify: `.claude/skills/create-iteration/SKILL.md`
- Modify: `.claude/skills/build/SKILL.md`
- Modify: `.claude/skills/review-iteration/SKILL.md`

**What to implement:**

For each step that tells the agent to run a specific command with specific flags, add a help-first fallback. Keep the example (agents benefit from seeing the expected invocation) but wrap it with a discovery pattern:

Before (example from create-story):
```markdown
2. **Create the story:** Run `lazyspec create story "<title>" --author <name>`
```

After:
```markdown
2. **Create the story:** Run `lazyspec help create` to confirm usage, then: `lazyspec create story "<title>" --author <name>`
```

Apply this pattern to these specific command sites:
- `create-story/SKILL.md` step 2: `lazyspec create story`
- `create-iteration/SKILL.md` step 3: `lazyspec create iteration`
- `write-rfc/SKILL.md` step 2: `lazyspec create rfc`
- `write-rfc/SKILL.md` step 6: `lazyspec create adr`
- `build/SKILL.md` step 12 (status updates): `lazyspec update`
- `review-iteration/SKILL.md` status updates: `lazyspec update`

Do NOT apply this to read-only commands (`show`, `list`, `search`, `status`, `context`, `validate`) since those are low-risk and already covered by the `## CLI Reference` preamble.

**How to verify:**
- Each mutation command example is preceded by a `lazyspec help <subcommand>` hint
- Read-only commands are unchanged (no unnecessary help calls)

## Test Plan

These are documentation-only changes, so testing is manual verification:

1. **Completeness check:** Every `.claude/skills/*/SKILL.md` file contains the `## CLI Reference` section with all three elements (discovery, --json default, error recovery)
2. **Consistency check:** The `## CLI Reference` section is identical across all 7 skills
3. **--json audit:** Grep for `lazyspec (show|list|search|status|context|validate)` without `--json` -- should find zero matches (except in the CLI Reference block's general guidance and user-presentation contexts)
4. **Help-first audit:** Every `lazyspec create`, `lazyspec update`, `lazyspec link` example is preceded by a help hint
5. **No regressions:** `lazyspec validate --json` still passes after changes

Tradeoff: these are manual/grep-based checks rather than automated tests. Appropriate for documentation changes where the "test" is structural consistency, not runtime behavior.

## Notes

This is a standalone iteration not linked to any Story. The work improves agent ergonomics when using the lazyspec CLI via skills.

Design decision: keep existing command examples alongside the help-first pattern rather than removing them entirely. Examples give agents a fast path; help gives them a fallback when examples drift. This is a middle ground between pure discovery and pure hardcoding.
