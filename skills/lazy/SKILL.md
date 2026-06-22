---
name: lazy
description: Use as the entry point for any lazyspec work. Reads the configured DAG and the user's position, then dispatches the right verb -- advancing within the current document automatically but stopping at type boundaries.
---

```
ADVANCE WITHIN A DOCUMENT, STOP AT THE BOUNDARY
```

Lazy is the entry router: it reads the configured DAG and where the user is, then dispatches the right verb -- progressing within the current document automatically, but never crossing a type boundary on its own.

<HARD-GATE>
Do NOT auto-run `create <child-type>` across a `parent_type` edge. Crossing into a different type is always human-initiated -- even when a `require_parent_status` gate is already satisfied. Within-document progression is automatic; crossing a type boundary is not.
Compute the dispatch table from `lazyspec config --json` at runtime. There is no fixed chain in this prose.
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

## Preflight (the routing read)

This is the resolve-context fold-in: `/lazy` reads context from the CLI rather than calling a separate skill.

1. `lazyspec config --json` -- the full DAG: `types` (with `intent`, `authorship`, `lifecycle`, `parent_type`), `relationships`, and `rules` (including any `require_parent_status` gates).
2. `lazyspec status --json` -- what documents exist and each one's current status.
3. `lazyspec context --json` -- the chain around the user's current document.

## Locate-in-DAG

From config + status + context, determine which document and type the user is on and where it sits in its lifecycle (current status, outgoing edges, gates).

## Dispatch (computed from config)

Build the dispatch table at runtime from config. No `parent_type` chain is hardcoded here. (The shipped default config happens to define a chain among types named `rfc`, `story`, and `iteration` -- treat that only as the shipped default, never as a routing assumption.)

**Within-document progression is automatic.** If the current document has an eligible outgoing `lifecycle` edge (the edge exists and its gate, if any, is met), dispatch the matching verb WITHOUT asking:

- a status move with no authoring/work needed -> /advance
- an authoring step appropriate to the type's `authorship` and current status -> the authoring verb at the type's ceiling (/scaffold, /co-write, or /generate)
- work described by the document -> /execute
- a critique step before the next status -> /review

**Authorship-aware dispatch.** When routing to an authoring action, pick the verb at or below the type's `authorship` ceiling. Default to the ceiling verb (`human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate) and allow the human to drop lower. Never dispatch an above-ceiling verb.

## Stop-at-Type-Boundary

When the only remaining next step would create a child of a **different type** -- crossing a `parent_type` edge -- `/lazy` **STOPS.** It reports the boundary and what the human can do next; it never auto-runs `create <child-type>`.

This holds **even when a `require_parent_status` gate is already satisfied.** Gate-clear makes the child *eligible*, not *automatic*. Crossing a type boundary is always human-initiated. Report it like:

> `<doc>` (type `<type>`) is at status `<status>`; its child type `<child-type>` is now eligible to create. Crossing types is human-initiated -- run /scaffold (or the ceiling verb for `<child-type>`) to start one.

with every value read from config + status for that run.

## Rules

- All routing reads from `config --json` / `status --json` / `context --json` at runtime. No type name and no chain are load-bearing in this prose.
- Within-document progression is automatic; crossing a type boundary is never automatic.
- Dispatch only verbs at or below the type's authorship ceiling.
- Read DAG/gate/status facts from the CLI, never from `.lazyspec/` graph files directly.
