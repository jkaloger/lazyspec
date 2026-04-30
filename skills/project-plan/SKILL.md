---
name: project-plan
description: Use when starting a project from a SOW or brief that spans multiple capabilities. Orchestrates project-level planning above /plan-work by decomposing the brief into RFCs with MoSCoW priority and cross-RFC blocks edges.
---

```
NO RFCS WITHOUT A PROJECT MAP
```

This skill sits ABOVE `/plan-work`. Project-level scope only. SOW / brief in, set of RFCs out. Each RFC then re-enters `/plan-work` at the RFC level.

<HARD-GATE>
Do NOT dispatch `/write-rfc` subagents until the user has approved the capability list AND the priority distribution (must / should / could). Present both. Wait for explicit approval. Revise on request.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT estimate effort, story points, or dates. Out of scope. PM tooling concern.
- Do NOT capture discovery notes, research, or interview output. Out of scope.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. RFC-041) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Workflow

```d2
SOW / brief input -> Extract capabilities -> Distribute MoSCoW priority -> Identify cross-RFC dependencies -> Present to user -> User approves?

User approves?.shape: diamond
User approves? -> Dispatch /write-rfc subagents (parallel): yes
User approves? -> Revise capabilities or priorities: no
Revise capabilities or priorities -> Extract capabilities

Dispatch /write-rfc subagents (parallel) -> Collect RFCs -> Wire cross-RFC blocks edges -> Emit project summary

Emit project summary.shape: double_circle
```

Downstream: each RFC re-enters `/plan-work` at the RFC level. Stories come from `/create-story`. Iterations come from `/create-iteration` (incl. sweep mode). This skill produces RFCs and only RFCs.

## Preflight

```
lazyspec status --json
lazyspec list rfc --json
lazyspec search "<capability keyword>" --json
```

Search per capability keyword. Avoid duplication of existing RFCs. If an existing RFC already covers a capability, link to it instead of creating a new one.

## Steps

1. **Capture project name + SOW / brief input.** Read the brief end-to-end. Confirm project name with the user.
2. **Extract capabilities.** One capability per major user-facing area. Each capability = future RFC title. Capability boundary = independently designable surface.
3. **Distribute MoSCoW priority** across capabilities (per RFC-041). `must` = NEED (contractual obligation in the SOW). `should` / `could` = WANT (budget headroom, nice-to-have). Every capability gets one tag.
4. **Identify cross-RFC dependencies.** Capability A depends on capability B's interface? That becomes a `blocks` edge: B `blocks` A (RFC-041 blocks DAG). Note these for the wiring step.
5. **Present capability list + priorities + dependencies to user.** Table format: capability, priority, depends-on. Wait for explicit approval. HARD-GATE: do not proceed without it. Revise on request.
6. **Dispatch one `/write-rfc` subagent per capability in parallel.** Each subagent receives:
   - Capability scope (the slice of the SOW it covers)
   - Priority tag (`must` / `should` / `could`)
   - Sibling capability boundaries (so it doesn't bleed scope)
   - Project context (project name, brief excerpt, link to other capabilities)
7. **Collect RFCs.** Wire cross-RFC `blocks` edges via `lazyspec link <blocker-rfc> blocks <blocked-rfc>` per dependency identified in step 4.
8. **Emit project summary.** Counts of must / should / could. Suggested sequencing (must-first, then should, then could; respecting `blocks` DAG). Total RFC count. Present to user.

## Out of Scope (explicit)

- **Estimation.** No effort, points, or dates. Use a PM tool.
- **Discovery.** No research notes, interview transcripts, or stakeholder maps.
- **Story partition.** Delegated to `/create-story` (multi-slice partition mode).
- **Iteration sweep.** Delegated to `/create-iteration` (sweep mode).

## Rules

- This skill produces RFCs only. Stories and Iterations come later via downstream skills.
- `must` is contractual NEED. `should` / `could` is WANT (budget headroom).
- Every capability becomes exactly one RFC (or links to an existing RFC).
- Cross-RFC dependencies are wired as `blocks` edges, not `relates-to`.
- After this skill: each RFC re-enters `/plan-work` at the RFC level for slice brainstorming.
