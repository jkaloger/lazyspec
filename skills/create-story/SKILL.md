---
name: create-story
description: Use when starting a new feature, card, or vertical slice of work. Creates Story documents with given/when/then acceptance criteria linked to an RFC. Supports parallel subagent dispatch for RFCs with multiple vertical slices.
---

```
NO WORK WITHOUT ACCEPTANCE CRITERIA
```

If you can't state given/when/then, you don't understand the work yet.

<HARD-GATE>
Do NOT create a Story without a parent RFC. If no RFC exists, use `/write-rfc` first.
After identifying multiple slices, partition upfront and get user approval before dispatching subagents.
After completion: use `/create-iteration` to plan the first iteration.
You already have RFC and Story context from this session, so resolve-context is not needed when continuing.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration.
- Do NOT write acceptance criteria without reading the parent RFC first.
- Do NOT dispatch subagents without user approval of the partition.
</NEVER>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

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

1. Read relevant documents with `lazyspec show --json` before modifying anything
2. Check for existing artifacts: `lazyspec search --json`, `lazyspec list --json`
3. Read parent RFC: `lazyspec show <rfc-id> --json`
4. Check for existing stories under this RFC: `lazyspec search "<rfc-title>" --json`

## Multi-slice RFCs

1. Extract vertical slices from RFC. For each: define title, in-scope, out-of-scope, RFC sections addressed.
2. Verify non-overlapping scope.
3. Present partition table to user. Wait for explicit approval.
4. Dispatch one subagent (general-purpose, Opus) per story in parallel. Each receives: full RFC body, its slice definition, other slices' boundaries, and instructions to use `lazyspec create story`, write given/when/then ACs, `lazyspec link`, and `lazyspec validate --json`.
5. Collect results. Run `lazyspec validate --json`. Present all stories to user.

## Single-slice RFCs

1. `lazyspec create story "<title>" --author <name>`
2. Edit file: write ACs in given/when/then format (Given precondition, When action, Then observable outcome)
3. `lazyspec link <story-path> implements <rfc-path>`
4. Fill In Scope and Out of Scope sections explicitly.
5. `lazyspec validate --json`

## Rules

- A Story must be readable without implementation specifics
- Each AC must be independently testable
- Each AC must have given/when/then structure
- Keep stories small enough to complete in 1-3 iterations
- For multi-slice RFCs, always get user approval of the partition before dispatching
- Subagents receive full RFC text, not file references
