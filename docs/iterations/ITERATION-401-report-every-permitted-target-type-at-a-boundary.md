---
title: Report every permitted target type at a boundary
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-262
- blocks: ITERATION-402
---

## Objective

`/lazy`'s boundary report names every type the edge permits, each with its own ceiling verb, instead of one `<child-type>` slot -- and the multi-hop report does the same at every hop.

## Satisfies

STORY-262 AC4. AC1 landed in ITERATION-398, AC2 in ITERATION-399, AC3 in ITERATION-400; AC5 lands in ITERATION-402.

## Context

- Story + ACs: STORY-262, and §Notes: "The set-valued reporting criterion matters most. An agent that says 'create a story' when a spike or bug would do is the same failure as a validation message that names one permitted type out of three"
- `to = ["spike","story","bug"]` is one edge, and `required` on it is satisfied by one link to any one member: ADR-030 §Decision; RFC-067 §Design
- Touch:
  - `skills/lazy/SKILL.md:153-161` -- Stop-at-Type-Boundary. The report template at `:159` has one `<child-type>` slot and one `<ceiling-verb>` slot; `:161`'s multi-hop paragraph says "the required parent type", singular, in both the sentence and its worked example
  - `skills/lazy/SKILL.md:151` -- Authorship-aware dispatch. The ceiling is a property of the *type*, so a three-member target set can carry three different ceilings and therefore three different verbs. That is what breaks the single-slot template, and it is not obvious from reading `:159` alone
  - `src/engine/validation.rs:154-160` `to_phrase` and `:196-211` `UnsatisfiedEdge`'s `Display` -- the engine already renders "to one of: spike, story, bug", and "to a document of any type" for `"*"`. This is the executable form of the same criterion, and the prose should read like it rather than inventing a second phrasing
  - `src/cli/validate.rs:65-81` `run_json`
- **The report must read the type set from `config --json`, not from the finding.** `validate --json` serialises every issue as a flat `format!("{}", e)` string (`src/cli/validate.rs:67-68`), so the target set an agent could read from a finding is embedded in prose and has to be string-parsed out. `config --json`'s `edges[].to` is the same set, structured, and is what `to_phrase` was rendered from. Tell the agent to learn *that* an edge is unsatisfied from `validate --json` and *which types satisfy it* from `config --json`. Say that explicitly, because the finding string looks like the more direct source and is the wrong one.
- **`"*"` has no list to enumerate.** A row with `to = "*"` permits every declared type, so the report cannot name "every permitted target type" by reading the row. The engine's answer is a different sentence -- "to a document of any type" (`validation.rs:156-158`) -- and the prose needs the same branch: enumerate when the row names types, and point at `types[]` when it does not. A report that expands `"*"` into the full type list is worse than one that says "any type": it presents eleven equally-weighted options as if the config had chosen them.
- **The ceiling verb is per-type, so the report is a list, not a sentence with a list in it.** `human -> /scaffold`, `assisted -> /co-write`, `generated -> /generate` (`skills/lazy/SKILL.md:151`). With `to = ["spike","story","bug"]` and three different ceilings, one line per permitted type is the only shape that carries the right verb with each. Rewrite the template accordingly and keep it a template -- the "with every value read from config + status for that run" line at `:163` is what stops it becoming an example an agent copies verbatim.
- **Multi-hop compounds.** If the nearest hop permits three types and the hop above permits two, the honest report is a chain of choices, not a chain of names. Decide how deep the enumeration goes -- naming the alternatives at the nearest hop and the chain beyond it is defensible and short; enumerating the cross-product is not -- and write the answer into the paragraph rather than leaving the agent to choose per run.
- This slice rewrites a block ITERATION-398 already rewrote and ITERATION-402 will rewrite again (`:157` still names `require_parent_status`). Three slices editing one paragraph is deliberate: each is a separate claim about what the report says, and merging them would make one unreviewable edit.

## Tasks

1. Rewrite the report template at `skills/lazy/SKILL.md:159` to one line per permitted target type, each carrying that type's ceiling verb, with the `"*"` branch from Context as a second template beside it.
2. Rewrite the Stop-at-Type-Boundary sentence at `:155` so "a child of a different type" reads as a set: the boundary is the edge, and the choice among its `to` members belongs to the human.
3. Add the two-command instruction from Context -- `validate --json` for the fact, `config --json` for the type set -- to Preflight (`:113-119`) or to this section, wherever ITERATION-398 left the read instructions, and not to both.
4. Resolve the multi-hop depth question from Context and write the answer into `:161`, replacing its singular "the required parent type".
5. Align the wording with `to_phrase` (`src/engine/validation.rs:155-160`). If the prose and the finding phrase the same set two ways, the agent will report a third. Pin it with a comment in `to_phrase` naming the skill, or with a test asserting the phrases match -- pick one and say why the other was not enough.
6. Regenerate AGENTS.md by ITERATION-398's route, after `cargo build`.

## Out of scope

- Making `validate --json` emit structured issues. It would be the better fix for the reading problem in Context and it changes every consumer's contract; no AC on this story reaches it. Record it and route around it.
- Changing which type `/lazy` recommends. AC4 is about offering the real choice, not about choosing well. A ranking heuristic over the permitted set is the opposite of what the story asks for.
- The gate clause at `:157` (AC5) -> ITERATION-402, which edits the same paragraph after this one.
- `parent_type` (AC3) -> ITERATION-400, which must land first: `:155` still names it until then.
- The TUI's and the CLI's own renderings of a target set. `to_phrase` already names every member; nothing on this story asks for a second surface.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: the report is assembled from `config --json` and `validate --json`, so the prose names commands and keys. Dictum 6: one phrasing of a target set, shared between the finding and the skill, rather than one per reader.

## Verification

On a scratch project with `to = ["spike","story","bug"]` on the delivery edge and three different `authorship` ceilings among those types, drive `/lazy` to the boundary: the report lists three lines, one verb each. Change the row to `to = "*"` and the report says "any type" and points at `types[]` rather than listing eleven names.
