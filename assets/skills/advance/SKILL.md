---
name: advance
description: Use when moving a document to its next status along the type's lifecycle DAG, maintaining links and checking gates at the transition.
---

```
MOVE ONE STATUS ALONG THE LIFECYCLE EDGE
```

Advance computes the next status from the type's lifecycle edges, checks the gates that guard it, and applies the move -- one document, one edge.

<HARD-GATE>
Do NOT propose a status that is not an outgoing `lifecycle.edges` edge from the current status. Derive the next status from config; the binary rejects off-edge transitions.
Advance moves status only. It NEVER spawns a child document, even when the move clears a gate that makes a child creatable.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` -- read the type's `lifecycle` (its `states` and `edges`). The edge set is the source of truth for what moves are legal; no status name is baked here.
2. `lazyspec show <id> --json` -- read the document's current status.
3. `lazyspec context --json` -- read parent/child state relevant to any gate that guards the transition.

## Workflow

1. **Compute the eligible next status:** from `lifecycle.edges`, find edges whose `from` matches the document's current status. The set of `to` values is the candidate next statuses. (Edges with `from: "*"` apply from any status -- e.g. the default config's `* -> superseded`.) Derive the move from the edge set; do not assume status names.
2. **Check gates:** if config attaches a gate to the target status -- such as a `require_parent_status` rule -- confirm the parent's status satisfies it (read parent status from `context --json`). If the gate is unmet, do not advance; report what status the parent must reach first.
3. **Apply:** `lazyspec update <id> --status <next>`. The binary rejects any off-edge transition, so propose only declared edges.
4. **Maintain relations:** keep the configured relations intact across the transition.

## Gates and the Type Boundary

Advance is where status-conditioned gates matter. When a transition would make a *child* of a different type creatable (a `require_parent_status` gate clears), advance does the status move **only** and **stops**. It never spawns the child.

Crossing into a child type is a human-initiated step, handled by /lazy's stop-at-boundary rule. Gate-clear makes the child *eligible*, not *automatic*.

## Rules

- Next status is always derived from `lifecycle.edges` in config. No status name is load-bearing in this prose.
- Apply via `lazyspec update <id> --status <next>`; never bypass the binary's edge enforcement.
- Check config gates before moving; do not advance through an unmet gate.
- Never spawn a child document. Status only.
- Read lifecycle/gate facts from config, never from `.lazyspec/` graph files directly.
