---
title: Validation diagnostic for duplicate IDs
type: iteration
status: draft
author: agent
date: 2026-03-13
tags: []
related:
- implements: STORY-062
---



## Test Plan

### Unit tests in `tests/cli_validate_test.rs`

- **duplicate_id_reported**: Create two RFCs that resolve to the same extracted ID (e.g. `RFC-001-alpha.md` and `RFC-001-beta.md`, both extracting to `001`). Call `validate_full`, assert `errors` contains a `DuplicateId` issue listing both paths. (AC 1, 4)
- **duplicate_id_json_output**: Same fixture as above but call `run_json`. Parse the JSON, assert the `errors` array contains a string matching `"duplicate id"` with the conflicting ID and both paths. (AC 2)
- **duplicate_id_human_output**: Same fixture, call `run_human`. Assert the output contains a descriptive line with the duplicate ID and both paths. (AC 3)
- **unique_ids_no_duplicate_diagnostic**: Create two RFCs with distinct IDs (`RFC-001.md`, `RFC-002.md`). Validate. Assert no `DuplicateId` issues appear. (AC 4)
- **validate_ignore_excludes_from_duplicate_grouping**: Create two RFCs with the same extracted ID. Mark one with `validate_ignore: true`. Validate. Assert no `DuplicateId` issue is emitted. (AC 5)

## Changes

### 1. Add `DuplicateId` variant to `ValidationIssue`

- **File**: `src/engine/validation.rs`
- **ACs**: 1, 3
- **Details**: Add a new variant `DuplicateId { id: String, paths: Vec<PathBuf> }` to the `ValidationIssue` enum. Implement `Display` for it following the existing pattern, producing a message like `"duplicate id: 001 (docs/rfcs/RFC-001-alpha.md, docs/rfcs/RFC-001-beta.md)"`.
- **Verify**: Project compiles (`cargo build`).

### 2. Add duplicate-ID grouping logic to `validate_full`

- **File**: `src/engine/validation.rs`
- **ACs**: 1, 4, 5
- **Details**: After the existing per-document loop, collect all non-`validate_ignore` documents into a `HashMap<String, Vec<PathBuf>>` keyed by `meta.id`. Iterate the map; for any entry with `len > 1`, push a `DuplicateId` error via `result.errors.push(...)`. Sort the paths within each issue for deterministic output.
- **Verify**: `cargo test` passes existing tests. New unit test `duplicate_id_reported` passes.

### 3. Verify JSON and human-readable output surface the diagnostic

- **File**: `src/cli/validate.rs`
- **ACs**: 2, 3
- **Details**: No code changes expected here. The existing `run_json` and `run_human` functions format all `result.errors` via `Display`, so the new variant will be picked up automatically. The new tests (`duplicate_id_json_output`, `duplicate_id_human_output`) confirm this.
- **Verify**: `cargo test duplicate_id_json_output` and `cargo test duplicate_id_human_output` pass.

### 4. Add tests

- **File**: `tests/cli_validate_test.rs`
- **ACs**: 1, 2, 3, 4, 5
- **Details**: Add the five tests described in the Test Plan above. Use the existing `TestFixture` helpers (`write_rfc`, `write_doc`) to set up fixtures. For the `validate_ignore` test, write raw frontmatter with `validate_ignore: true` via `write_doc`.
- **Verify**: `cargo test` -- all new and existing tests green.

## Notes

The `run_json` and `run_human` functions in `src/cli/validate.rs` already iterate over `result.errors` and format via `Display`, so task 3 should require no code changes. If the error message format needs adjustment for machine parsing (e.g. structured JSON fields instead of a flat string), that would be a separate story.
