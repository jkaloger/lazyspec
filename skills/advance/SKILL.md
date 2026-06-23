---
name: advance
description: Use when moving a document to its next status along the type's lifecycle DAG, maintaining links and checking gates at the transition.
---

```
TRAVERSE ONE OUT-EDGE OF THE LIFECYCLE GRAPH
```

A type's lifecycle is a directed graph: the nodes are its statuses, the edges are the transitions config permits. A document sits on one status. Advance reads the out-edges from that status, picks the successor, confirms the gate on that edge holds, and writes the move. One document, one edge.

<HARD-GATE>
Propose only a successor: a status the current one has an out-edge to in `lifecycle.edges`. Read the edge set from config. The binary rejects any pair that is not an edge.
Advance writes status only. It never creates a child document, even when the move satisfies a gate that makes a child creatable.
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

1. `lazyspec config --json` gives the type's `lifecycle`: its `states` (the nodes) and `edges` (the transitions). The edge set decides which moves exist. Every status name comes from config; this skill names none.
2. `lazyspec show <id> --json` gives the document's current status.
3. `lazyspec context --json` gives the parent and child statuses a gate may depend on.

## Workflow

1. Find the successors. Keep the edges in `lifecycle.edges` whose `from` is the current status; their `to` values are the statuses you can move to. An edge with `from: "*"` applies from every status, so the default config's `* -> superseded` is always available.
2. Test the gate. A gate is a predicate on the target status, such as `require_parent_status`. Read the parent's status from `context --json` and check the predicate. If it fails, stop and report which status the parent must reach first.
3. Write the move. `lazyspec update <id> --status <next>`. The binary rejects any pair that is not an edge, so offer only successors.
4. Preserve the links across the move.

## Gates and the type boundary

A gate can make a child of another type creatable once the parent reaches a status. When that happens, advance writes the status move and stops. It does not create the child.

Two conditions separate a move within the lifecycle from crossing into a child type. The gate makes the child creatable; starting it is a second, human step, handled by /lazy's stop-at-boundary rule. Satisfying the gate is necessary, not sufficient.

## Rules

- The successor comes from `lifecycle.edges`. This skill names no status; config does.
- Write the move with `lazyspec update <id> --status <next>`; never go around the binary's edge check.
- Test the gate before moving; do not cross an unsatisfied gate.
- Never create a child document. Status only.
- Read lifecycle and gate facts from config, not from the `.lazyspec/` graph files.
