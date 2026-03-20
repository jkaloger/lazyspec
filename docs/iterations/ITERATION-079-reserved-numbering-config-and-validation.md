---
title: Reserved numbering config and validation
type: iteration
status: accepted
author: agent
date: 2026-03-20
tags: []
related:
- implements: docs/stories/STORY-069-reserved-numbering-config-and-validation.md
---



## Context

STORY-069 requires the config layer to understand `numbering = "reserved"` before any git plumbing can be wired up. This iteration adds the types, parsing, validation, and format dispatch. The existing pattern for sqids (enum variant, raw config struct, validation in `Config::parse`) is the template to follow.

## Changes

### Task 1: Add `ReservedConfig`, `ReservedFormat`, and `NumberingStrategy::Reserved`

**ACs addressed:** AC-1, AC-2

**Files:**
- Modify: `src/engine/config.rs`

**What to implement:**

Add two new types after `SqidsConfig` (line 45):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReservedFormat {
    Incremental,
    Sqids,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReservedConfig {
    #[serde(default = "default_reserved_remote")]
    pub remote: String,
    pub format: ReservedFormat,
    #[serde(default = "default_reserved_max_retries")]
    pub max_retries: u8,
}

fn default_reserved_remote() -> String { "origin".to_string() }
fn default_reserved_max_retries() -> u8 { 5 }
```

Add a `Reserved` variant to `NumberingStrategy`:

```rust
pub enum NumberingStrategy {
    #[default]
    Incremental,
    Sqids,
    Reserved,
}
```

The variant is a simple tag (like `Sqids`), not carrying the config inline. The `ReservedConfig` lives on `Config` as a sibling to `sqids: Option<SqidsConfig>`, following the same pattern.

Add `reserved: Option<ReservedConfig>` to the `Config` struct (line 73, alongside `sqids`).

**How to verify:**
```
cargo test --test config_test
cargo build
```

### Task 2: Parse `[numbering.reserved]` from TOML

**ACs addressed:** AC-1, AC-2, AC-4

**Files:**
- Modify: `src/engine/config.rs`

**What to implement:**

Add `reserved: Option<ReservedConfig>` to `RawNumbering` (line 106, alongside `sqids`).

In `Config::parse` (around line 277), extract the reserved config the same way sqids is extracted:

```rust
let reserved = raw.numbering.as_ref().and_then(|n| n.reserved.clone());
```

Include `reserved` in the returned `Config`.

When no `[numbering.reserved]` section is present but a type uses `numbering = "reserved"`, validation should fail (Task 3 handles this). When the section is present with omitted `remote` and `max_retries`, serde defaults produce `"origin"` and `5`.

**How to verify:**
```
cargo test --test config_test
```

### Task 3: Validate reserved config

**ACs addressed:** AC-3, AC-4, AC-5

**Files:**
- Modify: `src/engine/config.rs`

**What to implement:**

In `Config::parse`, after the existing sqids validation block (line 289), add reserved validation:

```rust
let any_reserved = types.iter().any(|t| t.numbering == NumberingStrategy::Reserved);
if any_reserved {
    let Some(ref reserved_cfg) = reserved else {
        bail!("numbering = \"reserved\" requires a [numbering.reserved] section");
    };
    if reserved_cfg.remote.is_empty() {
        bail!("numbering.reserved.remote must not be empty");
    }
    if reserved_cfg.format == ReservedFormat::Sqids {
        let Some(ref sqids_cfg) = sqids else {
            bail!("numbering.reserved.format = \"sqids\" requires a [numbering.sqids] section with a non-empty salt");
        };
        if sqids_cfg.salt.is_empty() {
            bail!("numbering.reserved.format = \"sqids\" requires a non-empty numbering.sqids.salt");
        }
        if sqids_cfg.min_length < 1 || sqids_cfg.min_length > 10 {
            bail!("numbering.sqids.min_length must be between 1 and 10, got {}", sqids_cfg.min_length);
        }
    }
}
```

This reuses the sqids validation when `format = "sqids"`, and adds the empty-remote check.

**How to verify:**
```
cargo test --test config_test
```

### Task 4: Format dispatch in `resolve_filename` and `create`

**ACs addressed:** AC-6, AC-7

**Files:**
- Modify: `src/engine/template.rs`
- Modify: `src/cli/create.rs`

**What to implement:**

In `src/cli/create.rs` (lines 25-32), add a `NumberingStrategy::Reserved` arm to the match. For now, the reserved path will compute the number locally using the format dispatch (the actual git reservation is STORY-068's scope). The reserved arm should:

- If `format = "incremental"`: call `next_number` to get the integer, format as zero-padded string
- If `format = "sqids"`: call `next_sqids_id` to get the sqids-encoded string

This means `resolve_filename` needs to accept a pre-computed ID string as an alternative to computing one internally. Add an enum or change the signature to support passing a pre-computed number:

```rust
pub enum NumberSource {
    Compute(Option<(&NumberingStrategy, &SqidsConfig)>),
    Precomputed(String),
}
```

Or simpler: add an `Option<String>` parameter for the pre-computed ID. When `Some`, skip the `next_number`/`next_sqids_id` calls and substitute directly. When `None`, use the existing logic.

The `create` command for `Reserved` computes the ID locally (format dispatch), then passes it as the pre-computed value. This keeps the template layer pure for when STORY-068 replaces the local computation with git reservation.

**How to verify:**
```
cargo test --test config_test
cargo test --test sqids_numbering_test
cargo test template::tests
```

### Task 5: Tests

**ACs addressed:** All

**Files:**
- Modify: `tests/config_test.rs`

**What to implement:**

Add a "Numbering / Reserved config tests" section after the existing sqids tests (line 453). Tests to write:

1. `valid_reserved_config_parses` -- TOML with `numbering = "reserved"` and full `[numbering.reserved]` section produces `NumberingStrategy::Reserved` with correct fields on `config.reserved`.
2. `reserved_config_defaults` -- Omit `remote` and `max_retries`, verify defaults to `"origin"` and `5`.
3. `reserved_sqids_format_requires_sqids_config` -- `format = "sqids"` without `[numbering.sqids]` fails with error mentioning "sqids".
4. `reserved_incremental_format_no_sqids_needed` -- `format = "incremental"` without `[numbering.sqids]` succeeds.
5. `reserved_empty_remote_fails` -- `remote = ""` fails with error mentioning "remote".
6. `reserved_missing_section_fails` -- `numbering = "reserved"` on a type without `[numbering.reserved]` fails.

Follow the existing test pattern: parse a TOML string, assert on `Config::parse` result.

**How to verify:**
```
cargo test --test config_test
```

## Test Plan

- Valid `[numbering.reserved]` section parses into `NumberingStrategy::Reserved` with correct `ReservedConfig` fields (isolated, deterministic, fast)
- Omitted `remote`/`max_retries` default to `"origin"` and `5` (tests serde defaults, deterministic)
- `format = "sqids"` without `[numbering.sqids]` fails validation with actionable error (behavioral, specific)
- `format = "incremental"` without `[numbering.sqids]` succeeds (behavioral)
- Empty remote string fails validation (behavioral, specific)
- `numbering = "reserved"` without `[numbering.reserved]` section fails (behavioral, specific)
- Existing sqids and incremental tests continue passing (structure-insensitive, ensures no regressions)

All tests are unit-level config parsing: parse a TOML string, assert on the result. No filesystem, no git, no timing. Fast, isolated, deterministic.

## Notes

The format dispatch in Task 4 is a temporary shim. It computes the number locally using the same logic as incremental/sqids, which means it doesn't actually reserve anything yet. STORY-068 replaces this with the git plumbing that does real reservation. The shim exists so the config-to-filename pipeline is wired end-to-end and testable before git enters the picture.
