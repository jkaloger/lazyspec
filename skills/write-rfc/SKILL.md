---
name: write-rfc
description: Use when proposing a design or significant change. Creates an RFC document with intent, interface sketches, and identifies the Stories that fall out of it.
---

```
NO STORIES WITHOUT DESIGN INTENT
```

If you're about to create a Story without an RFC, stop. Write the RFC first.

<HARD-GATE>
Do NOT create Stories until this RFC is written and the user has approved it.
After completion: use `/create-story` for each vertical slice identified.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration.
- Do NOT create Story documents from this skill. Finish the RFC, get approval, then use `/create-story`.
</NEVER>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Workflow

```d2
Understand the problem -> Create RFC -> Write intent and context -> Sketch interfaces -> Identify stories -> Validate -> User approves?

User approves?.shape: diamond
User approves? -> Use /create-story skill: yes
User approves? -> Revise RFC: no
Revise RFC -> Write intent and context

Use /create-story skill.shape: double_circle
```

## Preflight

0. Load convention context: `lazyspec convention --tags rfc,architecture --json`. Use non-empty results to inform design.
1. Search for existing RFCs: `lazyspec search "<topic>" --json`, `lazyspec list rfc --json`
2. Read related RFCs: `lazyspec show <id> --json`
3. Confirm no existing RFC covers this design

## Steps

1. **Create:** `lazyspec create rfc "<title>" --author <name>`
2. **Write intent:** Describe the problem and why. Design intent, not implementation detail.
3. **Sketch interfaces:** `@draft` for proposed types, `@ref path#Symbol` for existing types, `@ref path#Symbol @sha abc1234` to pin to a commit. Preview with `lazyspec show -e <id>`.
4. **Identify Stories:** List vertical slices, each independently shippable.
5. **Emit ADRs:** For significant decisions: `lazyspec create adr "<decision>"`, `lazyspec link <adr-path> related-to <rfc-path>`.
6. **Validate:** `lazyspec validate --json`

## Verification

- [ ] `lazyspec validate --json` passes
- [ ] User has explicitly approved the RFC
- [ ] At least one Story identified
- [ ] Significant decisions have ADRs

## Rules

- RFCs describe intent, not implementation
- An RFC captures thinking at the time of writing
- Every RFC should identify at least one Story
