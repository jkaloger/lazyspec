---
title: Compare parent-type containment on resolved paths
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-283
reviewed: ead6302e2ac8d5f3cfc0277ede6907ec26112f68
---

## Objective

`validate`'s parent-type containment rule compares resolved paths, so an external-`dir` parent raises no false violation.

## Satisfies

STORY-283 AC8.

## Context

- Story + ACs: STORY-283. RFC-072 Goals, last bullet: no rule assumes a doc path is project-relative.
- **The rule:** `src/engine/validation.rs:1367` `doc.path.starts_with(&parent_type_def.dir)` -- a doc path against a raw config string. After ITERATION-431 an external child's path is absolute and an external parent's `dir` is `../x` or absolute; the string compare fails both ways.
- **Like-for-like:** `store.root.join(&doc.path)` against `doc_root(&store.root, parent_type_def)` (ITERATION-430). `&store.root` is already in scope in this function (`validation.rs:1332`). Join discards root on an absolute doc path; both sides are normalised by construction.
- **`expected_dir` stays the raw `dir`.** `ParentTypeViolation { expected_dir: String }` (`validation.rs:81`, display `:362`); `tests/integration/cli_validate_test.rs:455` asserts `"docs/convention"`. The message is for the human who edits the config spelling.
- The `singleton` gate at `:1350` and the `ParentTypeNotSingleton` arm are untouched.

## Tasks

1. Test-first, `cli_validate_test.rs` beside `parent_type_outside_dir_error` (`:421`): (a) parent singleton with an absolute `dir` in a second `TempDir`, child `parent_type` on it, child doc inside -> zero `ParentTypeViolation`; (b) same with `dir = "../shared"` sibling spelling -> zero; (c) child doc under the project root while the parent is external -> one violation, `expected_dir` equals the raw `dir`.
2. Replace the compare at `:1367` with the resolved pair. `expected_dir` unchanged.
3. `parent_type_inside_dir_no_error` (`:392`) and `parent_type_outside_dir_error` (`:421`) stay green untouched.

## Out of scope

- `create --parent` same-store guard (`src/engine/ops/create.rs:254`) -- RFC-072 git store stories.
- Changing `expected_dir`'s type or adding `resolved_dir` to the violation JSON.
- Any other validation rule reading `doc.path`; none of the others compares against `dir`.

## Principles/conventions

`cargo run -q -- convention`. Principle 3: engine rule, no CLI/TUI change. Principle 6: reuse `doc_root`, no helper for one comparison. DICTUM-004: `TempDir` per test.

## Verification

On this repo, `cargo run -q -- validate --json | jq '[.errors[] | select(.rule=="parent-type-violation")] | length'` is `0` before and after.
