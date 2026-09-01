---
title: Retire the union instruction from the standing skill rules
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-262
---

## Objective

The standing rule every verb skill repeats -- "Respect the configured `parent_type` chain and `rules`" -- names the edge table instead, in `/execute` and in the five other skills carrying the same line verbatim.

## Satisfies

STORY-262 AC2. Beyond it, the five identical copies of the same line in `/advance`, `/co-write`, `/generate`, `/review` and `/scaffold`, which carry no AC: AC2 names `/execute` and the line is character-for-character the same in six files, so fixing one is the drift the story exists to remove. AC1 landed in ITERATION-398; AC3 lands in ITERATION-400, AC4 in ITERATION-401, AC5 in ITERATION-402.

## Context

- Story + ACs: STORY-262
- The wording and the AGENTS.md regeneration decision: ITERATION-398, which settles both. Do not re-open either
- Touch, all the same line:
  - `skills/execute/SKILL.md:23` -- the one AC2 names
  - `skills/advance/SKILL.md:30`, `skills/co-write/SKILL.md:18`, `skills/generate/SKILL.md:18`, `skills/review/SKILL.md:18`, `skills/scaffold/SKILL.md:18`
- Also touch:
  - `skills/README.md:9` and `:31` -- both describe `/lazy` as stopping at type boundaries without saying where a boundary comes from, so both survive AC1 unchanged and are the natural place to name the table once for the whole set
  - `skills/MIGRATION-2026-06-23.md:31` and `:48` -- the same, in the migration note that maps the old skill names onto the new ones
  - `src/engine/skills.rs:9-42` `EMBEDDED_SKILLS` -- all six of these files are embedded, so the test surface from ITERATION-398 Task 7 covers them
- **`/execute` has one boundary sentence and it is that line.** Unlike `/lazy`, `/execute` never derives a boundary: it confirms the document's type is a delivery type (`skills/execute/SKILL.md:14`, `:74`) and works the breakdown. So AC2's "the same holds" resolves to a single-line edit plus the honest observation that there was never a union rule in `/execute` to remove -- only an inherited standing rule that pointed at the wrong table. Record that, because a reader expecting AC2 to be the size of AC1 will look for something that is not there.
- **`rules` is the load-bearing half of the line, not `parent_type`.** The line couples two things: a containment field and a constraint table. After ITERATION-385 the second does not exist. The replacement names `edges` for the constraint and drops `parent_type` from the sentence entirely -- ITERATION-400 gives `parent_type` its own sentence, in the places that should describe it, which is not a standing `<NEVER>` rule.
- **Six identical lines will not stay identical unless something checks.** They are six files with no shared source; the plugin ships them from `skills/` and the binary embeds them from the same path (`src/engine/skills.rs:9-42`). A test that asserts the line is present and identical across `EMBEDDED_SKILLS` is what stops the seventh skill from being added with the old wording. Note that `skills/configure-type/SKILL.md` and `skills/create-audit/SKILL.md` are *not* in `EMBEDDED_SKILLS`, so such a test would not cover them -- and ITERATION-400 and ITERATION-402 both edit `configure-type`.

## Tasks

1. Rewrite the line once, in `/execute`, to the wording ITERATION-398 settled.
2. Apply it verbatim to the other five. One sweep, one wording; a per-file variation defeats the point.
3. Name the edge table in `skills/README.md:9` and `:31`, and in `skills/MIGRATION-2026-06-23.md:31` and `:48`. Both are read by humans deciding which skill to invoke, so both are places the old model would survive the story.
4. Add the identical-across-skills assertion from Context to `src/cli/skills.rs`'s test module, and state in its name that `configure-type` and `create-audit` are outside its reach.
5. Regenerate AGENTS.md by the route ITERATION-398 chose, after `cargo build`, and diff it: six one-line changes and nothing else. Any other movement in that diff is the divergence ITERATION-398 resolved reappearing.

## Out of scope

- `/lazy`'s copy of the line -> ITERATION-398, which must land first and owns the wording.
- What `parent_type` positively means (AC3) -> ITERATION-400. This slice deletes it from six sentences and defines it in none.
- The gate language in `/advance` (AC5) -> ITERATION-402. `skills/advance/SKILL.md:52` and its "Gates and the type boundary" section at `:56-60` are untouched here, so between the two slices `/advance` names the edge table in its standing rules and `require_parent_status` in its workflow. That is one slice of incoherence and it is chosen, because the gate prose is a deletion with a replacement to design and this is a find-and-replace.
- The boundary report's shape (AC4) -> ITERATION-401.
- `skills/systematic-debugging/SKILL.md`. It carries no boundary line -- its `:78` "component boundary" is about code, not document types. Confirm rather than assume before sweeping.
- Adding the standing line to `configure-type` or `create-audit`, which do not carry it. No AC, and neither ships through `skills install`.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: one wording with six occurrences and a test pinning them together, rather than six independently maintained sentences. `writing-skills`: a `<NEVER>` line is a rule an agent is expected to check, so it must name a field the agent can read.

## Verification

`grep -rn 'parent_type. chain and .rules' skills/ AGENTS.md` finds nothing. `grep -rc 'edges' skills/*/SKILL.md` shows the new line in all six. `cargo test` passes the new identical-wording assertion.
