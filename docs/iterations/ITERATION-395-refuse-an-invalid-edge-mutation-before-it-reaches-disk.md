---
title: Refuse an invalid edge mutation before it reaches disk
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-261
---

## Objective

`add-edge`, `set-edge` and `remove-edge` render, re-parse the exact bytes destined for disk, and on failure exit with `Config::parse`'s own message and an unchanged file -- the protocol the TUI settings screen already runs.

## Satisfies

STORY-261 AC5. AC1, AC4 landed in ITERATION-392, AC2 in ITERATION-393, AC3 in ITERATION-394; AC6 lands in ITERATION-396, AC7 in ITERATION-397.

## Context

- Story + ACs: STORY-261
- The errors this has to surface, and where each lives: `via` absent at `src/engine/config.rs:1307-1319`, unknown type at `:1327-1336`, unknown relationship at `:1337-1345` (all landed); `required` on a wildcard `from` in ITERATION-370; a traversal-role disagreement between overlapping rows in ITERATION-372
- Touch:
  - `src/tui/state/app.rs:1594-1637` `settings_commit_write` -- the protocol to copy, and its comment says why: "Validate the exact bytes destined for disk (catches every field-level and cross-field constraint, plus any `toml_edit` slip), so the file never holds an invalid intermediate"
  - `src/cli/config.rs:138-186` `run_add_type`, `:729-754` `run_set_lifecycle`, and ITERATION-392 / ITERATION-393 / ITERATION-394's three new functions -- all six lines of `let out = write_config_in_place(&src, &config)?; fs.write(&path, &out)?;`
  - `src/main.rs:654-778` -- the dispatch; `spinner::finish_err` then `return Err(e)` is how a mutator already reports failure
- **The CLI mutators write blind, and always have.** `run_add_type` and `run_set_lifecycle` mutate the typed buffer, render, and write, with nothing between the render and the write. The TUI's identical operation re-parses first. So AC5 is not "add validation" -- every error it names is already in the loader -- it is "give the CLI the guard the TUI has". That makes this a wiring slice, and nothing in it should format an edge error.
- **The pre-existing half of the hole.** `--parent-type nonsense` on `config add-type` writes a config that the next command refuses to load, for exactly the same reason. Extract the render-parse-write step into one helper in `src/cli/config.rs` and route all five mutators through it, not only the three this story adds. Fixing three and leaving two is the asymmetry a reader will trip over, and the fix is the same three lines. If a pre-existing test asserts that a bad `add-type` writes anyway, that test encodes the bug -- invert it and say so in the commit.
- **`--json` has no error envelope, on any command.** A refused mutation propagates an `anyhow::Error` out of `main`, which prints to stderr and exits non-zero; the JSON success object from ITERATION-392 is simply never printed. That is the existing convention for every `--json` command in the binary, not a gap this slice invents, so match it: exit non-zero, loader's message on stderr, nothing on stdout. State it in the README's mutator paragraph so an agent parsing stdout knows an empty stdout with a non-zero exit is the refusal.
- **The message must not be re-spelled.** STORY-260 §Notes' "two spellings of the same error is how they drift" governs this slice too, one surface over. Test against `Config::parse(...).unwrap_err().to_string()`, never against a literal -- a literal in the test is the second spelling the AC forbids, written into the guard meant to prevent it.
- The two errors from ITERATION-370 and ITERATION-372 do not exist yet, which is why both are blocking edges. After ITERATION-384 the loader also refuses a surviving `[[rules]]` block -- an error no edge mutation caused. The guard re-parses the whole file, so a config carrying `[[rules]]` makes every edge mutation fail with a message about rules. That is correct (the config is obsolete and `fix --config` is the remedy) but confusing; check whether the read-side `Config::parse` at the top of each mutator already fails first, and if it does, say so rather than adding a second check.

## Tasks

1. Test-first, one table over the five errors AC5 can produce: for each, run the mutation, assert the process error string equals `Config::parse`'s for the same bytes, and assert the file is byte-identical to before.
2. Extract the render-parse-write helper and route all five `config` mutators through it. Its doc comment states that the parse is the guard and names `settings_commit_write` as the surface it mirrors.
3. Test the pre-existing case: `config add-type` with an unknown `--parent-type` is refused and writes nothing. If the loader has no check for an unknown `parent_type` at all, record that here -- the guard surfaces loader errors and adds none, so a missing check is a missing error, not this slice's work.
4. Test that a *valid* mutation still writes, on every one of the five mutators. A guard that also rejects the happy path passes every test in Task 1.
5. Assert the `--json` refusal shape from Context: non-zero exit, empty stdout, loader's message on stderr. One test is enough; the point is to pin the contract, not to enumerate errors again.
6. README: the mutators paragraph at `:508` states that a mutation producing a config that would not load is refused and the file is left untouched, and that `--json` reports the refusal by exit status and stderr.

## Out of scope

- Adding any load-time check. Every error here is the loader's; three of them are other iterations' work and this slice surfaces them.
- The two holes with no error to surface: duplicate edge `name` (ITERATION-388) and an empty target set (ITERATION-387 refuses it in the panel precisely because the loader has no error for it). Neither is reachable through AC5, and the asymmetry between the panel's refusal and the CLI's silence stays until someone gives the loader the check.
- Landing the refusal on a specific flag rather than reporting the loader's message. `settings_jump_to_violation` (`app.rs:1642-1702`) exists because a panel has a cursor to move; a CLI has no cursor and AC5 asks only for the message. Do not build a flag-attribution table here -- ITERATION-389 §Context already records that each such arm is a second implementation of a loader predicate.
- Retrofitting a JSON error envelope across the binary. Worth doing, no AC, and it would change every command's contract.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: validation belongs to the engine's load path; the CLI's job is to run it before writing and print its answer. Dictum 6: one guard with five callers is the indirection this earns. Convention §"CLI Patterns / Output & Errors": `anyhow` context is written for the person reading it, which is why the guard must not rewrite it.

## Verification

On a scratch copy of this repo's config: `lazyspec config add-edge x --from story --to nonsense --via implements` prints `edge "x" names unknown type "nonsense" (not declared in [[types]])`, exits non-zero, and `git diff .lazyspec.toml` is empty. Then `lazyspec config add-type x xs docs/xs X --parent-type nonsense` is refused the same way.
