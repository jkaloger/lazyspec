---
title: Report the unsatisfied edge, not a gate that no longer exists
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-262
---

## Objective

No skill promises a status-conditioned gate on a create: `/advance` scopes the word "gate" to the lifecycle edge it still means, `/lazy` reports the unsatisfied edge from `validate --json` instead of a cleared gate, and `configure-type` stops interviewing for a field and a command that are both gone.

## Satisfies

STORY-262 AC5, by resolving its premise rather than implementing it -- see Context. AC1 landed in ITERATION-398, AC2 in ITERATION-399, AC3 in ITERATION-400, AC4 in ITERATION-401. Last slice on the story.

## Context

- Story + ACs: STORY-262
- **The AC names something the project has decided will not exist.** AC5 says "given a gated edge, when the gate is unmet, then the agent reports the gate rather than proposing a `create` that will be refused". There are no gated edges: ADR-033 abandons status-conditioned create gating outright rather than relocating it onto the edge table, RFC-067 §Design's final bullet says "No edge condition refuses a command. Every unsatisfied edge is a validation finding", and ADR-030's 2026-08-31 amendment says the same. `require_parent_status` and the `create` gate are deleted by ITERATION-381, and commit `40b91f3` already reverted the `require_to_status` successor. So the criterion resolves to its second clause only: the agent must not propose a create against a promise the binary no longer makes, and what it reports instead is the finding. **Amend the story** so the AC stops implying a gate exists. This is the same move ITERATION-387 made for `require_to_status` on STORY-260
- **The word "gate" still means one thing, and it is not this.** A lifecycle edge gates `update --status`: `src/engine/config.rs:503-506` says the transitions are what `update --status` is gated by, and the binary rejects a pair that is not an edge. That is a real refusal, inside one type's lifecycle, and `/advance` is right to check it. The edit is a re-scoping, not a deletion: every cross-type, status-conditioned sense goes, and the within-lifecycle sense stays and gets said clearly
- Touch:
  - `skills/advance/SKILL.md:3` -- the frontmatter `description`, "maintaining links and checking gates at the transition". This string is what a runtime matches on when deciding to load the skill, so it is the highest-leverage sentence in the file
  - `skills/advance/SKILL.md:10` ("confirms the gate on that edge holds"), `:24` (the HARD-GATE clause about a move satisfying "a gate that makes a child creatable"), `:41` ("Read lifecycle and gate facts from the CLI"), `:47` (Preflight step 3, "`context --json` gives the parent and child statuses a gate may depend on"), `:52` (Workflow step 2, which names `require_parent_status` outright), `:56-60` (the whole "Gates and the type boundary" section)
  - `skills/lazy/SKILL.md:80` -- "even when a `require_parent_status` gate is already satisfied"; `:98-104` -- the RED-FLAGS table, whose "Gate is satisfied, so it's automatic" row rests on the same premise; `:157` -- the same clause in Stop-at-Type-Boundary, a paragraph ITERATION-398 and ITERATION-401 have each already rewritten
  - `skills/configure-type/SKILL.md:86` (the parent-status gate interview row), `:143-150` (the `config add-gate` write step), `:159` (the verification jq over `.rules[]`), `:169-170` (the checklist item)
  - `src/engine/validation.rs:196-211` -- `UnsatisfiedEdge`'s `Display`, which is what the agent reports instead
- **`configure-type` instructs an agent to run a command that will not exist.** `:143-150` tells it to run `lazyspec config add-gate <rule-name> --status <s>`; ITERATION-381 removes that subcommand end to end. `:159`'s verification jq reads `.rules[]`, a key ITERATION-385 removes from `config --json`. Both fail loudly rather than silently, which is the good case, but they fail after the agent has already written a type. Remove the step, not just the sentence describing it -- and renumber what follows, since the write sequence is numbered
- **What replaces "eligible".** Under a gate, a child was *ineligible* until the parent reached a status; the report had something to wait for. Under edges there is nothing to wait for -- an edge is always legal to create against -- so the only thing an agent can truthfully report is what `validate --json` says: the edge is required and unsatisfied, at this severity, naming the types that satisfy it (ITERATION-401's report). The prose must not replace one promise with another: "no gate" does not mean "safe to create", it means the stop at the boundary is now the *only* thing standing between the agent and a create, which is what `/lazy`'s HARD-GATE already says
- **`create` can still be refused, for one unrelated reason.** `src/engine/ops/create.rs:283-293` rejects `--parent` across store backends. Do not write "create is never refused" -- it is never refused *on DAG grounds*. One clause, so the next reader does not test the stronger claim and find it false
- ITERATION-381 must land first: prose that says the gate is gone while `config add-gate` still ships is a skill that contradicts `--help`

## Tasks

1. Amend STORY-262 AC5 per Context, before editing any prose. An iteration that satisfies an AC by arguing the AC is wrong has to leave the story readable to the next person.
2. Rewrite `skills/advance/SKILL.md`'s gate language as the lifecycle-edge check it now is: `:3`, `:10`, `:24`, `:41`, `:47`, `:52`. Step 2 at `:52` becomes "the binary rejects a pair that is not an edge" -- which step 3 at `:53` already says, so fold rather than duplicate, and make Preflight step 3 earn its place or drop it (`context --json` has nothing left to contribute to a status move once no gate depends on a parent's status).
3. Delete `skills/advance/SKILL.md:56-60`. Its claim -- that a status move can make a child creatable -- has no mechanism behind it. `/lazy`'s stop-at-boundary rule survives on its own terms and is stated in `/lazy`.
4. Remove the gate clauses from `skills/lazy/SKILL.md:80` and `:157`, and rewrite the RED-FLAGS row at `:98-104`. The row's *point* -- eligibility is not consent -- is still true and still worth keeping; only its premise changes, so rewrite it rather than deleting it and losing the rationalization it catches.
5. Add the replacement to `/lazy`: an edge that `validate --json` reports as unsatisfied is reported as a finding, with the severity `required` gave it, and never as a reason a create would be refused. One paragraph, in the section ITERATION-401 left the report in.
6. Remove `configure-type`'s gate row (`:86`), write step (`:143-150`, renumbering), verification jq (`:159`) and checklist item (`:169-170`).
7. Add the assertion that closes this: no shipped skill mentions `require_parent_status` or `add-gate`. Put it beside ITERATION-399's identical-wording test in `src/cli/skills.rs`, and note there that `configure-type` is outside `EMBEDDED_SKILLS` (`src/engine/skills.rs:9-42`) so the assertion does not reach it.
8. Regenerate AGENTS.md by ITERATION-398's route, after `cargo build`.

## Out of scope

- Reintroducing gating in any form, on the edge table or beside it. ADR-033 abandons it; RFC-067 §Design forbids the table carrying a second policy; ITERATION-381 §Context refuses even a warning-severity consolation finding.
- `require_parent_status`, `config add-gate` and the `create` gate in the code -> ITERATION-381, which must land first. This slice removes the prose that describes them and touches none of them.
- The `<HARD-GATE>` confirm-before-mutating rule in `/lazy`. It is not a DAG gate and nothing on this story reaches it; the word collision is unfortunate and renaming it is not in scope.
- Making `configure-type` author an `[[edges]]` row in place of the removed gate step -> recorded by ITERATION-400 Task 3 as a separate question. This slice deletes; it does not replace.
- Structured `validate --json` issues -> recorded in ITERATION-401 §Out of scope, still no AC.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Convention §Governance: a rule that no longer reflects how the codebase works gets changed, not left in disagreement -- which is this whole slice, and also why Task 1 amends the story. `writing-skills`: a skill that names a command must name one that exists.

## Verification

`grep -rn 'require_parent_status\|add-gate' skills/ AGENTS.md` finds nothing. On a scratch project whose delivery type has a `required` edge with no satisfying link, drive `/lazy` to the boundary: it reports the unsatisfied edge and its severity, proposes no create, and mentions no status a parent must reach. `lazyspec create <child-type> "x"` on the same project succeeds -- the point being that it always would have.
