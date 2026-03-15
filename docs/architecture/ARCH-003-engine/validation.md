---
title: "Validation"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, engine, validation]
related:
  - related-to: "docs/rfcs/RFC-008-project-health-awareness.md"
  - related-to: "docs/stories/STORY-022-expanded-validation.md"
  - related-to: "docs/stories/STORY-024-upward-consistency-validation.md"
  - related-to: "docs/stories/STORY-038-config-driven-validation-rules.md"
  - related-to: "docs/stories/STORY-030-validate-ignore-flag.md"
---

# Validation

`validate_full(store, config)` is a pure function that returns a `ValidationResult`
containing separate error and warning vectors. It never mutates the store.

Driven by [RFC-008: Project Health Awareness](../../rfcs/RFC-008-project-health-awareness.md),
with upward consistency added in [STORY-024](../../stories/STORY-024-upward-consistency-validation.md).

## Validation Flow

```d2
direction: down

input: "Store + Config" {
  shape: parallelogram
}

broken: "Check broken links" {
  desc: "For each relation, verify target exists in store"
}

status: "Check status consistency" {
  desc: "Rejected parent = error\nSuperseded parent = warning\nOrphaned acceptance = warning"
}

rules: "Apply config rules" {
  parent_child: "ParentChild: child must link to parent type"
  relation_exist: "RelationExistence: doc must have any relation"
}

hierarchy: "Hierarchy checks" {
  all_accepted: "AllChildrenAccepted: nudge parent"
  upward: "UpwardOrphanedAcceptance"
}

dupes: "Duplicate ID check" {
  desc: "No two documents share the same extracted ID"
}

result: "ValidationResult" {
  shape: parallelogram
  errors: "Vec<ValidationIssue>"
  warnings: "Vec<ValidationIssue>"
}

input -> broken -> status -> rules -> hierarchy -> dupes -> result
```

## Issue Types

@ref src/engine/validation.rs#ValidationIssue

| Issue | Severity | Trigger |
|---|---|---|
| BrokenLink | error | Relation target path not in store |
| MissingParentLink | configurable | Child type missing required parent relation |
| MissingRelation | configurable | Document type has no relations at all |
| RejectedParent | error | Implements a rejected document |
| SupersededParent | warning | Accepted doc implements superseded parent |
| OrphanedAcceptance | warning | Accepted child, non-accepted parent |
| AllChildrenAccepted | warning | All children accepted but parent isn't |
| UpwardOrphanedAcceptance | warning | Same as orphaned, different traversal |
| DuplicateId | error | Multiple docs share extracted ID |

Documents with `validate-ignore: true` are skipped entirely.
See [STORY-030: Validate-Ignore Flag](../../stories/STORY-030-validate-ignore-flag.md).
