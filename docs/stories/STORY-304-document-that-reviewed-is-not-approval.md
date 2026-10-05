---
title: Document that reviewed is not approval
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-069
---

## Value

As a pack author, I want the README to say what `reviewed` means, so I do not read it as approval.

## Acceptance Criteria

- AC1: README states `reviewed` is the commit a document's content was last checked against. It is stamped on any status change (STORY-274) and by `pin`.
- AC2: README states `reviewed` is not approval: a transition to any status stamps it, and approval is a status or relation the pack defines.
- AC3: README states there is no per-type opt-out today.

## Scope

### In Scope

- Docs only. The staleness paragraph is `README.md:61`; the stamp is `STORY-274`.

### Out of Scope

- Per-type opt-out of the stamp. Deferred until a second consumer needs it; indirection needs two uses.
