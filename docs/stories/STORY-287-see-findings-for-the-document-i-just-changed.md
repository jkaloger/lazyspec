---
title: See findings for the document I just changed
type: story
status: draft
author: Jack Kaloger
date: 2026-09-12
tags: []
related:
- implements: RFC-071
- blocks: STORY-288
---

## Context

As an agent that just mutated a document, I want that document's findings in front of me in the same turn, so that a link or status move that broke something is caught where I made it.

RFC-068 already made findings structured objects with rule slugs. What is missing is the filter — and without it the hook greps whole-repo output by substring, where `STORY-23` matches `STORY-237`. The hook is the caller that makes the filter worth building; they ship together.

## Acceptance Criteria

- **Given** `validate --id STORY-23 --json`, **then** output carries only findings whose document-path fields resolve to that document, and `STORY-237`'s findings do not appear.
- **Given** a `broken-link` finding, **then** `--id` matches it via `source` or `target`; **given** `duplicate-id`, **then** via the `paths` list; **given** `rejected-parent` or `superseded-parent`, **then** via `path` or `parent`. Matching `path` alone would drop the two rules most likely to fire right after a `link`.
- **Given** the test suite, **then** a test asserts every rule variant is reachable through `--id`.
- **Given** a `PostToolUse` hook on `Bash`, **when** the command was a lazyspec mutation naming an ID, **then** that document's findings are injected into context.
- **Given** the document is clean, or the command was not a lazyspec mutation, **then** the hook is silent; **given** an ID that no longer resolves, **then** it exits without error.

## Scope

### In Scope

- `validate [--id <ID>]`, resolving through the store's existing lookup, matching across `path`, `parent`, `source`, `target`, `paths`.
- One `PostToolUse` hook script in `hooks/` composing it.

### Out of Scope

- A per-finding `severity` key. Severity stays positional — error or warning by which array a finding lands in.
- The commit gate. Documented in `hooks/` as an opt-in pattern, shipped disabled: this repo carries 56 standing errors, so a gate here fires every commit and trains the agent to dismiss it. It is honest only against a clean baseline.
