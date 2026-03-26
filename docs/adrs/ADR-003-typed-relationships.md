---
title: Typed Relationships Over Directory Nesting
type: adr
status: accepted
author: jkaloger
date: 2026-03-04
tags:
- architecture
- relationships
related:
- related-to: RFC-001
---


## Context

Documents need to reference each other. Options considered:

1. **Directory nesting** - put iterations inside story directories (e.g. `stories/auth/iterations/`)
2. **Naming conventions** - prefix iteration filenames with story ID
3. **Typed relationships in frontmatter** - explicit `related` field with relation types

## Decision

Typed relationships in frontmatter using four relation types: `implements`, `supersedes`, `blocks`, and `related-to`. Relationships are stored in the source document's `related` array and resolved bidirectionally by the store's `LinkGraph`.

```yaml
related:
  - implements: docs/stories/STORY-001-user-auth.md
  - supersedes: docs/rfcs/RFC-000-old.md
```

## Consequences

- Documents are flat within their type directory, no nesting complexity
- Relationships are explicit and queryable (find all documents that implement RFC-001)
- Validation can check link integrity (target must exist)
- Moving files breaks links since relationships use relative paths
- No implicit relationships, everything must be declared
