---
title: "Data Model"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, data-model]
related:
  - related-to: "docs/rfcs/RFC-001-my-first-rfc.md"
  - related-to: "docs/rfcs/RFC-013-custom-document-types.md"
  - related-to: "docs/stories/STORY-001-document-model-and-store.md"
---

# Data Model

This document covers lazyspec's core data structures: documents, relationships,
frontmatter schema, and configuration.

The data model was established in [RFC-001: Core Document Management Tool](../../rfcs/RFC-001-my-first-rfc.md)
and implemented through [STORY-001: Document Model and Store](../../stories/STORY-001-document-model-and-store.md).
Subsequent RFCs extended it:

- [RFC-013: Custom document types](../../rfcs/RFC-013-custom-document-types.md) made types configurable
- [RFC-014: Nested child document support](../../rfcs/RFC-014-nested-child-document-support.md) added parent/child hierarchy
- [RFC-010: Subfolder Document Support](../../rfcs/RFC-010-subfolder-document-support.md) introduced folder-based layouts

## Document Model

Every document is a markdown file with YAML frontmatter. The frontmatter is the
single source of truth for metadata; the body is free-form markdown.

```d2
direction: right

doc: Document {
  frontmatter: "YAML Frontmatter" {
    shape: class
    title: "string"
    type: "DocType"
    status: "Status"
    author: "string"
    date: "NaiveDate"
    tags: "Vec<string>"
    related: "Vec<Relation>"
    validate-ignore: "bool"
  }

  body: "Markdown Body" {
    shape: document
  }
}
```

The metadata struct that backs every document:

@ref src/engine/document.rs#DocMeta

Children cover the detailed breakdowns:
- **document-types**: DocType, Status, RelationType, ID extraction
- **document-organization**: Flat/nested layouts, virtual documents
- **configuration**: Config schema, validation rules, templates
- **ref-directives**: @ref syntax and expansion
