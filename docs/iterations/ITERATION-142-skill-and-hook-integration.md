---
title: Skill and Hook Integration
type: iteration
status: accepted
author: agent
date: 2026-03-27
tags: []
related:
- implements: docs/stories/STORY-096-skill-and-hook-integration.md
---



## Changes

### Task 1: Add user-prompt-submit hook configuration

ACs addressed: AC-1 (hook fires lazyspec convention --preamble), AC-2 (empty output when no convention exists), AC-8 (hook entry in settings.json)

Files:
- Modify: `.claude/settings.local.json`

Add a `hooks` key to the existing settings file:

```json
"hooks": {
  "user-prompt-submit": [
    {
      "command": "lazyspec convention --preamble",
      "type": "intercept"
    }
  ]
}
```

The `lazyspec convention --preamble` command already exits cleanly with empty output when no convention document exists (verified in ITERATION-119), so AC-2 is satisfied by the existing CLI behavior. No code changes needed for graceful degradation on the hook side.

Verify: inspect `.claude/settings.local.json` for the hook entry. Run `lazyspec convention --preamble` in a project without a convention doc and confirm it exits 0 with empty output.

### Task 2: Add dictum preflight to /build skill

ACs addressed: AC-4 (build skill calls convention --tags)

Files:
- Modify: `.claude/skills/build/SKILL.md`

Insert a new step 0 in the Preflight section, before the existing chain resolution:

```
0. Load convention context: `lazyspec convention --tags build,testing,architecture --json`
   If the command returns non-empty dicta, include them in the context provided to each subagent prompt.
```

Also update the subagent prompt template in the Subagent Dispatch section to include a `## Convention Context` block that receives the dicta output (or is omitted when empty).

Verify: read the modified skill file and confirm the preflight step is present and references `lazyspec convention --tags`.

### Task 3: Add dictum preflight to /write-rfc skill

ACs addressed: AC-5 (write-rfc skill calls convention --tags rfc)

Files:
- Modify: `.claude/skills/write-rfc/SKILL.md`

Insert a new step 0 in the Preflight section:

```
0. Load convention context: `lazyspec convention --tags rfc,architecture --json`
   If the command returns non-empty dicta, include them when writing the RFC intent and interface sketches.
```

Verify: read the modified skill file and confirm the preflight step is present.

### Task 4: Add dictum preflight to /create-iteration skill

ACs addressed: AC-6 (create-iteration skill calls convention --tags iteration)

Files:
- Modify: `.claude/skills/create-iteration/SKILL.md`

Insert a new step 0 in the Preflight section:

```
0. Load convention context: `lazyspec convention --tags iteration,testing --json`
   If the command returns non-empty dicta, include them when planning the task breakdown and test plan.
```

Also update the subagent prompt template to pass convention context to iteration-creating subagents.

Verify: read the modified skill file and confirm the preflight step is present.

### Task 5: Graceful no-op in all skills

ACs addressed: AC-3 (empty JSON when no convention), AC-7 (skill proceeds without injecting context)

Files:
- Modify: `.claude/skills/build/SKILL.md`
- Modify: `.claude/skills/write-rfc/SKILL.md`
- Modify: `.claude/skills/create-iteration/SKILL.md`

Each preflight step added in Tasks 2-4 must include a guard: "If the command returns an empty JSON result (no convention or no matching dicta), proceed without injecting any convention context." This is documentation-only -- the skills are markdown instructions, not executable code.

The underlying CLI already returns `{"convention": null, "dicta": []}` when no convention exists (ITERATION-119), so the guard is about instructing the agent to skip the convention context block rather than error.

Verify: confirm each skill's preflight step includes the empty-result guard clause.

## Test Plan

STORY-096 is a pure configuration and documentation change (hook config + skill markdown). There is no Rust code to test. Verification is manual:

### verify: hook configuration is valid JSON

Read `.claude/settings.local.json` and confirm it parses as valid JSON with the `hooks.user-prompt-submit` entry referencing `lazyspec convention --preamble` with type `intercept`.

### verify: CLI graceful degradation

Run `lazyspec convention --preamble` and `lazyspec convention --tags nonexistent --json` in a project without convention docs. Confirm both exit 0 with empty/null output. This validates AC-2, AC-3, AC-7 at the CLI level (already covered by ITERATION-119 tests, but worth confirming).

### verify: skill preflight steps present

Read each modified skill file and confirm the convention preflight step exists in the Preflight section with the correct `--tags` flags and the empty-result guard.

## Notes

This iteration is entirely configuration and documentation. No Rust code changes. The `lazyspec convention` CLI subcommand (ITERATION-119) already handles all the edge cases (no convention, no matching tags, empty output). This iteration wires it into the skill and hook system.

The tag selections for each skill (`build,testing,architecture` vs `rfc,architecture` vs `iteration,testing`) are initial defaults. Users will tag their dicta to match; the tags just need to be reasonable starting points that a user can customize in their convention.
