---
title: "Document Types and Relations"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, data-model]
related:
  - related-to: "docs/stories/STORY-004-story-iteration-types.md"
  - related-to: "docs/stories/STORY-037-config-driven-type-definitions.md"
---

# Document Types and Relations

## Frontmatter Schema

```yaml
---
title: "Document title"
type: rfc              # one of the configured types
status: draft          # draft | review | accepted | rejected | superseded
author: "Author Name"
date: 2026-03-14
tags: [feature, backend]
related:
  - implements: "docs/rfcs/RFC-001-feature.md"
  - blocks: "docs/iterations/ITERATION-005-thing.md"
validate-ignore: false  # optional, skip validation
---
```

## DocType

@ref src/engine/document.rs#DocType

A newtype wrapper around `String`, always stored lowercase. Four built-in types
ship by default but are fully configurable via `.lazyspec.toml`:

| Type | Prefix | Default Dir | Icon |
|---|---|---|---|
| rfc | RFC | docs/rfcs | ● |
| story | STORY | docs/stories | ▲ |
| iteration | ITERATION | docs/iterations | ◆ |
| adr | ADR | docs/adrs | ■ |

## Status Lifecycle

@ref src/engine/document.rs#Status

```d2
direction: right

draft: Draft {
  style.fill: "#fff3e0"
}
review: Review {
  style.fill: "#e3f2fd"
}
accepted: Accepted {
  style.fill: "#e8f5e9"
}
rejected: Rejected {
  style.fill: "#fce4ec"
}
superseded: Superseded {
  style.fill: "#f5f5f5"
}

draft -> review: "ready for review"
review -> accepted: "approved"
review -> rejected: "declined"
accepted -> superseded: "replaced by newer doc"
review -> draft: "needs rework"
```

## Relation Types

@ref src/engine/document.rs#RelationType

Relations are directional links stored in the source document's frontmatter.

| Type | Meaning | YAML Key |
|---|---|---|
| Implements | Child realizes parent spec | `implements` |
| Supersedes | Replaces an older document | `supersedes` |
| Blocks | Prevents progress on target | `blocks` |
| RelatedTo | Loose association | `related-to` |

## Document ID

IDs are derived from the filename, not stored in frontmatter. This prevents drift.

@ref src/engine/store.rs#extract_id_from_name

The extraction algorithm:
1. Split filename stem on `-`
2. Find the first segment that is all digits
3. Join `PREFIX-NNN` from the segments up to and including that number

Examples: `RFC-001-my-feature.md` yields `RFC-001`. Folder-based documents
(`STORY-001/index.md`) derive their ID from the folder name.
