---
title: State what the migration destroys before it applies
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-258
---

## Objective

`fix --config`'s plan names what the rewrite destroys -- the comments attached to each translated `[[rules]]` block, and the `require_parent_status` gate that goes with them -- before anything is written, in both the human and the `--json` output.

## Satisfies

STORY-258 AC7. AC1, AC2, AC3, AC4, AC6, AC8 landed in the preceding iterations; AC5 deferred to the last iteration on this story.

## Context

- Story + ACs: STORY-258
- "Comments attached to a `[[rules]]` block do not survive translation; the migration plan must say so before applying": ADR-032 §Consequences
- Touch:
  - `src/engine/ops/fix/config.rs` -- comments have to be read off the source document's `toml_edit` decor before the rewrite runs; `Config::parse_lenient` has already thrown them away, so the parsed config cannot answer this
  - `src/engine/ops/fix.rs:64-69` -- `ConfigFixResult` is the plan; whatever the human text says has to be a field here first
  - `src/cli/fix/output.rs:72-103` -- `format_config_human` prints "Would add X" / "Added X" per name and otherwise falls back to `"Config already up to date; nothing to add"`, which is false the moment the file is being rewritten rather than added to
  - `README.md:305` (command table) and `README.md:476` -- both describe `fix --config` as append-only and comment-preserving
- `fix --config` has no confirmation step: `run_config` (`src/cli/fix.rs:80-99`) plans and applies in one call. `--dry-run` is therefore the only surface that is genuinely "before anything is applied". Print the warning in both modes and treat the dry-run as the one the AC is about.
- Dictum 2: the human sentence and the JSON field carry the same facts. A warning that exists only in prose is invisible to the agent that runs this command.

## Tasks

1. Test-first: a `[[rules]]` block with a comment line above it, and one with a trailing comment on a key, each produce a named warning in `fix --config --dry-run`; a rules block with no comments produces none. False positives matter here -- a warning on every block trains the reader to ignore it.
2. Collect the attached comments from the source decor, keyed by the rule name so the message can say which block loses what. Prefix decor on the table header and suffix decor on its keys are both "attached"; a comment floating between blocks is not, and the preceding iteration's AC8 test already asserts those survive.
3. Extend `ConfigFixResult` with the plan the rewrite now has to describe: edges written, rules removed, traversal keys removed, comments that will be lost, and gates dropped. Serialised, because `fix --config --json` is the agent's copy of the plan.
4. Rewrite `format_config_human` for the migration lines, keeping the module's existing "Would ..." / past-tense pairing, and replace the "nothing to add" fallback with something true when there is nothing to add *and* nothing to migrate.
5. Report each dropped `require_parent_status` as its own line, naming ADR-033. It changes no finding, so the finding-set iteration cannot catch it -- a `create` gate that silently stops gating is exactly the kind of change the plan exists to disclose.
6. README: `fix --config` is append-only for relationships, rules and lifecycles, and a translating rewrite for the edge migration. State what does not survive, that `--dry-run` shows it first, and fix the one-line description in the command table.

## Out of scope

- The finding-set proof (AC5) -> the last iteration on this story.
- Preserving the comments instead of warning about them. ADR-032 §Consequences accepts the loss; re-opening it is an ADR amendment, not a slice.
- A confirmation prompt on `fix --config`. Nothing in this CLI prompts, and STORY-258 asks for a plan, not a gate.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 2: every fact in the human plan is a field in the JSON. Dictum 3: detection is engine-side, wording is `cli/fix/output.rs`.

## Verification

On a scratch config carrying a commented `[[rules]]` block and a `require_parent_status` (this repo's own config has neither -- `.lazyspec.toml:174-193`), `cargo run -- fix --config --dry-run --json` lists the lost comment and the dropped gate, and the file is unchanged afterwards.
