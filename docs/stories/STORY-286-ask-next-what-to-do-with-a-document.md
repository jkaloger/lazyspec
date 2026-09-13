---
title: Ask next what to do with a document
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

As an agent picking up a document, I want one command that tells me what to do, what to run, and whether a human should approve first, so that the dispatch decision is computed once against the graph instead of re-derived from an edge table in every skill.

The sessions behind ITERATION-399, 400 and 401 are agents misreading that table. The constraint on the answer: lazyspec's graph is the user's. It may be a RAID register, a repo of nothing but ADRs, iterations against stories, or a type nobody here has imagined. So `next` reports what the config says to do at a state and otherwise reads the graph — it never guesses what a document means. Two earlier drafts of this design failed that test (a four-value `role` enum, then fixed `review`/`work_ready`/`work_active` state names); both are dropped.

## Acceptance Criteria

- **Given** a `[[types.lifecycle]]`, **then** it may declare `actions = { <state> = { verb = "<string>", approve = <bool> } }` — optional, keys validated against the declared `states`, `approve` defaulting to `false`, `verb` an opaque string lazyspec never interprets. `config --json` exposes the map.
- **Given** a `verb` naming a command lazyspec has never heard of, **then** `next` reports it unchanged. No enum, no validation against a known verb list.
- **Given** an `actions` key naming a state not in `states`, **then** config validation rejects it.
- **Given** the shipped config, **then** every type's `review` state declares `/review`; `iteration` declares `/execute` (approve) at `accepted` and `/review-work` at `in-progress`; `bug` declares the same pair at `triaged` and `in-progress`. Every other state is unannotated.
- **Given** `show <id> --json`, **then** the object carries `next_statuses`, `child_types` and `unsatisfied_edges` — injected as keys the way `staleness` is, not added to `DocMeta`. `next_statuses` is `[]` at a terminal status; each `child_types` entry is `{type, authorship, verb}` mapping `human -> /scaffold`, `assisted -> /co-write`, `generated -> /generate`; `unsatisfied_edges` carries the `ValidationIssue::UnsatisfiedEdge` shape, and `to = ["*"]` stays `["*"]`.
- **Given** `next <id> --json`, **then** it returns `{doc, type, status, action, verb, next_status, crossing, requires_approval}` where `action` is one of `author | declared | boundary | advance | terminal` and names where the answer came from, not what it means.
- **Given** an unwritten body (equal to the rendered template, or only headings and guidance comments) and a permitted authoring verb, **then** `action` is `author` at the ceiling verb, `requires_approval: true` — and this holds even when the state declares an action, so an empty document at a `/execute` state is authored, not executed.
- **Given** the state has an `actions` entry, **then** `action` is `declared`, carrying that entry's `verb` and its `approve` as `requires_approval`.
- **Given** the state has at least one explicitly-declared out-edge and the type has child types, **then** `action` is `boundary`, `requires_approval: true`, and `crossing.types` lists **every** child type with its ceiling verb — `next` never picks one. `crossing.unsatisfied_edges` reports which required edges are unmet.
- **Given** a terminal state (no explicitly-declared out-edge) of a type that has child types, **then** `action` is `terminal`, not `boundary`.
- **Given** exactly one explicitly-declared out-edge and no earlier rule matched, **then** `action` is `advance` into it, `requires_approval: false`. **Given** a wildcard edge (`from = "*"`, such as `-> superseded`), **then** it is excluded from that count — reachable everywhere is never "the" next move.
- **Given** a type that declares no `actions` at all, **then** `next` still answers from the graph alone: `author`, `boundary`, `advance` or `terminal`, and never a verb it invented.
- **Given** the TUI detail pane and the web doc page, **then** both render `next_statuses`, `child_types` and the `next` decision. Human CLI output is one line per field.

## Scope

### In Scope

- `[[types.lifecycle]] actions`, its validation, and the shipped defaults above.
- Derived key injection at `src/cli/show.rs`, reusing `Lifecycle::targets_from` and `traversal::child_types_for`.
- `engine::next`, the `next <ID> [--json]` command, TUI row, web display.
- Decision order exactly as RFC-071 §Next lists it, first match wins.

### Out of Scope

- Any vocabulary for what a document *is*. No `role`, no fixed state-role names, nothing inferred from `intent` prose or a state's position.
- Choosing between child types at a crossing. Several candidates means several are returned.
- Lifting the authorship ceiling map (`human`/`assisted`/`generated` -> verb) into config. One runtime uses it; it stays a constant until a second asks (principle 6).
- Derived fields on `context --json`. Its `target` is a path string; widening it breaks a key the TUI, web view and skills all read.
- `--ready` or a dependency-ordered list for `/orchestrate`. Follow-up.
- Enforcing authorship ceilings in the binary. `next` reads the ceiling; nothing refuses on it.
