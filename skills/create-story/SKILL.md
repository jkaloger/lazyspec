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

Stories are user-facing slices only. Each must be expressible as G/W/T from a user's perspective. Dev-only work (interface stubs, infra scaffolding, cleanup, refactors) does NOT become a Story — defer to `/create-iteration` with `implements` linking to one or more Stories.

1. Extract user-facing slices from RFC. Filter out dev-only work (note it for later iteration sweep).
2. Generate **2-3 candidate partitions**, not one. Different axes: by user journey, by surface area, by data flow, by priority tier. Presenting one partition causes the user to anchor on it rather than evaluate trade-offs.
3. For each candidate: list Stories with title, in-scope, out-of-scope, RFC sections addressed, slice category, and RFC-041 priority (must / should / could).
4. Slice category tag per Story (pick one):
   - `route-stub` — user-reachable surface with placeholder content
   - `data-integration` — wires real data into an existing surface
   - `ui-presentation` — visual/interaction layer on existing data
   - `functional` — end-to-end user capability
   - `polish` — UX refinements on shipped capability
5. Present all candidate partitions side-by-side. User compares, picks one, or remixes (e.g. "partition B but split Story 2 like in partition A"). Wait for explicit approval.
6. Dispatch one subagent (general-purpose, Opus) per Story in parallel. Each receives: full RFC body, its slice definition, slice category, priority, other slices' boundaries, and instructions to use `lazyspec create story`, write given/when/then ACs, `lazyspec link`, and `lazyspec validate --json`.
7. Collect results. Run `lazyspec validate --json`. Present all Stories to user.
8. Hand off the dev-only work list to `/create-iteration` for the iteration sweep — those Iterations attach via `implements` to whichever Stories they support.

## Single-slice RFCs

1. `lazyspec create story "<title>" --author <name>`
2. Edit file: write ACs in given/when/then format (Given precondition, When action, Then observable outcome)
3. `lazyspec link <story-path> implements <rfc-path>`
4. Fill In Scope and Out of Scope sections explicitly.
5. `lazyspec validate --json`

## Rules

- A Story must be user-facing. If you can't write G/W/T from a user's POV, it's not a Story — it's an Iteration.
- A Story must be readable without implementation specifics
- Each AC must be independently testable
- Each AC must have given/when/then structure
- Each Story carries a slice category (route-stub / data-integration / ui-presentation / functional / polish) and an RFC-041 priority (must / should / could)
- Keep stories small enough to complete in 1-3 iterations
- For multi-slice RFCs, generate 2-3 candidate partitions and let the user compare/remix before dispatching
- Dev-only work (stubs, infra, cleanup) belongs in Iterations with `implements` to Stories — never create a Story for dev-only work
- Subagents receive full RFC text, not file references
