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
After completion: use the `/create-story` skill for each vertical slice identified.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT create Story documents from this skill. Finish the RFC, get approval, then use the `/create-story` skill.
</NEVER>

## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.

Always pass `--json` when the command supports it. This gives you structured,
parseable output. Only omit `--json` when presenting output directly to the user.

If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying. Do not guess at fixes or retry the same
command blindly.

# Write RFC

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build

write-rfc.style.fill: "#4A9EFF"
write-rfc.style.font-color: "#FFFFFF"
plan.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
build.style.opacity: 0.4
```

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

1. Read relevant documents using `lazyspec show --json` before modifying anything
2. Check for existing artifacts using `lazyspec search --json` and `lazyspec list --json`
3. Search for existing RFCs on the topic: `lazyspec search "<topic>" --json`, `lazyspec list rfc --json`
4. Read any related RFCs with `lazyspec show <id> --json`
5. Confirm no existing RFC already covers this design

## Steps

1. **Understand the problem:** Search existing docs with `lazyspec search <topic> --json` to avoid duplicating prior work. Check for superseded RFCs.

2. **Create the RFC:** Run `lazyspec help create` to confirm usage, then: `lazyspec create rfc "<title>" --author <name>`

3. **Write intent:** Describe the problem being solved and why. This is design intent, not implementation detail.

4. **Sketch interfaces:** Use `@draft` syntax for types that don't exist yet:
   ```
   @draft UserProfile { id: string; email: string }
   ```
   Use `@ref` for types that already exist in the codebase:
   ```
   @ref src/types/user.ts#UserProfile
   ```
   Pin a reference to a specific commit with `@sha`:
   ```
   @ref src/types/user.ts#UserProfile @sha abc1234
   ```
   This ensures the reference resolves to that exact version, even if the file changes later.

   To preview how references expand, run `lazyspec show -e <id>`.

   **`@ref` vs `@draft`:** Use `@ref` when the type already exists in the codebase. Use `@draft` when you're proposing a type that doesn't exist yet. If you're unsure whether something exists, search with `lazyspec search` first.

5. **Identify Stories:** List the vertical slices that fall out of this RFC. Each should be independently shippable.

6. **Emit ADRs:** For significant decisions made during RFC writing, run `lazyspec help create` to confirm usage, then: `lazyspec create adr "<decision>"`. Run `lazyspec help link` to confirm usage, then: `lazyspec link <adr-path> related-to <rfc-path>`.

7. **Validate:** Run `lazyspec validate --json`.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I'll just start coding and document later" | Documentation after = rationalisation. Write the RFC. |
| "This is too small for an RFC" | Small changes with unexamined assumptions cause the most rework. |
| "I already know the design" | If it's not written down, it doesn't exist. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes
- [ ] User has explicitly approved the RFC
- [ ] At least one Story has been identified
- [ ] Any significant decisions have ADRs

## Rules

- RFCs describe intent, not implementation
- An RFC is a design record -- it captures thinking at the time of writing
- Sketch interfaces in prose or TypeScript, not as live code
- Every RFC should identify at least one Story
