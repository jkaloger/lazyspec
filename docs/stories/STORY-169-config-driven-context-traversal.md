---
title: "Config-driven context traversal"
type: story
status: accepted
author: "jkaloger"
date: 2026-06-26
tags: []
related: []
---

## Problem

Context resolution (RFC -> Story -> Iteration chain + related-to neighbourhood) driven by side effect of validation rules. `chain_relationships` derived from `link` field of `[[rules]]` ParentChild entries; `related-to` BFS hardcodes literal string. Relationship cannot join context chain without also writing a validation rule. Two orthogonal concerns coupled: validation (type hierarchy must hold) vs context (which edges form navigable chain).

## Want

Declare traversal behaviour at relationship declaration (`[[relationships]]`) via `traversal` field. Context config-driven, independent of validation rules.

## Acceptance

- `[[relationships]]` entry can opt into context via `traversal = "chain"` or `traversal = "related"`, no validation rule required.
- Chain relationship with no ParentChild rule still produces a context chain.
- ParentChild rule validates against any chain edge to parent-type doc (no named `link`).
- `related-to` neighbourhood behaviour preserved after migration; emitted relation carries real edge type.
- No traversal markers = empty chain, legal silent state, no warning.
- README + starters + init document the model and the breaking removal of rule `link`.
