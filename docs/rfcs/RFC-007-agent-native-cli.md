---
title: Agent-Native CLI
type: rfc
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- cli
- agents
- json
related:
- related-to: RFC-002
- related-to: STORY-002
---



## Summary

Make lazyspec's CLI the complete interface for agent-driven workflows. Today, agents depend on Claude Code skills for context resolution and review. This couples lazyspec's agent capability to a single runtime. The CLI should provide everything an agent needs directly, making skills a convenience layer rather than a requirement.

## Problem

RFC-002 introduced agent workflow skills: `resolve-context`, `review-iteration`, `create-iteration`, etc. These work well with Claude Code but have three problems:

1. **Runtime coupling.** An agent using a different framework (Cursor, Aider, custom tooling) can't use the skills. The CLI is the universal interface, but it doesn't expose enough for agents to work autonomously.

2. **No single-call project state.** An agent starting work must run multiple commands (`list rfc`, `list story`, `list iteration`) and stitch results together to understand the project. There's no way to get the full picture in one call.

3. **Inconsistent machine output.** `list`, `search`, and `validate` support `--json`. `show` does not. An agent parsing CLI output has to handle both structured and unstructured formats.

4. **Validation gaps.** `validate` checks broken links and structural rules but doesn't catch semantic inconsistencies like accepted work under a superseded parent.

## Design Intent

### `context <id>`

Walks the `implements` relationship chain upward from any document and outputs the full chain.

```
$ lazyspec context ITER-001
```

Starting from the given document, follow `implements` links upward: Iteration -> Story -> RFC. Output each document's frontmatter in chain order (RFC first, then Story, then Iteration). If the document has no `implements` links, output just that document.

The chain is always walked upward (toward the design intent). Downward queries ("what implements this RFC?") are already covered by `show` + relationship display and `search`.

```
$ lazyspec context ITER-001 --json
```

```json
{
  "chain": [
    {
      "path": "docs/rfcs/RFC-001-my-first-rfc.md",
      "title": "Core Document Management Tool",
      "type": "rfc",
      "status": "accepted",
      "author": "jkaloger",
      "date": "2026-03-04",
      "tags": ["core", "mvp"],
      "related": [...]
    },
    {
      "path": "docs/stories/STORY-002-cli-commands.md",
      "title": "CLI Commands",
      "type": "story",
      "status": "accepted",
      ...
    },
    {
      "path": "docs/iterations/ITER-001-cli-impl.md",
      "title": "CLI Implementation",
      "type": "iteration",
      "status": "accepted",
      ...
    }
  ]
}
```

Frontmatter only. No document bodies. An agent that needs a body can follow up with `show <id>` for a specific document.

### `status`

Outputs the full project graph in one call. Every document, its status, and its relationships.

```
$ lazyspec status --json
```

```json
{
  "documents": [
    {
      "path": "docs/rfcs/RFC-001-my-first-rfc.md",
      "title": "Core Document Management Tool",
      "type": "rfc",
      "status": "accepted",
      "author": "jkaloger",
      "date": "2026-03-04",
      "tags": ["core", "mvp"],
      "related": [
        { "type": "implements", "target": "..." }
      ]
    }
  ],
  "validation": {
    "errors": [],
    "warnings": []
  }
}
```

Frontmatter only, all documents, all relationships. Includes inline validation results so the agent doesn't need a separate `validate` call. Human-readable output (without `--json`) uses a compact table format grouped by type.

### `--json` on all commands

Every command that produces output gets `--json` support. Currently missing from `show`. The JSON schema is consistent across commands: documents are always represented as the same frontmatter object.

### Expanded `validate`

New validation rules beyond the existing broken-link and structural checks:

| Rule | Severity | Description |
|---|---|---|
| Superseded parent | warning | Accepted document implements a superseded document |
| Rejected parent | error | Accepted/draft document implements a rejected document |
| Orphaned acceptance | warning | Accepted iteration whose parent story is still in draft |
| Status regression | warning | Document date is older than its parent's date (may indicate stale work) |

Warnings don't affect the exit code. Errors do. This distinction matters for CI usage: warnings are informational, errors block.

The `validate` command gains `--warnings` to include warnings in output (hidden by default to keep noise down). `status --json` always includes both.

## Stories

1. **Context command** — `context <id>` with chain walking and `--json` output
2. **Status command** — `status` with full project graph, inline validation, and `--json` output
3. **JSON everywhere** — `--json` on `show`, consistent document schema across all commands
4. **Expanded validation** — new staleness/consistency rules with warning/error severity
