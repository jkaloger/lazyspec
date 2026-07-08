---
title: Link ClickUp tasks to lazyspec docs via relations
type: story
status: in-progress
author: unknown
date: 2026-07-05
tags: []
related:
- implements: RFC-056
---

As a developer, I want to link a ClickUp-backed doc to other lazyspec docs (RFC/story/spec) so ClickUp-tracked work joins lazyspec's relation graph.

Implements RFC-056 (ClickUp store). Journey step: manage. Depends on the read-skeleton story (needs doc-id resolution from the read path).

## Acceptance criteria

- Given a ClickUp-backed doc, when I run `lazyspec link <task> implements RFC-056`, then the relation persists by serializing lazyspec relation data (the `issue_body.rs` YAML relations-block format) into a configured ClickUp *text* custom field as a full-replace write — not a relationship-type field, not ClickUp's native dependency/linked-task API — so targets in any store (e.g. a filesystem RFC) are representable.
- Given a fetched task carrying relation data in its custom field, when I run `context --json`, then its relations resolve the same as filesystem docs.
- Given the custom-field map config (`clickup_custom_field_map`), then relation types and any non-native attribute resolve by name/id.

## Non-functional

- One custom-field mechanism handles every relation type lazyspec persists; this does not generalize the existing `github_native` mechanism into a parallel `clickup_native` path.
