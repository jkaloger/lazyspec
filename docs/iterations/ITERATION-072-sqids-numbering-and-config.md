---
title: Sqids numbering and config
type: iteration
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- implements: docs/stories/STORY-064-sqids-numbering-and-config.md
---



## Changes

### Task 1: Add sqids dependency and numbering config types

**ACs addressed:** AC-4, AC-5, AC-6, AC-7

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/engine/config.rs`

**What to implement:**

Add `sqids = "0.4"` to `[dependencies]` in `Cargo.toml`.

In `config.rs`, add a `NumberingStrategy` enum (`Incremental`, `Sqids`) and a `SqidsConfig` struct with fields `salt: String` and `min_length: u8`. Add an optional `numbering` field to `TypeDef` (defaults to `Incremental`). Add an optional `[numbering.sqids]` section to `RawConfig`/`Config`.

In `Config::parse`, add validation:
- If any type has `numbering = "sqids"`, a `[numbering.sqids]` section with a non-empty `salt` is required (AC-7).
- `min_length` must be in range 1..=10 (AC-6). Default to 3 if omitted.

Return `anyhow::bail!` with a clear message on validation failure.

**How to verify:**
`cargo test -- config` should pass. Write unit tests in `tests/config_test.rs` that assert: valid sqids config parses, missing salt fails, min_length=0 and min_length=11 fail, absent numbering field defaults to incremental.

---

### Task 2: Implement sqids ID generation in template.rs

**ACs addressed:** AC-1, AC-8, AC-9

**Files:**
- Modify: `src/engine/template.rs`

**What to implement:**

Add a `pub fn next_sqids_id(dir: &Path, prefix: &str, sqids_config: &SqidsConfig) -> String` function. It should:
1. Count existing files in `dir` that start with `prefix` (same scan as `next_number`).
2. Build a `sqids::Sqids` instance using the configured `salt` and `min_length`.
3. Encode `count + 1` to produce a candidate ID.
4. Check for filename collision in `dir`. If collision, increment input and retry (AC-8).
5. Return the ID as lowercase (AC-9).

Update `resolve_filename` to accept an optional `NumberingStrategy` + `SqidsConfig`. When sqids, replace `{n:03}` / `{n}` with the sqids ID instead of the zero-padded number. When incremental (or None), preserve current behavior (AC-2, AC-3).

**How to verify:**
`cargo test -- template` with new unit tests: sqids ID is lowercase, min_length respected, salt changes output, collision retry works.

---

### Task 3: Thread numbering config through the create command

**ACs addressed:** AC-1, AC-2, AC-3

**Files:**
- Modify: `src/cli/create.rs`
- Modify: `src/cli/fix.rs`

**What to implement:**

In `create.rs::run`, after resolving `type_def`, look up its `numbering` strategy. If sqids, retrieve `sqids_config` from `config` and pass both to `resolve_filename`. The call site change is small since `resolve_filename` is the integration point.

In `fix.rs`, update the call to `next_number` to be numbering-strategy-aware (use sqids ID generation when the type is configured for sqids).

**How to verify:**
Integration test: set up a temp project with `.lazyspec.toml` containing `numbering = "sqids"` on a type with a valid `[numbering.sqids]` section. Run `cargo run -- create rfc "Test" --json`. Assert the filename matches `RFC-<sqids-id>-test.md` pattern. Run again with default config and assert `RFC-001-test.md` pattern.

---

### Task 4: Integration tests for all ACs

**ACs addressed:** AC-1 through AC-9

**Files:**
- Create: `tests/sqids_numbering_test.rs`
- Modify: `tests/config_test.rs`

**What to implement:**

Tests covering each AC:
- AC-1: create with sqids config produces sqids filename
- AC-2: create with no numbering field produces incremental filename
- AC-3: create with `numbering = "incremental"` produces incremental filename
- AC-4: two different salts produce different IDs for same input
- AC-5: `min_length = 5` produces IDs >= 5 chars
- AC-6: `min_length = 0` and `min_length = 11` fail config validation
- AC-7: sqids numbering without salt fails config validation
- AC-8: pre-populate dir with colliding filename, verify retry
- AC-9: generated ID is all lowercase

Use `TestFixture` from `tests/common/mod.rs` and write custom `.lazyspec.toml` files into the temp dir.

**How to verify:**
`cargo test -- sqids` passes all tests.

## Test Plan

| AC | Test | Method |
|----|------|--------|
| AC-1 | Create doc with sqids config, assert filename pattern `RFC-<sqid>-title.md` | Integration |
| AC-2 | Create doc with no numbering field, assert `RFC-001-title.md` | Integration |
| AC-3 | Create doc with `numbering = "incremental"`, assert `RFC-001-title.md` | Integration |
| AC-4 | Generate IDs with two different salts, assert different outputs | Unit |
| AC-5 | Set `min_length = 5`, assert ID length >= 5 | Unit |
| AC-6 | Set `min_length = 0` or `11`, assert config parse error | Unit |
| AC-7 | Set sqids numbering with no salt, assert config parse error | Unit |
| AC-8 | Pre-create file with expected sqids name, assert next create increments | Unit |
| AC-9 | Generate sqids ID, assert `id == id.to_lowercase()` | Unit |

## Notes

The `sqids` crate (v0.4) handles alphabet customization and encoding. Salt is used to shuffle the default alphabet, not as a direct crate parameter -- implement by deterministically shuffling the alphabet using the salt before passing to `Sqids::builder().alphabet()`.

The `resolve_filename` signature change is the main integration point. All callers (`create.rs`, `fix.rs`) need updating but the change is mechanical.
