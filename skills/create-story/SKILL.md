---
name: create-story
description: Use when starting a new feature, card, or vertical slice of work. Creates Story documents with given/when/then acceptance criteria linked to an RFC. Supports parallel subagent dispatch for RFCs with multiple vertical slices.
---

```
NO WORK WITHOUT ACCEPTANCE CRITERIA
```

If you can't state given/when/then, you don't understand the work yet.

<HARD-GATE>
Do NOT create a Story without a parent RFC. If no RFC exists for this work,
use the `/write-rfc` skill first.
After identifying multiple slices, partition upfront and get user approval
before dispatching subagents.
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
- Do NOT dispatch subagents without user approval of the partition.
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
RFC exists? -> Read RFC and extract slices: yes
RFC exists? -> Use /write-rfc skill: no

Use /write-rfc skill.shape: hexagon

Read RFC and extract slices -> Multiple slices?

Multiple slices?.shape: diamond
Multiple slices? -> Define partitions -> User approves partitions?: yes
Multiple slices? -> Create single story (inline): no

User approves partitions?.shape: diamond
User approves partitions? -> Dispatch N subagents: yes
User approves partitions? -> Revise partitions: no
Revise partitions -> Define partitions

Dispatch N subagents -> Collect results -> Validate -> Present to user -> Use /create-iteration skill
Create single story (inline) -> Write ACs -> Link to RFC -> Validate -> Use /create-iteration skill

Use /create-iteration skill.shape: double_circle
```

## Preflight

1. Read relevant documents using `lazyspec show --json` before modifying anything
2. Check for existing artifacts using `lazyspec search --json` and `lazyspec list --json`
3. Read the parent RFC with `lazyspec show <rfc-id> --json`
4. Confirm you understand the design intent before writing ACs
5. Check for existing stories under this RFC: `lazyspec search "<rfc-title>" --json`

## Partitioning

Before dispatching subagents, the orchestrator must:

1. Read the RFC with `lazyspec show <rfc-id> --json`
2. Extract the identified vertical slices from the Stories section
3. For each slice, define: title, scope boundary (in/out), which RFC sections it addresses
4. Verify slices are non-overlapping (no shared scope)
5. Present the partition table to the user for approval

The partition table should clearly show each slice's title, what is in scope, what is out of scope, and which RFC sections it maps to. The user must approve or request revisions before any subagents are dispatched.

## Subagent Dispatch

| Tier   | Model  | Use for                                                                         |
| ------ | ------ | ------------------------------------------------------------------------------- |
| Light  | Haiku  | Parsing frontmatter, extracting structured data, simple validation              |
| Medium | Sonnet | Codebase exploration, searching for patterns, reading and summarizing documents |
| Heavy  | Opus   | Implementation, complex reasoning, multi-file changes, review                   |

| Operation    | Agent Type      | Tier  | Context to provide                                            |
| ------------ | --------------- | ----- | ------------------------------------------------------------- |
| Create story | general-purpose | Heavy | RFC context, slice definition, adjacent slice boundaries      |

Each subagent receives a prompt containing:
- The full RFC body (not a file reference)
- Its specific slice definition (title, in-scope, out-of-scope)
- The scope boundaries of all other slices (so it knows what to exclude)
- Instructions to: create the story with `lazyspec create story`, write given/when/then ACs, link to RFC with `lazyspec link`, define scope sections
- The standard lazyspec CLI reference block

Subagents are dispatched in parallel using the Agent tool.

### Subagent Prompt Template

```
IMPORTANT: You are working within the lazyspec workflow.
- Use `lazyspec` CLI commands for document operations. Do NOT write document files directly.
- Read files before editing them. Use the Read tool or `lazyspec show --json` before any modification.
- Implement ONLY what the task specifies. Do not add features, refactor surrounding code, or "improve" things not in the task.

You are creating a single Story document within the lazyspec workflow.

## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.

Always pass `--json` when the command supports it. This gives you structured,
parseable output. Only omit `--json` when presenting output directly to the user.

If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying. Do not guess at fixes or retry the same
command blindly.

## RFC Context
[Full RFC body]

## Your Slice
Title: [slice title]
In scope: [what this story covers]
Out of scope: [what this story does NOT cover]

## Other Slices (for boundary awareness)
[List of other slice titles and their scope]

## Instructions
1. Create the story: `lazyspec create story "<title>" --author <name>`
2. Edit the created file to write acceptance criteria using given/when/then format
3. Link to RFC: `lazyspec link <story-path> implements <rfc-path>`
4. Define In Scope and Out of Scope sections in the story body
5. Validate: `lazyspec validate --json`
6. Report: story path, ACs written, any concerns
```

## Steps

### Multi-slice RFCs

1. **Find the parent RFC:** Run `lazyspec list rfc --json` to find the relevant RFC. Use `lazyspec show <id> --json` to verify it's the right one. If no RFC exists, use the `/write-rfc` skill first.

2. **Read RFC and extract slices:** Read the full RFC body. Identify the vertical slices described in the Stories section or equivalent.

3. **Define partitions:** For each slice, define title, in-scope, out-of-scope, and which RFC sections it addresses. Verify slices are non-overlapping.

4. **Present partition to user for approval:** Show the partition table. Wait for explicit approval. If the user requests changes, revise and re-present.

5. **Dispatch N subagents in parallel:** One subagent per story, using the Agent tool with the prompt template above. Each receives the full RFC body, its slice definition, and the boundaries of all other slices.

6. **Collect results:** Gather reports from all subagents. Run `lazyspec validate --json` to verify all stories link correctly and pass validation.

7. **Present all created stories to the user:** Show a summary of each story created, its ACs, and the validation result.

### Single-slice RFCs (fallback)

When the RFC identifies only one vertical slice, create the story directly without subagent dispatch:

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
| "I'll create all stories myself" | Subagents prevent context pollution. One agent per story. |
| "I don't need user approval for the partition" | Always get approval. The user knows their domain better than you. |

## Verification

Before claiming this skill is complete:

- [ ] All created stories link to the parent RFC
- [ ] No overlapping scope between stories
- [ ] Every AC has given/when/then structure
- [ ] Scope sections are filled (not TODO)
- [ ] Story is readable without implementation specifics
- [ ] `lazyspec validate --json` passes

## Rules

- A Story must be readable by a client without referencing implementation details
- If you can't write the Story without mentioning implementation specifics, it's scoped wrong
- Each AC must be independently testable
- Keep stories small enough to complete in 1-3 iterations
- For multi-slice RFCs, dispatch one subagent per story
- Always get user approval of the partition before dispatching
- Each subagent receives full RFC text, not file references

## Guardrails

Before dispatching subagents, verify:

- [ ] Have you read the RFC? (not assumed -- actually read with `lazyspec show --json`)
- [ ] Are the slice boundaries non-overlapping?
- [ ] Has the user approved the partition?
- [ ] Is each subagent receiving full RFC text (not a file reference)?

If any answer is "no", stop. Complete the missing step.
