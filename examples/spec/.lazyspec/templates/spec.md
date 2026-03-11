---
title: "{title}"
type: spec
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

<!-- Target: under 100 lines. If longer, extract implementation detail into a plan. -->

## Summary

One paragraph describing what this spec locks down and why.

## Scope

Unambiguous boundaries. No hedging (may include, if time permits).

### In Scope

What this spec covers.

### Out of Scope

What this spec explicitly does not cover.

## Acceptance Criteria

The core of this spec. Express requirements as given/when/then. Everything else supports these.

Given [precondition]
When [action]
Then [expected outcome]

## Data Models

Show shape, not wiring. Use `@draft` for new types, `@ref` for existing. Don't show how components consume them.

## API Surface

Endpoints, function signatures, message formats. Include request/response shapes.

## Validation Rules

Input constraints, business rules, invariants that must hold.

## Error Handling

Error types, codes, messages. How failures propagate and what callers see.

## Edge Cases

Boundary conditions, race conditions, unusual inputs. Document expected behavior.
