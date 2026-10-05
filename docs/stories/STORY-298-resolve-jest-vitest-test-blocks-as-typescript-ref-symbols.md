---
title: Resolve Jest/Vitest test blocks as TypeScript @ref symbols
type: story
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: RFC-019
---

## Context

`@ref foo.test.ts#<title>` fails for Jest/Vitest tests. The matcher only accepts declarations with a `name` field. `it("title", ...)` and `test("title", ...)` are `call_expression` nodes with a string first argument, so they never match.

## Acceptance Criteria

- **Given** a `.ts` file with `it("does x", ...)` or `test("does x", ...)`
  **When** `@ref file#does x` is resolved
  **Then** the whole call expression (statement) is returned.

- **Given** a `describe("suite", ...)` block
  **When** `@ref file#suite` is resolved
  **Then** the whole `describe` call is returned.

- **Given** an `it` nested inside a `describe`
  **When** `@ref file#<its title>` is resolved
  **Then** the nested test is found.

- **Given** a callee `it.skip`, `it.only`, `test.skip`, `test.only`, `describe.skip`, or `describe.only`
  **When** its title is referenced
  **Then** it resolves like the plain form.

- **Given** a title written as a template literal with no substitutions
  **When** it is referenced
  **Then** it matches like a string literal.

- **Given** an existing function, class, or other named declaration
  **When** it is referenced
  **Then** lookup is unchanged.

- **Given** two tests with the same title
  **When** the title is referenced
  **Then** the first match in document order is returned.

- **Given** a declaration and a test block with the same name, in either order
  **When** the name is referenced
  **Then** the declaration is returned. The declaration search runs over the whole tree first; test blocks are only a fallback.

- **Given** a title containing spaces
  **When** written as `@ref file#"title"` (optionally with `\"` inside, and a trailing `@{blob:..}` or `@sha`)
  **Then** the text between the quotes is the symbol. Unquoted refs parse as before. `pin` keeps the quotes. An unterminated quote parses as an unquoted symbol.

## Scope

### In Scope

- Title matching for `it`, `test`, `describe` and their `.skip`/`.only` forms in TypeScript.
- Deterministic first-match on duplicate titles.
- Quoted `#"symbol"` form in `@ref`.

### Out of Scope

- `describe > it` path syntax. Revisit when a real duplicate title appears.
- Other test frameworks and `.each` table forms.
