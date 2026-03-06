---
name: create-story
description: Use when starting a new feature, card, or vertical slice of work. Creates a Story document with given/when/then acceptance criteria linked to an RFC.
---

```
NO WORK WITHOUT ACCEPTANCE CRITERIA
```

If you can't state given/when/then, you don't understand the work yet.

<HARD-GATE>
Do NOT create a Story without a parent RFC. If no RFC exists for this work,
use the `/write-rfc` skill first.
After completion: use the `/create-iteration` skill to plan the first iteration.
You already have the RFC and Story context from writing this Story, so
resolve-context is not needed when continuing in the same session.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT write acceptance criteria without reading the parent RFC first.
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

# Create Story

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build

create-story.style.fill: "#4A9EFF"
create-story.style.font-color: "#FFFFFF"
plan.style.opacity: 0.4
write-rfc.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Find parent RFC -> RFC exists?

RFC exists?.shape: diamond
RFC exists? -> Create story: yes
RFC exists? -> Use /write-rfc skill: no

Use /write-rfc skill.shape: hexagon

Create story -> Write acceptance criteria -> Link to RFC -> Define scope -> Validate -> Use /create-iteration skill

Use /create-iteration skill.shape: double_circle
```

## Preflight

1. Read relevant documents using `lazyspec show --json` before modifying anything
2. Check for existing artifacts using `lazyspec search --json` and `lazyspec list --json`
3. Read the parent RFC with `lazyspec show <rfc-id> --json`
4. Confirm you understand the design intent before writing ACs
5. Check for existing stories under this RFC: `lazyspec search "<rfc-title>" --json`

## Steps

1. **Find the parent RFC:** Run `lazyspec list rfc --json` to find the relevant RFC. Use `lazyspec show <id> --json` to verify it's the right one. If no RFC exists, use the `/write-rfc` skill first.

2. **Create the story:** Run `lazyspec help create` to confirm usage, then: `lazyspec create story "<title>" --author <name>`

3. **Write acceptance criteria:** Edit the created file. Each AC must follow given/when/then:
   - **Given** a precondition that establishes context
   - **When** an action is taken
   - **Then** an observable outcome occurs

4. **Link to RFC:** Run `lazyspec help link` to confirm usage, then: `lazyspec link <story-path> implements <rfc-path>`

5. **Define scope:** Fill in the In Scope and Out of Scope sections. Be explicit about what this story does NOT cover.

6. **Validate:** Run `lazyspec validate --json` to ensure all links resolve.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "The RFC covers it, I don't need a Story" | RFCs describe intent. Stories describe acceptance. Different audiences. |
| "I'll write the ACs after I see what the code looks like" | That's testing after. ACs define the work, not the other way around. |
| "This AC is obvious, I don't need given/when/then" | Obvious to you. Given/when/then forces you to be explicit about preconditions. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes (story links to RFC)
- [ ] Every AC has given/when/then structure
- [ ] Scope sections are filled (not TODO)
- [ ] Story is readable without implementation specifics

## Rules

- A Story must be readable by a client without referencing implementation details
- If you can't write the Story without mentioning implementation specifics, it's scoped wrong
- Each AC must be independently testable
- Keep stories small enough to complete in 1-3 iterations
