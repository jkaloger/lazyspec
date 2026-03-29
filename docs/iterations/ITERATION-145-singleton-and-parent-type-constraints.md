---
title: Singleton and Parent Type Constraints
type: iteration
status: accepted
author: agent
date: 2026-03-27
tags: []
related:
- implements: docs/stories/STORY-094-singleton-and-parent-type-constraints.md
---



## Changes

### Task 1: Add `singleton` and `parent_type` fields to TypeDef

**ACs addressed:** singleton-config-deserialization, parent-type-config-deserialization

**Files:**
- Modify: `src/engine/config.rs`
- Test: `tests/config_test.rs`

**What to implement:**

Add two fields to `TypeDef` (around line 77):

```rust
#[serde(default)]
pub singleton: bool,
#[serde(default)]
pub parent_type: Option<String>,
```

Update `build_type_def()` (around line 202) to set both fields:
- `singleton: false`
- `parent_type: None`

No changes to `default_types()` needed yet (convention/dictum types are added in Story 3).

**How to verify:**
```
cargo test --test config_test
```

### Task 2: Singleton create guard

**ACs addressed:** singleton-create-guard

**Files:**
- Modify: `src/cli/create.rs`
- Modify: `src/main.rs` (create command dispatch, around line 57)
- Test: `tests/cli_create_test.rs`

**What to implement:**

The create command currently does not load the store. To check for existing singleton documents, it needs store access.

1. Change `create::run()` signature to accept `&Store` as a parameter.
2. In `main.rs`, load the store before calling `create::run()` when the create command is invoked. The store is already loaded elsewhere (e.g. for validate, list, show), so follow the same pattern.
3. In `create::run()`, after resolving the `type_def` (line 19), add:

```rust
if type_def.singleton {
    let existing: Vec<_> = store.list(&Filter {
        doc_type: Some(DocType::new(&doc_type)),
        ..Default::default()
    });
    if let Some(doc) = existing.first() {
        bail!("{} already exists at {}", doc_type, doc.path.display());
    }
}
```

4. Update `run_json()` signature to also accept `&Store`.

**How to verify:**
```
cargo test --test cli_create_test
```

### Task 3: Singleton and parent_type validation checks

**ACs addressed:** singleton-validation-error, parent-type-validation-location, parent-type-requires-singleton-parent

**Files:**
- Modify: `src/engine/validation.rs`
- Test: `tests/cli_validate_test.rs`

**What to implement:**

Add two new `ValidationIssue` variants (in the enum around line 8):

```rust
SingletonViolation {
    type_name: String,
    paths: Vec<PathBuf>,
},
ParentTypeViolation {
    path: PathBuf,
    type_name: String,
    expected_dir: String,
},
ParentTypeNotSingleton {
    type_name: String,
    parent_type: String,
},
```

Add display formatting for each variant in the `Display` impl.

Create a new checker struct `TypeConstraintChecker` implementing the `Checker` trait:

1. For each type in `config.documents.types` where `singleton == true`:
   - Count documents of that type in the store via `store.list()`
   - If count > 1, emit `(Error, SingletonViolation { type_name, paths })`

2. For each type in `config.documents.types` where `parent_type.is_some()`:
   - Look up the parent type in config by name
   - If the parent type does not have `singleton == true`, emit `(Error, ParentTypeNotSingleton { type_name, parent_type })`
   - For each document of this type, check its path starts with the parent type's `dir`
   - If not, emit `(Error, ParentTypeViolation { path, type_name, expected_dir })`

Register `TypeConstraintChecker` in `default_checkers()` (around line 756).

**How to verify:**
```
cargo test --test cli_validate_test
```

## Test Plan

### Config deserialization tests (Task 1)

Add to `tests/config_test.rs`:

- `singleton_field_defaults_to_false`: Parse TOML with a type entry that omits `singleton`. Assert `type_def.singleton == false`.
- `singleton_field_parses_true`: Parse TOML with `singleton = true`. Assert `type_def.singleton == true`.
- `parent_type_defaults_to_none`: Parse TOML with a type entry that omits `parent_type`. Assert `type_def.parent_type.is_none()`.
- `parent_type_parses_value`: Parse TOML with `parent_type = "convention"`. Assert `type_def.parent_type == Some("convention".to_string())`.

These are isolated, fast, deterministic unit tests. Follow the existing `parse_types_from_toml` pattern.

### Create guard tests (Task 2)

Add to `tests/cli_create_test.rs`:

- `singleton_create_first_succeeds`: Configure a singleton type, create first doc, assert success.
- `singleton_create_second_fails`: Configure a singleton type, create first doc, attempt second, assert error contains "already exists".
- `non_singleton_create_multiple_succeeds`: Configure a non-singleton type, create two docs, both succeed.

These tests use `TestFixture` and call `create::run()` directly. They need store access, so they'll call `fixture.store()` after creating the first document to get the updated store.

### Validation tests (Task 3)

Add to `tests/cli_validate_test.rs`:

- `singleton_violation_detected`: Create two docs of a singleton type (by writing files directly, bypassing the create guard). Validate. Assert `SingletonViolation` error.
- `singleton_single_doc_no_error`: Create one doc of a singleton type. Validate. Assert no singleton-related error.
- `parent_type_inside_dir_no_error`: Create a doc of a type with `parent_type` inside the parent's dir. Validate. Assert no parent_type-related error.
- `parent_type_outside_dir_error`: Create a doc of a type with `parent_type` outside the parent's dir. Validate. Assert `ParentTypeViolation` error.
- `parent_type_references_non_singleton_error`: Configure type A with `parent_type = "b"` where type B has `singleton = false`. Validate. Assert `ParentTypeNotSingleton` error.

These are integration tests that write fixture files and run the full validation pipeline. Follow existing patterns in `cli_validate_test.rs`.

## Notes

The create command signature change (adding `&Store`) is a breaking change to the internal API. All callers in `main.rs` and the TUI (if it calls create directly) need updating. The TUI create path should be checked during implementation -- it likely calls `create::run()` too.
