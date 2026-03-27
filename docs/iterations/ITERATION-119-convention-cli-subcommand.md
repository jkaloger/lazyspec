---
title: Convention CLI Subcommand
type: iteration
status: accepted
author: agent
date: 2026-03-27
tags: []
related:
- implements: docs/stories/STORY-095-convention-cli-subcommand.md
---



## Changes

### Task 1: Convention command definition and wiring

**ACs addressed:** (foundational -- enables all ACs)

**Files:**
- Modify: `src/cli.rs` (add Convention variant to Commands enum)
- Create: `src/cli/convention.rs` (new module)
- Modify: `src/main.rs` (add dispatch case)

**What to implement:**

Add a `Convention` variant to the `Commands` enum in `src/cli.rs`:

```rust
/// Show convention and dictum content
Convention {
    /// Show only the convention preamble (no dictum)
    #[arg(long)]
    preamble: bool,
    /// Filter dictum by tags (comma-separated, OR logic)
    #[arg(long)]
    tags: Option<String>,
    /// Output as JSON
    #[arg(long)]
    json: bool,
},
```

Create `src/cli/convention.rs` with the module declaration added to `src/cli.rs` (follow existing pattern: `pub mod convention;` alongside `pub mod show;` etc).

Add dispatch in `src/main.rs` following the Context command pattern:

```rust
Some(Commands::Convention { preamble, tags, json }) => {
    let store = Store::load(&cwd, &config)?;
    if json {
        let output = lazyspec::cli::convention::run_json(&store, &config, preamble, tags.as_deref())?;
        println!("{}", output);
    } else {
        let output = lazyspec::cli::convention::run_human(&store, &config, preamble, tags.as_deref())?;
        print!("{}", output);
    }
}
```

**How to verify:**
```
cargo check
```

### Task 2: Convention command logic and output formatting

**ACs addressed:** default-invocation, preamble-only, tags-single, tags-comma-or, preamble-precedence, no-convention-error, convention-no-dictum

**Files:**
- Modify: `src/cli/convention.rs`
- Test: `tests/cli_convention_test.rs`

**What to implement:**

The convention command needs to:

1. Find the convention document. Iterate `config.documents.types` for a type with `singleton == true` (the convention type). Then query `store.list()` with that type to find the document. If no singleton type exists or no document of that type is found, return an error: "no convention found".

2. Find dictum. Look for types with `parent_type` matching the convention type's name. Query `store.list()` for documents of those types. These are the dictum.

3. Apply filters:
   - If `--preamble` is set, skip dictum entirely (preamble takes precedence over --tags)
   - If `--tags` is set, parse comma-separated values and filter dictum to those matching any tag (OR logic). Use `doc.tags.iter().any(|t| requested_tags.contains(t))`
   - If neither flag, return preamble + all dictum

4. Human-readable output (`run_human`):
   - Print convention body (from `store.get_body_raw(&convention.path)`)
   - For each matching dictum, print a heading separator (`## <dictum title>`) followed by its body
   - If no dictum match (empty after filtering), just print preamble

5. JSON output (`run_json`):
   - Return `{ "convention": { ...doc_to_json fields, "body": "..." }, "dicta": [ { ...doc_to_json fields, "body": "..." }, ... ] }`
   - Use `doc_to_json()` from `src/cli/json.rs` for metadata, add `body` field separately via `store.get_body_raw()`

**How to verify:**
```
cargo test --test cli_convention_test
```

## Test Plan

### Tests in `tests/cli_convention_test.rs`

All tests use `TestFixture` from `tests/common/mod.rs`. Each test creates convention and dictum documents by writing files directly with `fixture.write_subfolder_doc()` and `fixture.write_child_doc()`, then loading the store. Tests need a custom config with convention (singleton) and dictum (parent_type) types.

A shared helper `convention_config(fixture: &TestFixture) -> Config` builds a config with convention and dictum types added to the defaults.

- `default_invocation_returns_preamble_and_all_dictum`: Create convention with 2 dictum. Call `run_human()` with no flags. Assert output contains convention body and both dictum titles and bodies.

- `preamble_only_returns_convention_index`: Create convention with dictum. Call `run_human()` with `preamble=true`. Assert output contains convention body, does not contain dictum titles.

- `tags_single_filters_dictum`: Create 2 dictum, one tagged "testing", one tagged "architecture". Call `run_human()` with `tags=Some("testing")`. Assert output contains testing dictum, does not contain architecture dictum.

- `tags_comma_or_logic`: Create 3 dictum: testing, architecture, errors. Call `run_human()` with `tags=Some("testing,architecture")`. Assert output contains testing and architecture dictum, does not contain errors dictum.

- `json_output_structured`: Create convention with dictum. Call `run_json()`. Parse output as JSON. Assert `convention.title`, `convention.body` exist. Assert `dicta` is an array with expected entries.

- `preamble_takes_precedence_over_tags`: Call `run_human()` with `preamble=true, tags=Some("testing")`. Assert output contains convention body, does not contain any dictum.

- `no_convention_returns_error`: Load store with no convention document. Call `run_human()`. Assert error contains "no convention found".

- `convention_with_no_dictum_returns_preamble`: Create convention with no child dictum. Call `run_human()`. Assert output contains convention body, no dictum separators.

All tests are isolated (TestFixture creates temp dirs), deterministic, behavioral (test output content not internal state), and specific (each tests one AC).

## Notes

The convention command discovers the convention type dynamically from config rather than hardcoding "convention". It looks for any type with `singleton == true`. This keeps the command generic enough that it works with custom type names (e.g. if a user names their singleton type "manifesto" instead of "convention"). Similarly, dictum are found by looking for types with `parent_type` pointing to the singleton type.
