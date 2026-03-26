---
title: Pin command
type: iteration
status: accepted
author: agent
date: 2026-03-26
tags: []
related:
- implements: STORY-085
---




## Acceptance Criteria

- **pin-command-writes-hashes** — Given a document with unpinned `@ref` directives, when `lazyspec pin <spec-id>` is run, then each ref is resolved at HEAD, blob hashes are computed (normalized for symbol refs, raw for file refs), and `@{blob:hash}` suffixes are written into the directives.
- **pin-command-updates-existing** — Given a document with already-pinned `@ref` directives, when `lazyspec pin <spec-id>` is run, then the existing `@{blob:hash}` suffixes are replaced with freshly computed hashes from HEAD.
- **pin-unresolvable-ref-errors** — Given a document with an `@ref` targeting a symbol that does not exist at HEAD, when `lazyspec pin <spec-id>` is run, then the command reports an error for that ref and does not write a hash for it.

## Changes

### Task 1: Add `Pin` variant to CLI commands enum

**ACs addressed:** pin-command-writes-hashes, pin-command-updates-existing, pin-unresolvable-ref-errors (prerequisite)

**Files:**
- Modify: `src/cli.rs`

**What to implement:**
Add a `Pin` variant to the `Commands` enum with a single positional `id: String` argument (spec path or shorthand ID) and a `--json` flag. Follow the same clap derive pattern used by `Show` and other commands. Add `pub mod pin;` to the module declarations at the top of the file.

**How to verify:**
`cargo run -- help pin` prints the expected usage with `<ID>` argument and `--json` flag.

---

### Task 2: Create `src/cli/pin.rs` — pin command implementation

**ACs addressed:** pin-command-writes-hashes, pin-command-updates-existing, pin-unresolvable-ref-errors

**Files:**
- Create: `src/cli/pin.rs`

**What to implement:**
Implement the pin command logic:

1. Resolve the spec ID to a document path using `resolve_shorthand_or_path` from `src/cli/resolve.rs`.
2. Read the document body from disk (via the store or direct file read).
3. Parse all `@ref` directives from the body using a regex that captures the full ref syntax including optional `@{blob:<hash>}` suffix. The regex must handle both `@ref path#symbol` and `@ref path` forms, with or without existing `@{blob:...}` suffix.
4. For each ref:
   - Resolve the target at HEAD: use `git show HEAD:<path>` to get file content. For symbol refs, extract the symbol using the existing `SymbolExtractor` infrastructure in `src/engine/symbols.rs`.
   - If the target cannot be resolved (file doesn't exist, symbol not found), record an error for that ref and skip it — do not write a hash.
   - For symbol refs: normalize the extracted symbol source (strip comments via tree-sitter, collapse whitespace) then compute hash via `git hash-object --stdin`.
   - For file refs (no `#symbol`): compute hash via `git hash-object --stdin` on raw file content.
5. Rewrite the document body, replacing each successfully resolved ref's text with the updated form including `@{blob:<computed-hash>}`. Already-pinned refs get their existing hash replaced.
6. Write the modified file back to disk. Do not commit.
7. Print a summary: number of refs pinned, number of errors. In `--json` mode, output a JSON object with `pinned` (array of ref targets and hashes) and `errors` (array of ref targets and error messages).

The normalization pipeline (strip comments, collapse whitespace, hash via git hash-object) is expected to come from Iteration 2 (semantic hashing). This task should call those functions. If they are not yet available, define the expected function signatures as trait calls or direct function calls and document the dependency.

**How to verify:**
Create a test spec document with `@ref Cargo.toml` and `@ref src/cli.rs#Commands`, run `cargo run -- pin <spec-id>`, and verify the file is rewritten with `@{blob:<hash>}` suffixes.

---

### Task 3: Wire `Pin` command into `main.rs` dispatch

**ACs addressed:** pin-command-writes-hashes, pin-command-updates-existing, pin-unresolvable-ref-errors (prerequisite)

**Files:**
- Modify: `src/main.rs`

**What to implement:**
Add a match arm for `Commands::Pin { id, json }` in the main dispatch. Load the store and config, then call the pin module's run function. Follow the same pattern as `Commands::Show` — resolve via store, call the appropriate `run` or `run_json` function, print output.

**How to verify:**
`cargo run -- pin ITERATION-114` executes without a "not implemented" panic and either pins refs or reports no refs found.

---

### Task 4: Ref regex update to support `@{blob:hash}` suffix

**ACs addressed:** pin-command-writes-hashes, pin-command-updates-existing

**Files:**
- Modify: `src/engine/refs.rs` (the `REF_PATTERN` constant)

**What to implement:**
Extend `REF_PATTERN` to optionally capture a `@{blob:<hex>}` suffix after the existing ref syntax. The current pattern is:

```
@ref\s+([^#@\s]+)(?:#([^@\s]+))?(?:@([a-fA-F0-9]+))?
```

It needs to additionally match the `@{blob:<hex>}` form. Add a new capture group for the blob hash. The existing `@<sha>` capture (group 3) is the short-SHA for expansion display; the blob hash is a separate concept. The updated pattern should capture both forms.

Note: this must not break existing ref expansion behavior. The `RefExpander` in `resolve.rs` uses groups 1-3. The new blob group should be an additional optional group.

**How to verify:**
Unit tests on the updated regex: `@ref src/foo.rs#Bar@{blob:abc123}` captures path=`src/foo.rs`, symbol=`Bar`, blob_hash=`abc123`. Unpinned refs still parse correctly. Existing tests in `src/engine/refs.rs` continue to pass.

## Test Plan

### Tests for AC: pin-command-writes-hashes

**Test: pin writes blob hash for file ref**
- Set up a git repo with a known file at HEAD.
- Create a spec document containing `@ref <file>` (no hash suffix).
- Run the pin command.
- Assert the file is rewritten to `@ref <file>@{blob:<expected-hash>}` where expected-hash matches `git hash-object <file>`.

**Test: pin writes blob hash for symbol ref**
- Set up a git repo with a Rust file containing a known struct.
- Create a spec document containing `@ref src/foo.rs#MyStruct`.
- Run the pin command.
- Assert the file is rewritten with `@{blob:<hash>}` suffix where hash is the semantic hash of the normalized struct body.

**Test: pin handles multiple refs in one document**
- Create a spec with three `@ref` directives (mix of file and symbol refs).
- Run pin.
- Assert all three are rewritten with blob hashes.

**Test: pin does not modify non-ref content**
- Create a spec with prose, headings, and one `@ref`.
- Run pin.
- Assert only the `@ref` line changed; all other content is byte-identical.

### Tests for AC: pin-command-updates-existing

**Test: pin replaces existing blob hash with fresh hash**
- Create a spec with `@ref src/foo.rs@{blob:oldoldold}`.
- Run pin.
- Assert the suffix is replaced with the current hash from HEAD, not `oldoldold`.

**Test: pin replaces existing blob hash on symbol ref**
- Create a spec with `@ref src/foo.rs#Bar@{blob:stale123}`.
- Run pin.
- Assert the suffix is replaced with the freshly computed semantic hash.

### Tests for AC: pin-unresolvable-ref-errors

**Test: pin reports error for nonexistent file ref**
- Create a spec with `@ref nonexistent/path.rs`.
- Run pin.
- Assert the command reports an error mentioning `nonexistent/path.rs`.
- Assert the ref line is not modified (no hash is written).

**Test: pin reports error for nonexistent symbol ref**
- Create a spec with `@ref src/real_file.rs#NoSuchSymbol`.
- Run pin.
- Assert the command reports an error mentioning `NoSuchSymbol`.
- Assert the ref line is left unchanged.

**Test: pin errors do not block other refs**
- Create a spec with one valid ref and one unresolvable ref.
- Run pin.
- Assert the valid ref gets a hash written, the unresolvable ref is unchanged, and the error is reported.

**Test: pin JSON output includes errors**
- Run pin with `--json` on a spec with an unresolvable ref.
- Assert the JSON output contains an `errors` array with the ref target and error message.

## Notes

This iteration depends on Iteration 2 (semantic hashing pipeline) for the normalization functions. The pin command calls into the normalize + `git hash-object` pipeline. If Iteration 2 is not yet merged, the pin module should define the expected interface and use direct `git hash-object --stdin` calls for file-level refs as a baseline, with normalization wired in once available.

The `REF_PATTERN` regex update (Task 4) must be coordinated with Iteration 1 (ref model), which may also be modifying the regex to parse `@{blob:hash}`. If Iteration 1 lands first, Task 4 becomes a no-op or a small adjustment.
