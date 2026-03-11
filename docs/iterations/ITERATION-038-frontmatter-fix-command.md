---
title: Frontmatter fix command
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-045-frontmatter-fix-command.md
---



## Changes

### Task 1: Add Fix subcommand definition and dispatch

**ACs addressed:** AC-1, AC-3, AC-4, AC-5

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`
- Create: `src/cli/fix.rs`

**What to implement:**

Add `pub mod fix;` to `src/cli/mod.rs` (alongside the other module declarations).

Add the `Fix` variant to the `Commands` enum in `src/cli/mod.rs`:

```rust
/// Fix documents with broken or incomplete frontmatter
Fix {
    /// Document paths to fix (fixes all broken docs if none given)
    #[arg()]
    paths: Vec<String>,
    /// Show what would change without writing
    #[arg(long)]
    dry_run: bool,
    /// Output as JSON
    #[arg(long)]
    json: bool,
},
```

Add the match arm in `src/main.rs`:

```rust
Some(Commands::Fix { paths, dry_run, json }) => {
    let store = Store::load(&cwd, &config)?;
    let exit_code = lazyspec::cli::fix::run(&cwd, &store, &config, &paths, dry_run, json);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}
```

Create `src/cli/fix.rs` with the core fix logic:

```rust
pub fn run(root: &Path, store: &Store, config: &Config, paths: &[String], dry_run: bool, json: bool) -> i32
```

The function should:

1. Determine which files to fix:
   - If `paths` is non-empty, use those paths directly
   - If `paths` is empty, use `store.parse_errors()` to get all broken file paths

2. For each file path, call a `fix_file(root, config, path, dry_run)` function that returns a `FixResult`:

@ref src/cli/fix.rs#FixResult@febc9353350b931358067b354a6fa96070e14c3d

3. The `fix_file` function:
   - Read the file content
   - Try `split_frontmatter()`. If it fails (no frontmatter), wrap the entire content with a generated frontmatter block.
   - Parse the YAML as `serde_yaml::Value` (loose parse)
   - Check each required field. If missing, insert the default:
     - `title`: derive from filename using slug-to-title (strip type prefix and number, e.g. `RFC-001-my-doc.md` -> `My doc`)
     - `type`: infer from parent directory by matching against `config.types` (e.g. file in `docs/rfcs/` -> find TypeDef where `dir == "docs/rfcs"` -> `name == "rfc"`)
     - `status`: `"draft"`
     - `author`: run `git config user.name` via `std::process::Command`, fall back to `"unknown"`
     - `date`: today's date as `YYYY-MM-DD` string
     - `tags`: empty sequence `[]`
   - Track which fields were added
   - If `dry_run` is false, write the updated frontmatter back using the same pattern as `rewrite_frontmatter` (format `"---\n{}---\n{}"` with serde_yaml output and body)
   - Return `FixResult`

4. Output results:
   - If `json`: serialize all `FixResult`s as a JSON array
   - If not json: print human-readable lines like `Fixed docs/rfcs/broken.md (added: status, tags)`
   - Return 0 if any files were fixed, 1 if no files needed fixing

**How to verify:**
`cargo check` compiles. `cargo test` passes (existing tests unaffected).

---

### Task 2: Tests

**ACs addressed:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6

**Files:**
- Create: `tests/cli_fix_test.rs`

**What to implement:**

Create `tests/cli_fix_test.rs` with `mod common;` and the following tests:

1. `fix_fills_missing_fields` (AC-1) -- Write an RFC missing `status` and `tags`. Call `fix::run()` with the file path, `dry_run: false`, `json: false`. Re-read the file. Assert frontmatter now contains `status: draft` and `tags: []`. Assert existing fields (`title`, `type`, `author`, `date`) are preserved.

2. `fix_preserves_body` (AC-2) -- Write a doc with missing `status` but with markdown body content `"## Hello\n\nWorld\n"`. Run fix. Re-read file. Assert the body content is unchanged.

3. `fix_dry_run_does_not_write` (AC-3) -- Write a broken doc. Call `fix::run()` with `dry_run: true`. Re-read the file. Assert it's unchanged from the original.

4. `fix_all_broken_docs` (AC-4) -- Write two broken docs (one RFC missing `status`, one story missing `date`). Call `fix::run()` with empty paths vec. Re-read both files. Assert both are now valid (can be parsed by `DocMeta::parse`).

5. `fix_json_output` (AC-5) -- Write a broken doc. Call `fix::run()` with `json: true`, capture stdout or call the underlying function that returns the JSON string. Parse the JSON. Assert it's an array with one entry containing `path`, `fields_added`, and `written` keys.

6. `fix_infers_type_from_directory` (AC-6) -- Write a doc in `docs/rfcs/` that's missing the `type` field. Run fix. Re-read the file. Assert `type: rfc` was added.

For tests that need to capture output, the `run` function should return a `String` for JSON mode (similar to `validate::run_json`). Alternatively, structure `fix.rs` so there's a `run_json()` that returns a String and `run_human()` that returns a String, called from `run()`. This matches the pattern in `validate.rs` and `status.rs`.

**How to verify:**
`cargo test cli_fix`

## Test Plan

| Test | AC | Properties | Notes |
|------|-----|-----------|-------|
| `fix_fills_missing_fields` | AC-1 | Isolated, Behavioral, Specific | Checks defaults are applied, existing fields preserved |
| `fix_preserves_body` | AC-2 | Isolated, Behavioral | Reads file after fix, compares body |
| `fix_dry_run_does_not_write` | AC-3 | Isolated, Deterministic | Compares file content before and after |
| `fix_all_broken_docs` | AC-4 | Isolated, Predictive | Verifies fix-all mode by re-parsing both docs |
| `fix_json_output` | AC-5 | Isolated, Behavioral | Validates JSON schema of output |
| `fix_infers_type_from_directory` | AC-6 | Isolated, Specific | Checks type field matches directory config |

## Notes

The `run` function needs to be structured so tests can inspect the output. Follow the `validate.rs` pattern: have `run()` orchestrate, with `run_json()` returning a String. This avoids capturing stdout in tests.
