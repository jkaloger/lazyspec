---
title: CLI expand-references flag and search fix
type: iteration
status: accepted
author: jkaloger
date: 2026-03-12
tags: []
related:
- implements: docs/stories/STORY-058-ref-expansion-hardening-and-performance.md
---



## Changes

### Task Breakdown

1. **Split `get_body` into raw and expanded variants**
   - Rename current `Store::get_body` to `Store::get_body_expanded`. This calls `RefExpander::expand` (from iteration 1).
   - Add `Store::get_body_raw` that reads the file and extracts the body without ref expansion.
   - Keep `get_body` as a convenience alias for `get_body_raw` (the safe default). Callers that want expansion must explicitly call `get_body_expanded`.
   - File: `src/engine/store.rs`

2. **Fix search to use raw body**
   - `Store::search()` at line 451 calls `get_body()` for snippet extraction. Change to `get_body_raw()`. Search should never trigger git commands.
   - File: `src/engine/store.rs`

3. **Add `-e`/`--expand-references` flag to `show` CLI command**
   - Add a `#[arg(short = 'e', long = "expand-references")]` flag to the show command's clap struct.
   - In `show::run()` and `show::run_json()`: call `get_body_expanded` when `-e` is set, `get_body_raw` otherwise.
   - Update the function signatures to accept the flag.
   - File: `src/cli/show.rs`, CLI arg structs (find with `grep "show" src/cli/mod.rs` or similar)

4. **Update existing tests**
   - Update `tests/expand_refs_test.rs` tests that call `show::run_json` to pass the expand flag explicitly.
   - Add a new test: `test_show_without_expand_flag_shows_raw_refs` -- create a doc with `@ref`, call `show::run_json` without `-e`, assert the output still contains the raw `@ref` directive.
   - File: `tests/expand_refs_test.rs`

5. **Update README**
   - Document the `-e`/`--expand-references` flag under the `show` command section.
   - Add a section on `@ref` syntax: `@ref <path>[#symbol][@sha]`.
   - Include examples of expanded output.
   - File: `README.md`

## Test Plan

| AC | Test | Validates |
|----|------|-----------|
| Default show is raw | `test_show_without_expand_flag_shows_raw_refs` | `-e` not set means raw output |
| `-e` shows expanded | Existing `test_cli_show_expands_ref_to_code_block` (updated to pass flag) | Flag triggers expansion |
| Search uses raw body | `test_search_does_not_expand_refs` (new) | No git calls during search |
| Full suite | `cargo test` passes | Nothing broken |

## Notes

- Depends on ITERATION-053 (RefExpander extraction).
- The `get_body` -> `get_body_raw` rename is the key change. Every existing caller of `get_body` gets the safe default (no expansion). Only explicit opt-in triggers git+tree-sitter.
- The TUI will call `get_body_expanded` in iteration 3, but through the async path.
