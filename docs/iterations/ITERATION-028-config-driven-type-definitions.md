---
title: Config-driven type definitions
type: iteration
status: draft
author: agent
date: 2026-03-06
tags: []
related:
- implements: docs/stories/STORY-037-config-driven-type-definitions.md
---


## Changes

### Task 1: Add TypeDef struct and update Config

**ACs addressed:** AC-1 (defaults when no `[[types]]`), AC-3 (TypeDef fields), AC-4 (missing field error)

**Files:**
- Modify: `src/engine/config.rs`
- Modify: `tests/config_test.rs`

**What to implement:**

Add a `TypeDef` struct with `name`, `plural`, `dir`, `prefix`, and `icon: Option<String>`. Replace the `Directories` struct with `types: Vec<TypeDef>` on `Config`. Keep `templates` and `naming` as-is.

`Config::default()` returns four TypeDefs matching today's directories:
- `{ name: "rfc", plural: "rfcs", dir: "docs/rfcs", prefix: "RFC", icon: Some("●") }`
- `{ name: "story", plural: "stories", dir: "docs/stories", prefix: "STORY", icon: Some("▲") }`
- `{ name: "iteration", plural: "iterations", dir: "docs/iterations", prefix: "ITERATION", icon: Some("◆") }`
- `{ name: "adr", plural: "adrs", dir: "docs/adrs", prefix: "ADR", icon: Some("■") }`

Add a `Config::type_by_name(&self, name: &str) -> Option<&TypeDef>` helper for lookups.

For TOML parsing, support both the new `[[types]]` array format and the legacy `[directories]` format. When `[directories]` is present and `[[types]]` is absent, convert the named fields into TypeDef entries internally. This avoids breaking existing `.lazyspec.toml` files. When neither is present, use defaults.

Add `Serialize`/`Deserialize` derives to `TypeDef`. Use `#[serde(default)]` on the `types` field so missing `[[types]]` falls through to `Default`.

**How to verify:**
```
cargo test config_test
```

### Task 2: Convert DocType enum to string newtype

**ACs addressed:** AC-5 (frontmatter deserialization), AC-6 (unknown type error)

**Files:**
- Modify: `src/engine/document.rs`
- Modify: `tests/document_test.rs`

**What to implement:**

Replace the `DocType` enum with:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DocType(pub String);
```

Add a `DocType::new(s: &str) -> Self` that lowercases the input. Implement `Display` to return the inner string lowercase. Implement `FromStr` to accept any string (lowercased). Implement `Deserialize` to lower-case on deserialize.

Remove the hardcoded `FromStr` match arms. The validation of whether a type name is "known" moves to callers that have access to `Config` (like `create.rs`). The `DocType` itself accepts any string.

Update `RawFrontmatter` -- `doc_type: DocType` stays the same, serde handles it via the custom Deserialize impl.

Add constants for the default type names to avoid magic strings scattered through the codebase:
```rust
impl DocType {
    pub const RFC: &str = "rfc";
    pub const STORY: &str = "story";
    pub const ITERATION: &str = "iteration";
    pub const ADR: &str = "adr";
}
```

**How to verify:**
```
cargo test document_test
```

### Task 3: Update consumers to compile with new types

**ACs addressed:** AC-1 (defaults work), AC-5 (deserialization works end-to-end)

**Files:**
- Modify: `src/engine/store.rs` (lines 26-31: iterate `config.types` instead of named fields)
- Modify: `src/engine/validation.rs` (replace `DocType::Iteration` etc. with `DocType::new("iteration")` or the constants)
- Modify: `src/cli/create.rs` (lines 18-23: use `config.type_by_name()` lookup)
- Modify: `src/cli/init.rs` (iterate `config.types` for dir creation)
- Modify: `src/cli/status.rs` (line 32: build type list from config)
- Modify: `src/cli/style.rs` (use `DocType` display instead of matching)
- Modify: `src/cli/search.rs` (string parse still works)
- Modify: `src/cli/json.rs` (line 8: `DocType` display already lowercase)
- Modify: `src/tui/app.rs` (line 239: populate from config; line 115: use `DocType::new`)
- Modify: `src/tui/ui.rs` (lines 905-910: look up icon from config with fallback glyphs)
- Modify: `src/tui/mod.rs` (lines 60-63: iterate config.types for file watcher)
- Modify: `tests/common/mod.rs` (no code change needed, writes string types in frontmatter)

This is a mechanical task: replace every `DocType::Rfc` with `DocType::new(DocType::RFC)`, replace every `config.directories.rfcs` with a `config.type_by_name("rfc").unwrap().dir` call (or iterate `config.types`), and replace match-on-enum patterns with string comparisons.

For `create.rs`, the directory lookup becomes:
```rust
let type_def = config.type_by_name(doc_type)
    .ok_or_else(|| anyhow!("unknown doc type: {}. valid types: {}", doc_type, valid_types_str))?;
let dir = &type_def.dir;
```

For `validation.rs`, replace enum comparisons with string comparisons using the constants. The hardcoded parent-child hierarchy (`Rfc -> Story -> Iteration`) stays for now (STORY-038 makes it configurable).

For the TUI graph icons in `ui.rs`, look up `TypeDef.icon` from config. Fall back to a glyph from `["●", "■", "▲", "◆", "★", "◎"]` by index if no icon configured.

**How to verify:**
```
cargo test
```

### Task 4: Update test assertions

**ACs addressed:** All ACs (tests confirm end-to-end behavior)

**Files:**
- Modify: `tests/store_test.rs` (replace `DocType::Rfc` etc. with `DocType::new("rfc")`)
- Modify: `tests/tui_graph_test.rs` (same pattern)
- Modify: `tests/tui_create_form_test.rs` (same pattern)
- Modify: `tests/tui_submit_form_test.rs` (same pattern)
- Modify: `tests/cli_query_test.rs` (same pattern)
- Modify: `tests/config_test.rs` (assert on `config.types[0].dir` instead of `config.directories.rfcs`)

Mechanical replacement across all test files. Every `DocType::Rfc` becomes `DocType::new("rfc")`, every `config.directories.rfcs` becomes a lookup on `config.types`.

**How to verify:**
```
cargo test
```

## Test Plan

**Config parsing (Task 1):**
- Test default config produces 4 TypeDefs with expected names, dirs, prefixes
- Test `[[types]]` TOML parses into correct TypeDefs
- Test legacy `[directories]` TOML still works (converted internally)
- Test missing required field on a `[[types]]` entry returns error
- Test `type_by_name` returns None for unknown types

**DocType newtype (Task 2):**
- Test `DocType::new("RFC")` lowercases to `DocType("rfc")`
- Test frontmatter with `type: rfc` deserializes correctly
- Test frontmatter with `type: custom-thing` deserializes (no rejection at parse level)
- Test Display outputs lowercase

**Integration (Tasks 3-4):**
- Existing test suite passes with no behavioral change. This is the primary verification -- if `cargo test` passes, the refactor is correct.

## Notes

Changing `DocType` from an enum to a string newtype necessarily touches every consumer. Tasks 3 and 4 are large but mechanical. The key design work is in Tasks 1 and 2.

The validation rules in `validation.rs` keep their hardcoded type names for now (using `DocType` constants). STORY-038 will make these configurable.
