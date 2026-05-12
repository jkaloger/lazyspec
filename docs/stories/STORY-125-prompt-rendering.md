---
title: Prompt rendering
type: story
status: draft
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
---

## In Scope

This story delivers prompt rendering for the foreground-blocking daemon orchestrator introduced by RFC-041. The daemon must turn a normalized document plus session-local state into a fully-rendered string suitable for handing to a headless agent process. The rendering pipeline is responsible for the loading, validation, and lifecycle of role-specific prompt templates, and for deriving the dynamic inputs that those templates depend on.

The v1 surface ships a single role, `builder`. The builder prompt is a markdown document stored at `.lazyspec/prompts/builder.md`. The body of the file is treated as a minijinja template. Rendering happens in strict-undefined mode so that any unknown variable reference is treated as a configuration error rather than silently rendering empty.

Three render variables are exposed to the template:

- `doc` — the full normalized document (id, title, body, status, assignees, context_chain) that the daemon is currently engaging on.
- `attempt` — null on the first turn of a session, an integer on every subsequent continuation turn.
- `prior_iterations` — the list of iteration ids that have been created within the current session against the story under engagement.

`prior_iterations` is derived by store-diff, not by inspecting the agent's tool-call stream. At session start the daemon records the snapshot of iteration ids whose front-matter `implements` field points at the story under engagement. On each subsequent turn the daemon re-queries the store and reports the set delta. The store is the source of truth.

Snapshots must survive daemon restart. The daemon must be able to reconstruct the session-start snapshot from the metadata ref attached to the session, so that after a crash or planned restart the delta computation continues to produce correct results.

Prompt templates support hot-reload. A filesystem `notify` event on `.lazyspec/prompts/builder.md` triggers a preflight re-render. Preflight failure invalidates new dispatches — until the template renders cleanly, no new sessions may be started. In-flight sessions continue running against the template they were dispatched with; they are not interrupted by template changes. Daemon start also runs preflight, rendering the template against a stub document to surface unknown-variable errors before any real document is touched.

## Out of Scope

- The tick loop that decides when to call the renderer (slice 4).
- The AgentRunner subprocess and worktree wiring that consumes the rendered prompt (slice 3).
- The instructive content of the shipped builder prompt itself. The prompt body is a deliverable artifact, not a code artifact, and is not under acceptance-criteria scope here.
- IPC events emitted around dispatch and turn boundaries (slice 6).
- The metadata ref schema. This story reads from the document store; it does not define the ref shape used by slice 8.

## Acceptance Criteria

**AC1: builder role loads from the documented path**

Given a repo with a valid `.lazyspec/prompts/builder.md`
When the daemon starts and resolves the builder prompt template
Then the template body is loaded from `.lazyspec/prompts/builder.md`

**AC2: rendering exposes doc, attempt, prior_iterations**

Given a builder template that references `doc`, `attempt`, and `prior_iterations`
When the daemon renders the prompt for a session turn
Then minijinja substitutes the normalized document, the attempt counter, and the prior iteration ids into the output

**AC3: unknown variables fail at config load, not at dispatch**

Given a builder template that references a variable not provided by the renderer
When the daemon runs preflight on startup
Then preflight fails with a configuration error before any session can be dispatched

**AC4: attempt distinguishes first turn from continuation**

Given a session that has just begun
When the renderer produces the prompt for the first turn
Then `attempt` is null
And on every subsequent turn within the same session `attempt` is an integer that increments per turn

**AC5: prior_iterations reflects store-diff against the session-start snapshot**

Given a session-start snapshot recording iteration ids that implemented the story at session start
When an iteration is created during the session that implements the same story
Then a subsequent render exposes that new iteration id in `prior_iterations`
And iterations present at session start are excluded from `prior_iterations`

**AC6: snapshot survives daemon restart**

Given a session whose metadata ref records the session-start snapshot
When the daemon is restarted mid-session
Then the next turn in that session computes `prior_iterations` against the reconstructed snapshot
And the delta matches what would have been produced without the restart

**AC7: notify event triggers preflight; failure invalidates new dispatches**

Given a running daemon with a passing preflight
When `.lazyspec/prompts/builder.md` is modified and a notify event fires
Then preflight re-runs against the new template
And if preflight fails, no new session may be dispatched until preflight passes again

**AC8: in-flight sessions are not restarted on template change**

Given a session currently in flight against template version A
When the prompt file is changed and notify triggers a re-render
Then the in-flight session continues running against version A
And only sessions dispatched after the change use the new template

## Notes

`prior_iterations` is computed against the document store rather than the agent's tool-call log because the store is the durable source of truth and the only thing that survives crash. The metadata ref is the persistence layer that makes session-start snapshots reconstructable, so this story is coupled to slice 8 at the data-shape boundary but does not own the ref schema.

Hot-reload semantics are intentionally asymmetric: new dispatches see new templates, in-flight sessions see the template they started with. This avoids mutating a running agent's context mid-turn while still letting operators iterate on prompts without restarting the daemon.

Strict-undefined is chosen deliberately. A silently-empty variable in a builder prompt would degrade agent behavior without surfacing the cause; failing at preflight makes prompt authoring errors loud and early.

