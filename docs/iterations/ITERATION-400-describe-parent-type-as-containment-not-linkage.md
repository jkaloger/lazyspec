---
title: Describe parent_type as containment, not linkage
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-262
- blocks: ITERATION-401
---

## Objective

Every place the prose describes `parent_type` says what it does -- the child's documents live in the parent type's directory and share its store backend, and the parent type must be a singleton -- and no place presents it as a reason to link, to create, or to constrain.

## Satisfies

STORY-262 AC3. AC1 landed in ITERATION-398, AC2 in ITERATION-399; AC4 lands in ITERATION-401, AC5 in ITERATION-402.

## Context

- Story + ACs: STORY-262
- `parent_type` is containment, `parent` on a rule was a type constraint, and `Store.parent_of` is resolved path nesting -- three things sharing a name, of which only the latter two concern where a document lives: RFC-067 §Problem, closing paragraph and the surface table's last row
- `parent_type` is untouched by the edge table and documented as containment-only: RFC-067 §Design.1; ADR-030 §Decision
- Touch:
  - `skills/scaffold/SKILL.md:43` -- "**Link to the parent:** if config gives the type a `parent_type` and a parent exists, `lazyspec link <new-id> implements <parent-id>`". This is the sentence AC3 is written against: it turns a containment field into a link instruction, and it hardcodes `implements` in the same breath as telling the reader to "read it, don't bake it"
  - `skills/scaffold/SKILL.md:36`, `skills/co-write/SKILL.md:42`, `skills/generate/SKILL.md:46` -- each tells the agent to read the type's `parent_type` from `config --json` without saying what to do with it, which is how `:43`'s reading spreads
  - `skills/configure-type/SKILL.md:85` -- the `--parent-type` interview row: "Does this live UNDER a parent (e.g. iteration under story)? Creates a parent-child rule." It creates no rule (see below) and its example is a config that fails validation
  - `skills/lazy/SKILL.md:117` and `:137` -- rewritten by ITERATION-398, which removes `parent_type` from the *boundary* sentences and defines it nowhere
  - `src/cli/config.rs:43-45` -- the clap help for `--parent-type`: "Parent type name, gating creation and validation". It gates neither
  - `src/engine/config.rs:488-491` -- the `TypeDef.parent_type` doc comment. Accurate about the store backend, silent about the directory and the singleton requirement
  - `README.md:488` -- the `config add-type` example, `--parent-type rfc`
- **What `parent_type` actually does, with the code that does it.** `src/engine/validation.rs:1121-1155` is the whole of it: the parent type must be a singleton or the config is an error-severity `ParentTypeNotSingleton` finding, and every document of the child type must live under the parent type's `dir` or it is an error-severity `ParentTypeViolation`. `src/engine/ops/create.rs:283-293` refuses a cross-store parent on the sub-issue path. `src/cli/convention.rs:20-25` reads it to find the dictum types under the convention. `src/engine/config_write.rs:88` writes it. Nothing else reads it. There is no linkage, no create gate, no validation of relations -- and no rule is created anywhere.
- **The prose recommends a config that does not validate.** `parent_type` requires a *singleton* parent, so `iteration` under `story` -- `configure-type`'s worked example -- is an error finding, and so is `README.md:488`'s `--parent-type rfc`. The only legal use in this project is `dictum` under `convention` (`src/engine/config.rs:1117`), where the parent genuinely is a singleton. An agent following `configure-type` today produces a config `validate` rejects. That makes AC3 more than a wording fix: the interview row has to stop offering the field for ordinary parent/child modelling and start offering it for the one case it serves. Say which case, and name the singleton requirement in the row.
- **The `--help` text is prose too.** `src/cli/config.rs:43-45` is the string an agent reads when it runs `lazyspec config add-type --help`, which every skill instructs it to do before using an unfamiliar command (`skills/lazy/SKILL.md:111`-adjacent, and the same line in each skill). Leaving "gating creation and validation" there while fixing the skills is the two-spellings failure one layer down. It is a one-line clap doc comment; change it, and the `TypeDef` doc comment beside it.
- The replacement for `scaffold`'s link instruction is the edge table: the type's incoming and outgoing edges say what to link and `via` says with what. That is ITERATION-398's vocabulary, which is why this slice comes after it.

## Tasks

1. Rewrite `skills/scaffold/SKILL.md:43` to link by edge: find the row whose `from` admits this type, use its `via` as the relation name, and target a document of a type its `to` admits. Delete the `implements` fallback -- the row carries the verb and the skill's own next clause already says not to bake it.
2. Replace the bare "read its `parent_type`" clauses in `scaffold:36`, `co-write:42` and `generate:46` with one sentence, identical in all three, stating what the field decides: directory, store backend, and nothing about links.
3. Rewrite `configure-type/SKILL.md:85` per Context: what the field does, that the parent type must be a singleton, that it creates no rule, and that the DAG constraint is a separate `[[edges]]` row. Check whether `configure-type`'s write steps (`:130-150`) should now emit a `config add-edge` call -- that command exists after ITERATION-392 -- and if they should, that is a separate slice, not this one; record which.
4. Fix the clap help at `src/cli/config.rs:43-45` and the `TypeDef.parent_type` doc comment at `src/engine/config.rs:488-491`. Both are one line and both are read more often than the skills.
5. Fix `README.md:488`'s example, which currently demonstrates an error-severity config. Use a shape that validates, or drop `--parent-type` from the example and show it once, correctly, where the field is described.
6. Add a test that a type whose `parent_type` names a non-singleton type produces `ParentTypeNotSingleton`, if `validation.rs` does not already have one. AC3 is a prose criterion, but the reason the prose was wrong is that nothing pinned the constraint anywhere a doc author would see it.
7. Regenerate AGENTS.md by ITERATION-398's route, after `cargo build`. Note that `configure-type` is not in `EMBEDDED_SKILLS` (`src/engine/skills.rs:9-42`), so its edit ships through the plugin only and no regeneration reflects it.

## Out of scope

- Removing `parent_type`, changing the singleton requirement, or relaxing the directory check. RFC-067 §Design.1 keeps the field exactly as it is; only its description changes.
- Making `configure-type` author edges. It is not an embedded skill, it has no AC on this story, and the command it would call only exists after ITERATION-392. Task 3 records the question and stops.
- The boundary derivation (AC1) and the standing rule line (AC2) -> ITERATION-398, ITERATION-399, both of which must land first: this slice writes the sentence they left blank.
- The set-valued report (AC4) -> ITERATION-401; the gate prose (AC5) -> ITERATION-402. `configure-type:86`, `:146`, `:159` and `:169-170` are all gate prose and stay standing after this slice, in the same file this slice edits. Do not tidy them here -- ITERATION-402 removes them with their replacement.
- `Store.parent_of` and `create --parent`, which are the *document*-level nesting that shares the name. RFC-067 §Problem names the collision; renaming anything is not in any story.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: `--help` and the skills are the same contract read by the same reader, so they get the same sentence. `writing-reference-docs` governs the clap help and the README line; `writing-skills` the four skill files.

## Verification

`grep -rn 'parent_type' skills/ AGENTS.md README.md` and read every hit: none proposes a link, a create, or a constraint. `lazyspec config add-type --help` describes containment. On a scratch project, `lazyspec config add-type dictum dictums docs/dictums DICTUM --parent-type convention` then `lazyspec validate` is clean, and the same with a non-singleton parent reports `ParentTypeNotSingleton`.
