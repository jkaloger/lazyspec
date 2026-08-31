---
title: Derive type boundaries in /lazy from the edge table
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-262
- blocks: ITERATION-399
- blocks: ITERATION-400
- blocks: ITERATION-401
- blocks: ITERATION-402
---

## Objective

`/lazy`'s prose derives type boundaries from `config --json`'s `edges` and nothing else: the union-of-two-sources instruction, and every sentence that reads boundaries out of `rules` or `parent_type`, is gone.

## Satisfies

STORY-262 AC1. AC2 lands in ITERATION-399, AC3 in ITERATION-400, AC4 in ITERATION-401, AC5 in ITERATION-402.

## Context

- Story + ACs: STORY-262, and §Notes: the prose should describe traversal as it actually behaves once edges own it
- What an edge row is and what each position means: ADR-030 §Decision; RFC-067 §"Interface sketch"
- `"*"` on any position, and specific-over-wildcard: ADR-031 §Decision
- That an edge is never a refusal, only a finding: RFC-067 §Design, final bullet
- Touch:
  - `skills/lazy/SKILL.md:79` -- the HARD-GATE paragraph that defines a type-boundary edge as "EITHER via `parent_type` OR via a parent-child `rule` ... Derive boundaries from the UNION". This is the sentence the story is named after
  - `skills/lazy/SKILL.md:90` -- the `<NEVER>` line repeating the union
  - `skills/lazy/SKILL.md:117` -- Preflight step 1, which tells the agent to read `types`, `relationships` and `rules` from `config --json`
  - `skills/lazy/SKILL.md:128` -- the bug-triage step, which says the fix-doc type "may carry a `parent_type` or a `relation-existence` rule (e.g. `iterations-need-stories`)"
  - `skills/lazy/SKILL.md:137` -- "No `parent_type` chain is hardcoded here"
  - `skills/lazy/SKILL.md:155` -- Stop-at-Type-Boundary, "(a `parent_type` edge OR a parent-child `rule`, per the HARD-GATE union)"
  - `skills/lazy/SKILL.md:67` -- the d2 diagram's edge label, `"next step crosses a type boundary (parent_type or parent-child rule), or only work remains"`
  - `src/engine/skills.rs:9-42` -- `EMBEDDED_SKILLS`, which `include_str!`s each `SKILL.md` into the binary; `src/cli/skills.rs:80-97` -- the AGENTS.md writer
  - `src/engine/prompt.rs:177-190` `child_types_for` -- the engine's own answer to the same question, rebuilt off the edge table in ITERATION-373 Task 5. The prose and this function must agree
- **The direction is the trap.** An edge reads child-to-parent: `from = "iteration"`, `to = ["story"]`, `via = "implements"` says an iteration implements a story. So the *child types of a story* are the `from` values of rows whose `to` admits `story` -- a reverse lookup, not a forward one. The union instruction it replaces read forward (`rule.parent == doc_type` yields `rule.child`), so an agent transliterating the old sentence into the new table gets the arrow backwards and reports the parent type as the thing to create. Write the reverse lookup out as a jq the agent can run, and say which end of it is the boundary being crossed.
- **`"*"` makes "which types are children of this one" unanswerable by lookup alone.** A row with `from = "*"` admits every declared type as a child, so the answer is `types[]` filtered by the row, not a list read off the row. The prose must send the agent to `types` for the vocabulary and to `edges` for the constraint, and must not imply that a boundary's target set is always spelled out on the row.
- **AGENTS.md and `skills/lazy/SKILL.md` have diverged, in both directions.** AGENTS.md is a generated artifact (`src/cli/skills.rs:95` writes it by concatenating the embedded skills), but the checked-in copy carries a `## Rules` section, a "Confirm the plan before mutating" checklist and a longer RED-FLAGS list that `skills/lazy/SKILL.md` does not have -- and its `<BODY-CONTENT>` block mentions `--body-file`, which the skill's does not. The union instruction appears **three** times in AGENTS.md (`:812`-adjacent HARD-GATE, the `<NEVER>` line, and the `## Rules` bullet at `:923`) and twice in the skill. **Decide, and record it:** either back-port AGENTS.md's extra prose into `skills/lazy/SKILL.md` and regenerate, or edit both files by hand and accept that the generator's output no longer matches the checked-in file. Regenerating without back-porting silently deletes prose. This decision governs the four slices after this one, so it belongs here and not in each of them.
- **`rules` will not exist to read.** ITERATION-385 removes `Config.rules`, so `config --json` stops carrying the key entirely -- the instruction at `:117` does not merely become redundant, it names a field that is absent. That is the blocking edge to 385. ITERATION-373 and ITERATION-374 are the other two: the prose describes traversal, and until those land it would be describing `RelationshipDef.traversal`.

## Tasks

1. Resolve the AGENTS.md divergence from Context first, and write the answer into `skills/README.md` so the next slice does not re-litigate it. Everything after this depends on knowing which file is edited and which is generated.
2. Rewrite `skills/lazy/SKILL.md:79`'s HARD-GATE paragraph: a type-boundary edge is a row in `edges` whose `from` admits the current type and whose `to` admits a different one, with the reverse-lookup direction from Context stated explicitly and `"*"` on either end called out.
3. Give Preflight step 1 (`:117`) the exact command whose output the agent reads -- `lazyspec config --json` -- and name the keys: `types` for the vocabulary and ceilings, `edges` for the DAG, `relationships` for the link verb. Drop `rules`.
4. Rewrite `:90`, `:128`, `:137` and `:155` to the same vocabulary. `:128`'s "`relation-existence` rule (e.g. `iterations-need-stories`)" becomes the edge of that name; the example survives the change because ITERATION-383 scaffolds a row with that exact name.
5. Update the d2 diagram label at `:67`. A diagram that still says "parent-child rule" is the copy a reader trusts, because it is the one they look at first.
6. Regenerate AGENTS.md through `lazyspec skills install --runtime agents-md` if Task 1 chose regeneration. The skills are `include_str!`d (`src/engine/skills.rs:9-42`), so `cargo build` must run before the install ships the edit -- an install from a stale binary writes the old prose and looks like the edit did not take.
7. Add or extend a test asserting the shipped router prose no longer contains `UNION` or `parent-child rule`. `src/cli/skills.rs` already has a test module; a grep-style assertion over `EMBEDDED_SKILLS` is what makes AC1 verifiable rather than reviewable.

## Out of scope

- The standing `<NEVER>` line in the other six skills (AC2) -> ITERATION-399. This slice fixes `/lazy`'s copy of it and leaves six identical copies standing, which is deliberate: they are one edit applied six times and belong in one sweep.
- Describing `parent_type` (AC3) -> ITERATION-400. This slice removes `parent_type` from the *boundary* sentences; what it positively means is the next slice's sentence to write.
- Naming every permitted target type in the boundary report (AC4) -> ITERATION-401. The report template at `:157-159` still says `<child-type>`, singular.
- The gate prose (AC5) -> ITERATION-402. `:80` and `:157` still say "even when a `require_parent_status` gate is already satisfied", naming a field ITERATION-381 deleted.
- Any change to the routing behaviour itself -- what `/lazy` dispatches, when it stops, the authorship ceiling rules. Only the source of the boundary changes.
- `src/engine/prompt.rs`'s `child_types_for` -> ITERATION-373. This slice makes the prose agree with it; it does not touch it.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: the prose's job is to name the command and the key an agent reads, not to describe the DAG in words. `writing-skills` governs the edit: an instruction is verifiable when it names a command and a field, and unverifiable when it names a concept.

## Verification

`grep -n 'UNION\|parent-child rule' skills/lazy/SKILL.md AGENTS.md` finds nothing. `cargo build && lazyspec skills install --runtime claude` into a scratch project, then read `.claude/skills/lazy/SKILL.md`: its boundary paragraph names `edges` and the jq in it, run against this repo's `config --json`, returns the rows that name `iteration` on either end.
