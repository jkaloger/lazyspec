---
title: Project Health Awareness
type: rfc
status: draft
author: jkaloger
date: 2026-03-05
tags: [cli, validation, health]
related:
- related-to: docs/rfcs/RFC-007-agent-native-cli.md
---

## Summary

Give lazyspec the ability to detect and surface project health issues that currently require manual auditing. Today an agent (or human) must dump the full document list, mentally trace parent-child relationships, cross-reference statuses, and issue individual update commands to fix drift. This RFC adds the validation rules, views, and CLI ergonomics to make that a single-command operation.

## Problem

RFC-007 gave the CLI structured output and commands like `status` and `context`. But the experience of auditing project health is still manual and expensive. We hit this directly: finding 11 stale documents required dumping the full list, tracing `implements` links by hand, reading iteration docs to verify completion, checking git history, and issuing 11 separate `update` calls.

Three specific gaps:

**No upward consistency validation.** `validate_full()` checks downward: child implementing a rejected/superseded parent, accepted iteration under a draft story. It doesn't check the reverse: all children accepted but parent still in draft. This is the most common drift pattern -- work gets done, iterations and stories get accepted, but the parent RFC or story never gets promoted.

**No hierarchical view.** `status` groups documents flat by type. When you're looking for inconsistencies, you need to see the tree: which stories belong to which RFC, which iterations belong to which story, and whether statuses are coherent across each branch. The flat view forces you to cross-reference manually.

**No bulk operations.** Fixing drift means running `update` once per document. With 11 documents to fix, that's 11 separate invocations. The CLI should accept multiple paths.

## Design Intent

### Upward consistency validation

Add a new validation rule that inverts the existing `OrphanedAcceptance` check. Instead of "child is accepted but parent isn't," check "all children are accepted but parent isn't."

For each document that has children (other documents with `implements` links pointing to it):
- If all children are `accepted` and the parent is still `draft` or `review`, emit a warning: "all children accepted, consider promoting parent."
- Scope to meaningful relationships: RFC with all stories accepted, Story with all iterations accepted.

This is a warning, not an error. Promotion is a human decision -- sometimes an RFC has future stories not yet written, so "all current children accepted" doesn't necessarily mean the RFC is done. The warning surfaces the question rather than answering it.

```
$ lazyspec validate --warnings
warning: all children accepted but parent is draft
  docs/rfcs/RFC-003-tui-document-creation.md (draft)
    docs/stories/STORY-006-create-form-ui-and-input-handling.md (accepted)
    docs/stories/STORY-007-document-creation-on-submit.md (accepted)
```

Generalise the existing `OrphanedAcceptance` check while we're here. Currently it only fires for iteration->story. It should also fire for story->RFC.

> [!NOTE]
> The existing `validate_full()` walks relationships from child to parent. The new check requires walking from parent to children, which means building a reverse index. This is a new traversal pattern for the store.

### `status --tree`

Add a `--tree` flag to the `status` command that renders the document graph as a hierarchy instead of flat-by-type grouping.

```
$ lazyspec status --tree
RFC-003 TUI Document Creation [accepted]
  STORY-006 Create Form UI and Input Handling [accepted]
    ITERATION-002 Create Form UI [accepted]
  STORY-007 Document Creation on Submit [accepted]
    ITERATION-003 Document Creation on Submit [accepted]

RFC-007 Agent-Native CLI [accepted]
  STORY-019 Context Command [accepted]
    ITERATION-007 Context Command [accepted]
  STORY-020 Status Command [accepted]
    ITERATION-009 Status Command [accepted]
  STORY-021 JSON Everywhere [accepted]
    ITERATION-006 JSON Everywhere [accepted]
  ...

(orphaned)
  ITERATION-001 TUI Enhancements Design [draft]
```

The tree is built by following `implements` links. Documents at the root are those that nothing implements (typically RFCs, or orphaned documents). Children are indented under their parent. Documents that appear in multiple chains (unlikely but possible) appear under their first parent with a reference marker elsewhere.

`--tree --json` outputs a nested structure:

```json
{
  "roots": [
    {
      "document": { ... },
      "children": [
        {
          "document": { ... },
          "children": [ ... ]
        }
      ]
    }
  ],
  "orphaned": [ ... ]
}
```

ADRs use `related-to` links, not `implements`, so they don't appear in the tree. They could appear as annotations on their related documents in a future iteration, but that's out of scope here.

### `status --summary`

Add a `--summary` flag that shows rollup counts instead of individual documents.

```
$ lazyspec status --summary
RFC     6 accepted  1 draft
Story   16 accepted  7 draft
Iter    10 accepted  1 draft
ADR     4 accepted

Health: 1 warning (run validate --warnings for details)
```

This gives agents and humans a quick pulse check without the full listing. The health line summarises validation results.

`--summary --json`:

```json
{
  "counts": {
    "rfc": { "accepted": 6, "draft": 1 },
    "story": { "accepted": 16, "draft": 7 },
    "iteration": { "accepted": 10, "draft": 1 },
    "adr": { "accepted": 4 }
  },
  "health": {
    "errors": 0,
    "warnings": 1
  }
}
```

`--summary` and `--tree` are mutually exclusive.

### Bulk `update`

Allow `update` to accept multiple document paths:

```
$ lazyspec update docs/stories/STORY-019.md docs/stories/STORY-020.md --status accepted
Updated docs/stories/STORY-019-context-command.md
Updated docs/stories/STORY-020-status-command.md
```

@ref src/cli/mod.rs -- the `Update` command currently takes `path: String`. Change to `paths: Vec<String>` with `#[arg(required = true)]`. Each path gets the same field updates applied. If any path fails, report the error and continue with the remaining paths (don't abort on first failure).

`--json` output for bulk update:

```json
{
  "updated": ["docs/stories/STORY-019-context-command.md", "docs/stories/STORY-020-status-command.md"],
  "failed": []
}
```

## Stories

1. **Upward consistency validation** -- reverse-index traversal, "all children accepted" warning, generalise `OrphanedAcceptance` to story->RFC
2. **Tree view** -- `status --tree` with hierarchical rendering, `--json` nested output
3. **Summary view** -- `status --summary` with rollup counts and health line, `--json` counts output
4. **Bulk update** -- `update` accepting multiple paths, `--json` output for results
