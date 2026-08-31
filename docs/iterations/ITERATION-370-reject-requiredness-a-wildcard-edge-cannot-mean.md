---
title: Reject requiredness a wildcard edge cannot mean
type: iteration
status: draft
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-256
- blocks: ITERATION-371
---

## Objective

Config load rejects two requiredness declarations that cannot mean anything: `required` on a row whose `from` is `"*"`, and two rows of equal specificity that can match the same concrete edge but disagree on requiredness — the second naming both rows.

## Satisfies

STORY-256 AC3, AC4. Both are load-time rejections of a requiredness declaration, share the specificity and overlap machinery, and land in the same strict-load path — splitting them would mean two agents editing the same loop. AC2 deferred; AC1, AC5, AC6 landed in the preceding iterations.

## Context

- Story + ACs: STORY-256
- Specificity ordering, the tie-is-an-error decision, and rejecting `required` on a wildcard `from`: ADR-031 §Decision
- Why the message must name both rows by `name`, and why the O(n²) cost is acceptable: ADR-031 §Consequences
- RFC-067 §Open questions lists "does `required` on a `from = \"*\"` row mean anything" as closed by ADR-031; this slice is where the check lands
- Touch:
  - `src/engine/config.rs` — the strict-load loop in `Config::parse` that already rejects unknown types and relationships; the selectors gain specificity and overlap
  - `README.md` §`[[edges]]`

## Tasks

1. Test-first: `required = "error"` on a row with `from = "*"` fails load, naming the edge. The same row without `required` loads fine — a wildcard `from` is legal, demanding the edge from every declared type is not.
2. Test-first: two rows that can both match one concrete edge, of equal specificity, disagreeing on requiredness, fail load with a message naming both `name`s. Cover both flavours of disagreement — two different severities, and `required` set on one row and absent on the other. Absence is a positive statement ("legal, but its absence is not a finding", RFC-067 §Design), so it disagrees with `required = "error"`.
3. Specificity: the count of concrete positions among `from`/`to`/`via`, 0 through 3. `Any` scores nothing; a named selector scores one whether it lists one type or six. Record ADR-031 §Consequences' accepted coarseness in a comment where the scoring lives — `from = "iteration", to = "*"` and `from = "*", to = ["story"]` both score one and are a tie error, not a resolution.
4. Overlap: two rows can match one concrete edge when all three positions intersect — `Any` intersects anything, two `Types` intersect when they share a member.
5. Wire both checks into `Config::parse` beside the existing edge checks, and test that unequal-specificity overlap and non-overlapping rows both still load.
6. README: overlapping rows may not disagree on requiredness at equal specificity, and `required` may not sit on a wildcard `from`. State the specificity ordering here, since the next slice's behaviour depends on a reader knowing it.

## Out of scope

- Which row wins when specificity differs (AC2) → next iteration. This slice only proves no ambiguous pair survives load.
- Traversal-role contradiction between overlapping rows (ADR-031 §Decision, "two rows assigning different roles is a load error") → STORY-257, where `traversal` joins the edge. The overlap predicate built here is what that check will reuse.
- `fix --config` emitting rows that could tie → STORY-258.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: rejection happens in the engine's load path, not in a CLI validator.

## Verification

`cargo run -- config --json` still loads this repo's config, and `cargo run -- validate --json` is unchanged. A scratch config carrying ADR-031 §Consequences' worked tie (`from = "iteration", to = "*"` against `from = "*", to = ["story"]`, both with `required`) fails load with both names in the message.
