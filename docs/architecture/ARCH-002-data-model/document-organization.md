---
title: "Document Organization"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, data-model]
related:
  - related-to: "docs/rfcs/RFC-010-subfolder-document-support.md"
  - related-to: "docs/rfcs/RFC-014-nested-child-document-support.md"
  - related-to: "docs/stories/STORY-040-child-document-discovery.md"
---

# Document Organization

Documents live in type-specific directories. Two layout patterns are supported.
See [RFC-010: Subfolder Document Support](../../rfcs/RFC-010-subfolder-document-support.md)
and [RFC-014: Nested child document support](../../rfcs/RFC-014-nested-child-document-support.md)
for the design rationale.

## Flat Layout

```
docs/rfcs/
  RFC-001-my-feature.md
  RFC-002-another-feature.md
```

## Nested Layout (Parent + Children)

```
docs/stories/
  STORY-001-user-auth/
    index.md              # parent document
    login-flow.md         # child document
    signup-flow.md        # child document
```

When a folder exists without `index.md`, lazyspec creates a virtual parent document.
Virtual documents have their status computed from children (all accepted = accepted,
otherwise draft). They are not persisted to disk.

```d2
direction: down

folder: "STORY-001-user-auth/" {
  index: "index.md\n(parent)" {
    style.fill: "#e8f0fe"
  }
  child1: "login-flow.md\n(child)"
  child2: "signup-flow.md\n(child)"
}

store: Store {
  parent_of: "parent_of map"
  children: "children map"
}

folder.index -> store.children: "registers children"
folder.child1 -> store.parent_of: "maps to parent"
folder.child2 -> store.parent_of: "maps to parent"
```
