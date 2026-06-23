---
title: Per-type inline status DAGs
type: adr
status: accepted
author: jkaloger
date: 2026-06-21
tags:
- status
- config
- lifecycle
related:
- related-to: RFC-048
---

## Context

RFC-048 makes status load-bearing for gating and for the `advance` verb, so a type's lifecycle (its set of statuses and the legal transitions between them) must be configurable. A linear order is insufficient: lifecycles branch and converge (`draft → {accepted, rejected}`, `* → superseded`), so the model is a DAG, not an ordered list.

The scope question: one global status DAG shared by all types, named reusable lifecycle DAGs referenced per type, or a DAG declared inline on each type. Different types have genuinely different lifecycles (an RFC's `draft → review → ratified` versus an iteration's `draft → in-progress → complete`), so a single global DAG would let a type reach a nonsensical state.

## Decision

Each type declares its own status DAG inline: `states` (nodes) and `edges` (directed transitions, `*` permitted as source) on the `TypeDef`. No shared lifecycle registry, no global DAG.

## Consequences

- A type fully self-describes its lifecycle in one place; reading the type's config block is sufficient to know its states and transitions.
- Duplication: types that share a lifecycle repeat the same `states`/`edges`. Accepted as the cost of locality and self-description.
- A shared `lifecycle =` reference is the documented future de-dup if duplication becomes painful (two concrete uses; principle 6).

Locked for RFC-048. Pairs with ADR-023 (status as a validated newtype over this DAG).
