---
title: Semantic hashing pipeline
type: iteration
status: accepted
author: agent
date: 2026-03-26
tags: []
related:
- implements: STORY-085
---




## Changes

### Task 1: Add AST normalization to symbol extraction

**ACs addressed:** symbol-semantic-hash, comment-change-no-drift, structural-change-drifts

**Files:**
- Modify: `src/engine/symbols.rs`

**What to implement:**
Add a `normalize_symbol` function that takes a tree-sitter `Node` and the source bytes, then walks the node tree to produce a canonical representation. The walk must:
1. Skip all comment nodes (`line_comment`, `block_comment` for Rust; `comment` for TypeScript)
2. Collect the text of all remaining leaf nodes
3. Collapse runs of whitespace into single spaces
4. Return the normalized byte string

Add a new trait method `fn extract_normalized(&self, source: &str, symbol: &str) -> Option<String>` with a default implementation that calls `extract()` then re-parses and normalizes. The existing `extract()` method remains unchanged (no breaking changes).

Alternatively, implement normalization as a standalone `pub fn normalize_ast(source: &str, node: tree_sitter::Node) -> String` so the hashing module can call it directly after extraction.

**How to verify:**
- `cargo test --lib` -- unit tests in `symbols.rs` for normalization
- Extract a symbol, normalize it, change only comments/whitespace, normalize again -- outputs must match
- Change structural code, normalize -- outputs must differ

---

### Task 2: Implement `git hash-object` integration

**ACs addressed:** git-hash-object-integration, file-hash-raw-content, symbol-semantic-hash

**Files:**
- Create: `src/engine/hashing.rs`
- Modify: `src/engine/mod.rs` (add `pub mod hashing;`)

**What to implement:**
Create a `hashing` module with two public functions:

1. `pub fn hash_bytes(bytes: &[u8]) -> Result<String>` -- pipes the given bytes to `git hash-object --stdin` and returns the 40-char hex SHA. This is the single integration point for all hashing.

2. `pub fn hash_file(path: &Path) -> Result<String>` -- runs `git hash-object <path>` on the raw file (no normalization). Used for whole-file refs.

Both functions shell out to git. Errors propagate via `anyhow::Result`.

**How to verify:**
- `cargo test` -- unit/integration tests that hash known content and compare to expected SHA
- Manually: `echo -n "hello" | git hash-object --stdin` should match `hash_bytes(b"hello")`

---

### Task 3: Implement semantic hash pipeline (combining extraction + normalization + hashing)

**ACs addressed:** symbol-semantic-hash, comment-change-no-drift, structural-change-drifts, file-hash-raw-content

**Files:**
- Create: `src/engine/certification.rs`
- Modify: `src/engine/mod.rs` (add `pub mod certification;`)

**What to implement:**
Create a `certification` module with a public function:

`pub fn compute_blob_hash(root: &Path, file_path: &str, symbol: Option<&str>, normalize: bool) -> Result<String>`

Logic:
- If `symbol` is `None` (whole-file ref): call `hashing::hash_file()` on the file. Ignore the `normalize` flag (whole-file refs are never normalized per RFC).
- If `symbol` is `Some(name)`:
  - Read the file, determine language from extension
  - Use the appropriate `SymbolExtractor` to extract the symbol text
  - If `normalize` is true: parse with tree-sitter, call `normalize_ast()`, then `hash_bytes()` on the normalized output
  - If `normalize` is false: call `hash_bytes()` on the raw extracted text

This is the function that Iteration 3 (pin command) will call.

**How to verify:**
- Integration test: create a temp file with a known Rust function, compute its semantic hash, verify it matches manual `git hash-object` of the normalized content
- Verify whole-file path uses raw content

---

### Task 4: Add certification config section

**ACs addressed:** normalize-config-default, normalize-config-opt-out

**Files:**
- Modify: `src/engine/config.rs`

**What to implement:**
Add new structs to the config module:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CertificationConfig {
    #[serde(default = "default_normalize")]
    pub normalize: bool,
    #[serde(default)]
    pub overrides: HashMap<String, CertificationOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificationOverride {
    pub normalize: bool,
}

fn default_normalize() -> bool { true }
```

Add `pub certification: CertificationConfig` to the `Config` struct (with `#[serde(default)]`). Update `RawConfig` and `Config::parse()` to deserialize the `[certification]` section. When absent, defaults to `normalize = true` with no overrides.

Add a helper method: `pub fn should_normalize(&self, spec_path: &str) -> bool` that checks overrides first, then falls back to the global default.

**How to verify:**
- `cargo test` -- config parsing tests: parse with/without `[certification]`, parse with overrides
- Verify default is `normalize = true`
- Verify override for a specific spec path returns `false`

---

### Task 5: Wire normalize config into the hash pipeline

**ACs addressed:** normalize-config-default, normalize-config-opt-out

**Files:**
- Modify: `src/engine/certification.rs`

**What to implement:**
Add a higher-level function or update `compute_blob_hash` to accept a `&Config` (or just the `CertificationConfig`) so it can resolve the normalize flag from config. Add a convenience function:

`pub fn compute_blob_hash_for_spec(root: &Path, config: &Config, spec_path: &str, file_path: &str, symbol: Option<&str>) -> Result<String>`

This calls `config.should_normalize(spec_path)` and passes the result to `compute_blob_hash`.

**How to verify:**
- Test with default config (no certification section) -- normalization applied
- Test with override `normalize = false` for a specific spec -- raw bytes hashed

## Test Plan

### AC Coverage

| AC | Test Description | Location |
|----|-----------------|----------|
| symbol-semantic-hash | Extract a Rust function, normalize AST (strip comments, collapse whitespace), hash via git hash-object, verify SHA matches manual computation | `src/engine/symbols.rs` (unit), `tests/certification_test.rs` (integration) |
| comment-change-no-drift | Hash a symbol, add/change comments and whitespace, re-hash -- same SHA | `tests/certification_test.rs` |
| structural-change-drifts | Hash a symbol, add a parameter, re-hash -- different SHA | `tests/certification_test.rs` |
| file-hash-raw-content | Hash a whole file via `hash_file()`, compare to `git hash-object <file>` output | `tests/certification_test.rs` |
| git-hash-object-integration | Pass known bytes to `hash_bytes()`, verify output matches `echo -n <bytes> \| git hash-object --stdin` | `src/engine/hashing.rs` (unit) |
| normalize-config-default | Parse a `.lazyspec.toml` with no `[certification]` section, verify `should_normalize()` returns `true` | `src/engine/config.rs` (unit), `tests/config_test.rs` |
| normalize-config-opt-out | Parse a `.lazyspec.toml` with `[certification.overrides."path"] normalize = false`, verify `should_normalize("path")` returns `false` | `src/engine/config.rs` (unit), `tests/config_test.rs` |

### Planned Tests

**Normalization unit tests** (`src/engine/symbols.rs`):
1. `test_normalize_strips_line_comments` -- Rust source with `//` comments, normalized output has no comments
2. `test_normalize_strips_block_comments` -- Rust source with `/* */` comments, normalized output has no comments
3. `test_normalize_collapses_whitespace` -- Extra blank lines and indentation produce same output as minimal whitespace
4. `test_normalize_preserves_code_structure` -- Keywords, identifiers, operators all present after normalization
5. `test_normalize_ts_strips_comments` -- TypeScript source with `//` and `/* */` comments, normalized output clean
6. `test_normalize_idempotent` -- Normalizing already-normalized content produces identical output

**Hashing unit tests** (`src/engine/hashing.rs`):
7. `test_hash_bytes_known_content` -- Hash `b"hello"` and verify against known git hash-object output
8. `test_hash_bytes_empty` -- Hash empty bytes, verify valid SHA returned
9. `test_hash_file_matches_git` -- Create temp file, hash it, compare to `git hash-object` CLI output

**Integration tests** (`tests/certification_test.rs`):
10. `test_symbol_semantic_hash_roundtrip` -- Extract + normalize + hash a Rust function, verify deterministic
11. `test_comment_only_change_no_drift` -- Two versions of a function differing only in comments produce same hash
12. `test_whitespace_only_change_no_drift` -- Two versions differing only in whitespace produce same hash
13. `test_structural_change_produces_drift` -- Add a parameter to a function, hash changes
14. `test_type_change_produces_drift` -- Change a return type, hash changes
15. `test_whole_file_hash_no_normalization` -- File hash uses raw content, comments included in hash
16. `test_whole_file_hash_comment_change_drifts` -- Adding a comment to a file changes its whole-file hash (unlike symbol hash)

**Config tests** (`tests/config_test.rs` or `src/engine/config.rs`):
17. `test_certification_default_when_absent` -- No `[certification]` section, `normalize` defaults to `true`
18. `test_certification_explicit_true` -- `[certification] normalize = true` parses correctly
19. `test_certification_explicit_false` -- `[certification] normalize = false` parses correctly
20. `test_certification_override_disables_normalize` -- Override for a specific spec path returns `false`
21. `test_certification_override_does_not_affect_other_specs` -- Override for spec A does not affect spec B
22. `test_should_normalize_falls_back_to_global` -- No override for a path, returns global default

## Notes

### Boundary with Iteration 1

This iteration assumes the `@ref path#symbol@{blob:hash}` parsing from Iteration 1 is already in place. The `Ref` struct and regex extensions are not implemented here. This iteration provides the hashing functions that produce the blob hash values.

### Boundary with Iteration 3

The `pin` CLI command (Iteration 3) will call `compute_blob_hash_for_spec()` to compute hashes and write them into `@ref` directives. This iteration does not modify any CLI commands or `@ref` syntax.

### Design decisions

- `normalize_ast` is a standalone function rather than a trait method to keep the `SymbolExtractor` trait focused on extraction. Normalization is a separate concern used only by the certification pipeline.
- `git hash-object` is called via subprocess rather than reimplementing the git blob hashing algorithm (`blob <size>\0<content>`). This ensures hash compatibility with the actual git object store.
- Whole-file refs intentionally skip normalization even when the global config says `normalize = true`, per the RFC. Comments and whitespace may be meaningful in config files and schemas.
