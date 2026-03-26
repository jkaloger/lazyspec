---
title: Spec ref validation rules
type: iteration
status: accepted
author: agent
date: 2026-03-24
tags: []
related:
- implements: STORY-088
---




## Context

STORY-088 defines three validation rules for spec `@ref` directives (ref count ceiling, cross-module advisory, orphan ref) and a config knob for the ceiling. The store does not track `@ref` directives -- they exist only as raw text in document bodies. Validation checkers will parse refs from file content using the existing `REF_PATTERN` regex in `src/engine/refs.rs`, following the same `read_body` pattern established by `AcSlugFormatRule`.

## ACs Addressed

- `validate-ref-count-ceiling` -- warn when spec has >15 (or configured ceiling) `@ref` targets
- `validate-cross-module-advisory` -- warn when refs span >3 distinct modules
- `validate-orphan-ref` -- warn when ref target can't be found at HEAD
- `ref-ceiling-configurable` -- `.lazyspec.toml` config for the ceiling value

## Changes

### Task 1: Add `ref_count_ceiling` config field

ACs addressed: `ref-ceiling-configurable`

Files:
- Modify: `src/engine/config.rs`
- Modify: `.lazyspec.toml`

What to implement:

Add a `ref_count_ceiling: Option<usize>` field to `RawConfig` (and propagate to `Config`) with a default of `None` (meaning use the hardcoded default of 15). When present in TOML, it overrides the default. The field lives at the top level of the TOML, e.g.:

```toml
ref_count_ceiling = 20
```

In `Config`, add `pub ref_count_ceiling: usize` (resolved from `RawConfig` with fallback to 15 in `Config::parse()`). This follows the pattern of how `rules` is populated from `RawConfig` with a default.

How to verify:
- `cargo test --test config_test` passes
- Parsing a TOML with `ref_count_ceiling = 20` produces `config.ref_count_ceiling == 20`
- Parsing a TOML without it produces `config.ref_count_ceiling == 15`

### Task 2: Ref-counting validation rules

ACs addressed: `validate-ref-count-ceiling`, `validate-cross-module-advisory`

Files:
- Modify: `src/engine/validation.rs`

What to implement:

Add a new checker `RefScopeRule` that fires for spec `index.md` documents (check `doc_type == DocType::SPEC` and filename is `index.md` or path has no parent in the store). For each matching document:

1. Read the body using `store.root().join(path)` + `std::fs::read_to_string` + `DocMeta::extract_body` (same pattern as `AcSlugFormatRule::read_body`)
2. Parse `@ref` directives using `refs::REF_PATTERN` regex. Collect the path portion (capture group 1) of each match.
3. Count total unique ref paths. If count exceeds `config.ref_count_ceiling`, emit `Severity::Warning` with a `ValidationIssue::RefCountExceeded { path, count, ceiling }` variant.
4. Extract the "module" from each ref path: the first two path components (e.g. `src/engine` from `src/engine/store.rs`). If refs span >3 distinct modules, emit `Severity::Warning` with a `ValidationIssue::CrossModuleRefs { path, module_count }` variant.

Add both new `ValidationIssue` variants with `Display` implementations. Register `RefScopeRule` in `default_checkers()`.

How to verify:
- A spec with 14 refs produces no warning
- A spec with 16 refs (default ceiling) produces `RefCountExceeded`
- A config with `ref_count_ceiling = 5` and a spec with 6 refs produces `RefCountExceeded`
- A spec with refs in 4+ modules produces `CrossModuleRefs`
- A spec with refs in 2 modules produces no cross-module warning

### Task 3: Orphan ref validation rule

ACs addressed: `validate-orphan-ref`

Files:
- Modify: `src/engine/validation.rs`

What to implement:

Add a new checker `OrphanRefRule` (or extend `RefScopeRule` with a second pass) that fires for spec documents. For each `@ref` directive found in the body:

1. Check if the ref path exists on disk: `store.root().join(ref_path).exists()`
2. If the file does not exist, emit `Severity::Warning` with a `ValidationIssue::OrphanRef { path, ref_target }` variant.

This is a filesystem check, not a git check. Checking HEAD via `git show` would be more correct but slower and requires shelling out. Filesystem check is sufficient for the common case (deleted files) and avoids the complexity. Note this in `## Notes`.

Add the `OrphanRef` variant to `ValidationIssue` with `Display`. Register in `default_checkers()` (or fold into `RefScopeRule` if it makes sense to share the ref-parsing pass).

How to verify:
- A spec with `@ref src/engine/store.rs` (exists) produces no warning
- A spec with `@ref src/nonexistent.rs` produces `OrphanRef`
- A non-spec document with broken refs does NOT trigger this checker

## Test Plan

### test: ref count below ceiling produces no warning
Create a spec `index.md` fixture with 14 `@ref` directives. Run validation with default config. Assert no `RefCountExceeded` issue. Behavioural, isolated, fast.

### test: ref count above ceiling produces warning
Create a spec `index.md` with 16 `@ref` directives. Run validation with default config (ceiling=15). Assert `RefCountExceeded` with correct count and ceiling. Behavioural, isolated, fast.

### test: configurable ceiling overrides default
Create a spec with 6 `@ref` directives. Run validation with `ref_count_ceiling = 5`. Assert `RefCountExceeded`. Run again with `ref_count_ceiling = 10`. Assert no warning. Behavioural, isolated, fast.

### test: refs in 3 or fewer modules produce no cross-module warning
Create a spec with refs targeting `src/engine/store.rs`, `src/engine/config.rs`, `src/cli/create.rs` (3 modules: `src/engine`, `src/cli`... actually 2). Assert no `CrossModuleRefs`. Behavioural, isolated, fast.

### test: refs spanning more than 3 modules produce advisory
Create a spec with refs in 4+ distinct module prefixes. Assert `CrossModuleRefs` with correct module count. Behavioural, isolated, fast.

### test: orphan ref produces warning
Create a spec with `@ref src/nonexistent.rs`. Assert `OrphanRef` issue. Behavioural, isolated, fast.

### test: valid ref produces no orphan warning
Create a spec with `@ref` pointing to a file that exists in the fixture. Assert no `OrphanRef`. Behavioural, isolated, fast.

### test: non-spec documents skip ref validation
Create an RFC or Story with `@ref` directives. Assert no ref-related validation issues. Behavioural, isolated, fast.

### test: config parses ref_count_ceiling from TOML
Parse a `.lazyspec.toml` with `ref_count_ceiling = 20`. Assert `config.ref_count_ceiling == 20`. Behavioural, isolated, fast.

### test: config defaults ref_count_ceiling to 15
Parse a `.lazyspec.toml` without `ref_count_ceiling`. Assert `config.ref_count_ceiling == 15`. Behavioural, isolated, fast.

## Notes

Orphan ref checking uses filesystem existence (`path.exists()`) rather than `git show HEAD:<path>`. This catches the common case (file was deleted or renamed) without requiring git subprocess calls during validation. A future iteration could add git-aware checking if needed, but filesystem check is the right default for a validation rule that runs frequently.
