---
title: Status as validated newtype over a per-type DAG
type: adr
status: accepted
author: jkaloger
date: 2026-06-21
tags:
- status
- config
- types
related:
- related-to: RFC-048
---

## Context

Status is the one closed axis in lazyspec: a fixed 7-variant enum (`@ref src/engine/document.rs#Status`) with no transition model. Types are open (RFC-042) and relations are a validated string newtype with a config registry (ADR-010). Once RFC-048 makes status carry workflow weight (gating, the `advance` verb), the closed enum forces lazyspec's vocabulary (`accepted`, `in-progress`) onto every user and leaves the binary unable to validate transitions or name the sensible next status.

## Decision

Status becomes a validated string newtype, mirroring the ADR-010 pattern used for `RelationType`. A document's status is validated against its owning type's lifecycle `states` (ADR-021). Transitions are the lifecycle's declared `edges`: `update --status` rejects any move not on an edge.

The current 7 statuses become the starter config's default lifecycle. `fix --config` injects that default DAG into pre-existing configs as the migration path.

## Consequences

- Status vocabulary and transitions are user-defined, consistent with the open type and relation models.
- "Unconstrained any→any" becomes "transitions are the edges you declared", yielding transition validation for free.
- Touches every site that constructs or matches the `Status` enum (serialize/parse/display, TUI filters, list filtering). Scoped into RFC-048 Story 1.
- Migration is mechanical via `fix --config`; existing documents keep their current status values, which are present in the default lifecycle.

Locked for RFC-048. Extends ADR-010; pairs with ADR-021.
