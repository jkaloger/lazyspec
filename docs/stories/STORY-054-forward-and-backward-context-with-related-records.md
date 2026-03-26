---
title: Forward and Backward Context with Related Records
type: story
status: accepted
author: jkaloger
date: 2026-03-10
tags:
- cli
- tui
- context
related:
- implements: RFC-007
---




## Context

The `lazyspec context` command walks the `implements` chain upward from a document (Iteration -> Story -> RFC). This is useful for understanding design intent, but it only tells half the story. When looking at an RFC, you can't see what Stories implement it. When looking at a Story, you can't see its Iterations. And `related to` links are present in frontmatter but invisible in context output.

Agents and humans both need the full picture: what came before, what came after, and what's related.

## Acceptance Criteria

### Forward context (CLI)

- **Given** an RFC that has Stories implementing it
  **When** I run `lazyspec context <rfc-id>`
  **Then** the output includes the RFC mini-card followed by its child Stories listed with tree connectors (`├─`, `└─`)

- **Given** a Story that has Iterations implementing it
  **When** I run `lazyspec context <story-id>`
  **Then** the output shows the backward chain (RFC → Story as mini-cards with `│` connectors), then the Story's child Iterations listed with tree connectors

- **Given** a document with no forward relationships
  **When** I run `lazyspec context <id>`
  **Then** the output shows only the backward chain (existing behavior, no empty children section)

> [!NOTE]
> The existing code already renders `children_of` with tree connectors for every node in the chain. The change here is that the target document must also show its own children (forward context), not just nodes above it in the chain.

### "You are here" marker (CLI)

- **Given** I run `lazyspec context <id>` where `<id>` resolves to a document in the middle of a chain
  **When** the styled output renders
  **Then** the target document's mini-card has a `← you are here` marker appended to its top line, distinguishing it from ancestors and descendants

### Related records (CLI)

- **Given** a document in the chain has `related to` links in its frontmatter
  **When** the styled output renders
  **Then** a `─── related ───` section appears after the chain, listing each related document as `SHORTHAND  Title [status]`

- **Given** no documents in the chain have `related to` links
  **When** the styled output renders
  **Then** the related section is omitted entirely (no empty header)

> [!NOTE]
> Related records are collected from all documents in the chain, not just the target. The related section aggregates them, deduplicated.

### JSON output (CLI)

- **Given** `--json` is passed
  **When** I run `lazyspec context <id> --json`
  **Then** the output includes `chain` (the full backward+forward chain as today), plus a new `related` array containing frontmatter objects for all `related to` targets across the chain

### TUI context view

- **Given** a document is selected in the TUI
  **When** I view its Relations tab
  **Then** the tab shows the backward chain, forward children, and related records grouped by section

- **Given** a related or child document is shown in the TUI relations view
  **When** I select it
  **Then** I navigate to that document's detail view

### Example: styled CLI output

```
$ lazyspec context STORY-019

╭──────────────────╮
│ Agent-Native CLI  │
│ rfc [accepted]    │
╰──────────────────╯
  │
╭───────────────────╮
│ Context Command    │  ← you are here
│ story [accepted]   │
╰───────────────────╯
  │
╭─────────────────────────╮
│ Context Command          │
│ iteration [accepted]     │
╰─────────────────────────╯

─── related ───
  RFC-002  AI-Driven Development Workflow [accepted]
  STORY-002  CLI Commands [accepted]
```

## Scope

### In Scope

- Forward traversal of `implements` relationships (reverse direction)
- Surfacing `related to` links in context output
- JSON schema extension for forward and related fields
- TUI detail view showing forward, backward, and related records
- Navigation from related/child records in TUI

### Out of Scope

- Transitive forward traversal (RFC -> Story -> Iteration in one forward walk)
- New relationship types
- Graph visualization
- Modifying the `status` command output
