---
title: Require a parent when creating a document that needs one
type: story
status: draft
author: Jack Kaloger
date: 2026-09-12
tags: []
related:
- implements: RFC-071
- related-to: ADR-036
---

## Context

As a human whose repo carries 30 orphaned iterations, I want `create` to refuse a document that needs a parent unless I name one or opt out, so that the link happens at creation instead of never.

ADR-033 said an after-the-fact finding would clear none of them, and it has cleared none of them — 19 predate the verb skills, one arrived since. ADR-036 supersedes it with a structural gate.

## Acceptance Criteria

- **Given** a type on the `from` side of a required chain edge, **when** I run `create` without `--parent` or `--orphan`, **then** it refuses and names both flags.
- **Given** `--parent <ID>`, **then** the document is created and the chain link is written in the same operation.
- **Given** a `--parent` of the wrong type for the edge, **then** it refuses and names the expected type.
- **Given** `--orphan`, **then** the document is created unlinked and the existing validation finding still reports it.
- **Given** a type on no required chain edge, **then** `create` is unchanged.

## Scope

### In Scope

- The gate in `create`, `--orphan`, the `--parent` link write, the refusal messages.

### Out of Scope

- Status-conditioned gating. ADR-033 stands on that point; this gate is structural only.
- Retrofitting the 30 existing orphans.
