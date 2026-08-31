---
title: Gate create on the edge target status
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-255
---

## Objective

`create` is refused until a permitted target of the edge reaches the status `require_to_status` names for its type, with a machine-readable refusal.

## Satisfies

STORY-255 AC1, AC2, AC3, AC4, AC6. AC5 landed in the preceding iteration.

## Context

- Story + ACs: STORY-255
- ADR-022 (status-conditioned gating stands; only its carrier moves) — the story's own framing
- The scalar gate this parallels: `src/engine/ops/create.rs:86-116`
- Refusal plumbing: `src/cli/create.rs` for the `--json` shape
- Conventions: `lazyspec convention`

## Two decisions taken for you — read before starting

**1. The gate is an existence check across the project, not a check on a specific document.** `create` takes no target argument; its `--parent` flag controls directory nesting and is consumed later, at `create_with_parent`. The existing scalar gate at `create.rs:105-108` asks "does *any* document of the parent type sit at the required status?" ADR-022's decision stands and only its carrier moves, so this slice generalises that same question per target type — it does not introduce per-document targeting. Read the story's "against a story at `draft`" as "the project's stories are all at `draft`".

Consequence for AC2/AC4: the edge is satisfied when *any one* gated target type has a document at its required status, matching STORY-254's "any one member" rule for `required`. An ungated member (no `require_to_status` key) is satisfied by the existence of any document of that type.

If that reading is wrong the ACs still need re-wording, so surface it rather than quietly picking the other branch.

**2. The scalar `require_parent_status` gate stays.** The story's Notes say the edge gate "replaces" it, but `[[rules]]` and its scalar gate are retired by STORY-259, and STORY-254 set the precedent that the two tables run side by side during the migration window. Both gates run; a create must satisfy both. Do not delete `create.rs:86-116`.

## Tasks

1. Test-first: edge `from = "iteration"`, `to = ["story", "bug"]`, `require_to_status = { story = "accepted", bug = "triaged" }`. Cover: all stories at `draft` and no bugs → refused (AC1); a bug at `triaged` → succeeds (AC2); a bug at `reported` and no accepted story → refused (AC3); a `to` member with no key → creation succeeds against it (AC4).
2. Implement the gate in `src/engine/ops/create.rs`, beside the scalar one, reading `require_to_status` per target type. AC3 is the case a scalar could not express — `bug` has no `accepted` state — so make sure the test would fail against a single-status implementation.
3. Test-first: the refusal carries edge name, target type, current status and required status, and reaches `--json` in that shape (AC6). Where several target types are gated and none satisfied, report every unsatisfied gate rather than only the first; "current status" for a type with no documents at all is absent, not a fabricated value.
4. Check how existing `create` refusals surface under `--json` and match that shape rather than inventing one. Dictum 2.

## Out of scope

- Deleting the scalar `require_parent_status` gate → STORY-259.
- `"*"` wildcards → STORY-256. Traversal → STORY-257. Migration → STORY-258. Editors → STORY-260, STORY-261.

## Principles / conventions

`lazyspec convention`. Dictum 3: the gate is engine-side; the CLI only formats the refusal.

## Verification

`create` on this repo is unaffected — it declares no `[[edges]]`, so no edge gate can fire.
