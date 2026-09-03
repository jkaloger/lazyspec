---
name: advance
description: Use when moving a document to its next status along the type's lifecycle DAG, maintaining links across the transition.
---

```
TRAVERSE ONE OUT-EDGE OF THE LIFECYCLE GRAPH
```

A type's lifecycle is a directed graph: the nodes are its statuses, the edges are the transitions config permits. A document sits on one status. Advance reads the out-edges from that status, picks the successor, and writes the move. The edge set is what gates a status move -- `update --status` refuses a pair the lifecycle does not name -- and that refusal lives inside one type's lifecycle. No status a document reaches permits or refuses anything in another type. One document, one edge.

## The command

Advance is a skill, not a subcommand. The move is written by `update`:

```
lazyspec update <id> --status <next>
```

`lazyspec advance` does not exist. `lazyspec help` lists every subcommand there is.

<HARD-GATE>
Propose only a successor: a status the current one has an out-edge to in `lifecycle.edges`. Read the edge set from config. The binary rejects any pair that is not an edge.
Advance writes status only. It never creates a child document. No status makes a child creatable and none withholds one either; crossing into another type is human-initiated, per /lazy's stop-at-boundary rule.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"`.
- To modify after creation: `lazyspec update <ID> --body "new content"`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read lifecycle facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` gives the type's `lifecycle`: its `states` (the nodes) and `edges` (the transitions). The edge set decides which moves exist. Every status name comes from config; this skill names none.
2. `lazyspec show <id> --json` gives the document's current status and its `related` links.

## Workflow

1. Find the successors. Keep the edges in `lifecycle.edges` whose `from` is the current status; their `to` values are the statuses you can move to. An edge with `from: "*"` applies from every status, so the default config's `* -> superseded` is always available.
2. Write the move. `lazyspec update <id> --status <next>`. The binary rejects any pair that is not an edge, so offer only successors.
3. Preserve the links across the move.
