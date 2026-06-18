---
title: Fix --config migration
type: iteration
status: accepted
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-128
---

## Context

Final slice of RFC-042. STORY-126 makes config load strict: a missing `[[relationships]]` or `[[rules]]` block becomes a hard error in the load path. That error fires for *every* command on an upgraded project, because the 4 historical relationships and 3 historical rules were previously implicit (baked into `default_rules()` / the `RelationType` enum in code). `init` (`src/cli/init.rs`) only scaffolds fresh projects and bails when `.lazyspec.toml` already exists, so it cannot repair an upgrade.

Per ADR-012, `fix` is the migration path. `fix` already tolerates broken *document* frontmatter that normal load rejects (`src/cli/fix/fields.rs` reads files directly, falling back to an empty mapping on parse error). This story extends that tolerance one layer up to *config*: a `--config` flag scopes `fix` to config-only and uses a lenient config read that bypasses strict load, injecting the standard blocks. That resolves the chicken-and-egg — strict load would reject the very config `fix` must read.

Dependencies: this depends on STORY-126's strict-load error and on the `[[relationships]]` / `RelationshipDef` config shapes it introduces. It must inject the SAME standard set that STORY-127's `init` writes — STORY-127's blocks are the canonical content. The 4 relationships and 3 rules below are derived from the current `default_rules()` (`src/engine/config.rs`) and the `RelationType` enum + inverse strings (`src/engine/document.rs`), which are the historical implicit defaults this migration makes explicit.

The standard set to inject (must match STORY-127):

Relationships:
- `implements` / inverse `implemented-by`
- `supersedes` / inverse `superseded-by`
- `blocks` / inverse `blocked-by`
- `related-to` (symmetric, no inverse)

Rules (current `default_rules()`):
- `stories-need-rfcs` — parent-child, child `story`, parent `rfc`, link `implements`, severity **warning**
- `iterations-need-stories` — parent-child, child `iteration`, parent `story`, link `implements`, severity **error**
- `adrs-need-relations` — relation-existence, type `adr`, require `any-relation`, severity **error**

## Test Plan

All tests are CLI/engine integration tests in a new file `tests/integration/cli_fix_config_test.rs` (registered in `tests/integration/main.rs`). Each test gets its own `TestFixture` (own `TempDir`, per DICTUM-004) and writes its own `.lazyspec.toml`; tests are behavioral and deterministic. The migration entry point is exercised through the new `fix::run_config*` function(s) so assertions read on-disk bytes, not internal state.

A shared helper builds a "pre-migration" `.lazyspec.toml`: a valid `[[types]]`-only config (the 7 standard types) with NO `[[relationships]]` and NO `[[rules]]`, matching what an upgraded legacy project looks like.

- **AC1 — injects standard blocks** (`fix_config_injects_relationships_and_rules`)
  - Arrange: fixture with the pre-migration `.lazyspec.toml` (types only).
  - Act: run the config-fix entry point with `dry_run = false`.
  - Assert: re-read `.lazyspec.toml` from disk; parse as TOML; assert exactly 4 `[[relationships]]` entries with the names/inverses above (and that `related-to` has no inverse), and exactly 3 `[[rules]]` with the names/shapes/severities above. Assert the previously-present `[[types]]` block is preserved unchanged.

- **AC2 — dry-run reports but does not write** (`fix_config_dry_run_leaves_file_unchanged`)
  - Arrange: pre-migration fixture; capture the original `.lazyspec.toml` bytes.
  - Act: run config-fix with `dry_run = true`, capturing the JSON/human output.
  - Assert: output reports the additions (4 relationships + 3 rules named, "would add"); re-read the file and assert byte-for-byte equality with the captured original (explicit dry-run-unchanged per DICTUM-004).

- **AC3 — repaired config loads under strict load** (`fix_config_result_passes_strict_load`)
  - Arrange: pre-migration fixture.
  - Act: run config-fix (`dry_run = false`), then call the strict `Config::load`/parse path on the repaired file.
  - Assert: load returns Ok (no strict-load error); the loaded config contains the 4 relationships and 3 rules; a sample existing relationship reference (e.g. `implements`) resolves against the injected registry.

- **AC4 — config-only scope, no documents touched** (`fix_config_does_not_touch_documents`)
  - Arrange: pre-migration fixture PLUS a document with deliberately incomplete frontmatter (e.g. an RFC missing `status`/`tags`) that plain `fix` would rewrite. Capture that document's bytes.
  - Act: run config-fix (`dry_run = false`).
  - Assert: `.lazyspec.toml` is modified (blocks injected); the broken document's bytes are byte-for-byte unchanged (config-only scope skips all document fixes).

- **AC5 — strict-load error names `lazyspec fix`** (`strict_load_error_names_fix`)
  - Arrange: fixture with the pre-migration `.lazyspec.toml` (missing `[[relationships]]`).
  - Act: call the strict config load/parse path directly and capture the `Err`.
  - Assert: the error message contains the literal `lazyspec fix` (the remedy). This pins the message edited in Task 4.

- **AC6 — idempotent (run twice, no change)** (`fix_config_idempotent`)
  - Arrange: pre-migration fixture.
  - Act: run config-fix once (`dry_run = false`); capture the resulting `.lazyspec.toml` bytes; run config-fix a second time.
  - Assert: second run reports zero additions; re-read the file and assert byte-for-byte equality with the post-first-run bytes (explicit idempotency per DICTUM-004). Add a companion `fix_config_idempotent_dry_run` asserting a dry-run on already-migrated config reports nothing to add.

## Changes

### Task 1 — Add `--config` flag to the `fix` CLI command (ACs 1,2,4)
- File: `src/cli.rs` — in the `Commands::Fix` variant (around the `paths`/`dry_run`/`json`/`renumber`/`doc_type` args), add a `--config` boolean flag (`#[arg(long)] config: bool`) with a doc comment: "Repair `.lazyspec.toml` instead of documents (injects missing standard relationships/rules)".
- File: `src/main.rs` — in the `Some(Commands::Fix { .. })` arm, destructure the new `config` field. When `config` is true, branch to the new config-fix path BEFORE the existing renumber/run branches and BEFORE relying on the strict-loaded `config` (see Task 2 for the load ordering). The config branch must short-circuit and return without running document/renumber fixes.
- Verify: `cargo run --quiet -- help fix` lists `--config`; `cargo build` clean.

### Task 2 — Lenient config read + dispatch ordering (ACs 1,3,5)
- Problem to solve: `src/main.rs:64` calls strict `Config::load(&cwd, &fs)?` for every command before the match. Once STORY-126 makes that strict, the `?` aborts before `fix` runs — so `fix --config` can never read the broken config through the normal path.
- File: `src/main.rs` — restructure so `fix --config` is handled before (or independently of) the strict `Config::load(&cwd, &fs)?` at line 64. Detect the `fix --config` invocation from `cli.command` and dispatch into the config-fix path using a *lenient* read, not the strict load result. (Mirror how `Init`/`Completions` are already special-cased before requiring a loaded project.)
- File: `src/engine/config.rs` — add a lenient read function alongside `Config::load`, e.g. `Config::load_lenient(project_root, fs) -> Result<Config>` (or a `parse_lenient(&str)`), that parses `.lazyspec.toml` but does NOT enforce the STORY-126 strict requirement for `[[relationships]]`/`[[rules]]` (tolerates them being absent, mirroring how `fix/fields.rs` tolerates missing frontmatter). This is the only caller of the lenient path; strict load stays the default everywhere else.
- Note for implementer: confirm the exact strict-check location STORY-126 adds (it will live in `Config::parse`/`load`). The lenient path must bypass exactly that check while keeping all other validation (numbering, github, etc.).
- Verify: `cargo build`; a temp project whose `.lazyspec.toml` lacks `[[relationships]]` can be read by the lenient path without error.

### Task 3 — Injection logic, dry-run, idempotency, JSON/human output (ACs 1,2,6)
- File: `src/cli/fix.rs` — add `run_config(root, dry_run, json, fs) -> i32` plus `run_config_json(...) -> String` and `run_config_human(...) -> String` mirroring the existing `run` / `run_json` / `run_human` trio (so tests can assert on returned strings, as the existing fix tests do).
- New module `src/cli/fix/config.rs` (declared `mod config;` in `src/cli/fix.rs`) holding `collect_config_fixes(root, dry_run, fs) -> ConfigFixResult`:
  1. Lenient-read `.lazyspec.toml` (Task 2) into a TOML document/value (preserve the existing `[[types]]` and other sections).
  2. Compute which of the 4 standard relationships and 3 standard rules are missing (compare by `name`). Source the canonical content from a shared constant/helper — reuse `default_rules()` for the rules; add a `default_relationships()` helper in `src/engine/config.rs` (the canonical relationship vocabulary, matching STORY-127's `init` and the historical `RelationType` enum + `INVERSE_STRS`) so `init` and `fix` share one source of truth.
  3. Append only the missing blocks (idempotency: if all present, additions is empty → file untouched, exit reports no change). Preserve unknown/extra user-defined relationships and rules.
  4. If `!dry_run` and additions non-empty, write the file; else leave on disk untouched. Record `written: bool` and the list of added relationship/rule names (mirror `FieldFixResult { written, fields_added }`).
- File: `src/cli/fix/output.rs` — extend `format_human` (or add a config-specific formatter) to print "Would add relationship/rule X" (dry-run) vs "Added relationship/rule X", reusing the existing dry-run phrasing convention.
- Idempotency detail: dedupe by `name`; never emit a duplicate block; when nothing is missing, do not rewrite the file (no whitespace/formatting churn) so the byte-unchanged assertions in AC6 hold.
- Verify: manual run on a temp project missing the blocks shows them injected; second run shows no change; `--dry-run` shows "would add" and leaves the file unchanged.

### Task 4 — Strict-load error names `lazyspec fix` (AC5)
- File: `src/engine/config.rs` — at the strict-load hard-error site STORY-126 introduces for missing `[[relationships]]`/`[[rules]]` (in `Config::parse`/`load`), ensure the `bail!`/error message names the remedy, e.g. `".lazyspec.toml is missing [[relationships]] (and/or [[rules]]); run `lazyspec fix --config` to migrate"`. If STORY-126 already raises a generic message, edit it to include `lazyspec fix --config`.
- Coordination note: this depends on STORY-126 landing the strict check first. If STORY-126's message already names `fix`, this task narrows to verifying/asserting it via the AC5 test; otherwise edit the message.
- Verify: AC5 test passes; `grep` for the message confirms `lazyspec fix` present.

### Task 5 — Tests + README (all ACs)
- File: `tests/integration/cli_fix_config_test.rs` (new) — implement the 6 AC tests + idempotent-dry-run companion from the Test Plan; register the module in `tests/integration/main.rs`.
- Use `TestFixture` (`tests/integration/common/mod.rs`) for the TempDir + doc/store helpers; write the pre-migration `.lazyspec.toml` via a local helper.
- File: `README.md` — document the new `fix --config` flag and the `--config --dry-run` preview under the `fix` command section (CLAUDE.md: update README when the CLI changes).
- Verify: `cargo test --test integration cli_fix_config` green; `cargo run --quiet -- validate --json` clean.

## Notes

Real paths verified to exist:
- `src/cli.rs` — `Commands::Fix` variant (line ~212), needs `--config` flag.
- `src/main.rs` — strict `Config::load(&cwd, &fs)?` at line 64 (runs for every command); `Some(Commands::Fix { .. })` arm at line ~296. The line-64 ordering is the crux of the chicken-and-egg: `fix --config` must dispatch via the lenient read before this strict load.
- `src/cli/fix.rs` — `run`/`run_json`/`run_human` trio + `plan_field_and_conflict_fixes` to mirror; declares submodules `conflicts`/`fields`/`output`/`relations`.
- `src/cli/fix/fields.rs` — the lenient *document* pattern to mirror one layer up: reads file directly, `split_frontmatter` failure falls back to empty mapping, only writes when `!dry_run && !fields_added.is_empty()`, returns `written: bool`.
- `src/cli/fix/output.rs` — `format_human` dry-run phrasing ("Would fix ..." vs "Fixed ...") to reuse.
- `src/cli/init.rs` — fresh-scaffold-only (`bail!` when `.lazyspec.toml` exists); stays unchanged per ADR-012. STORY-127 adds the standard blocks here; share `default_relationships()`/`default_rules()` with it.
- `src/engine/config.rs` — `default_rules()` (the 3 canonical rules, line ~385), `Config::parse`/`Config::load` (lenient read + strict-error message live here), `RawConfig` (has `rules: Option<Vec<ValidationRule>>`; STORY-126 adds the relationships field + strict check).
- `src/engine/document.rs` — `RelationType` enum + `ALL_STRS` (`["implements","supersedes","blocks","related-to"]`) + `INVERSE_STRS` (`["implemented-by","superseded-by","blocked-by"]`): the historical implicit relationship set the migration makes explicit.
- `tests/integration/cli_fix_test.rs` — patterns to mirror: `run_human`/`run_json` assertions, `fix_dry_run_does_not_write` (byte-unchanged), JSON shape checks via `serde_json`.
- `tests/integration/common/mod.rs` — `TestFixture` (own TempDir, `write_doc`, `config`, `store`, `root`).

Decisions:
- One source of truth for the standard set: add `default_relationships()` in `config.rs`; both `init` (STORY-127) and `fix --config` consume `default_relationships()` + `default_rules()`. Keeps STORY-127 and STORY-128 consistent as RFC-042 requires.
- Lenient read is `fix`-only: do not relax strict load globally. The lenient function is the single sanctioned bypass, mirroring how broken-frontmatter tolerance is confined to `fix`.
- Idempotency = compare-by-name + append-only of missing blocks + no rewrite when nothing missing. This guarantees both "no duplicates" (AC6) and "byte-unchanged on no-op", and preserves user-added relationships/rules.
- Config-only scope: the `--config` branch returns without invoking `collect_field_fixes`/`collect_conflict_fixes`/`collect_relation_fixes`, so no document is touched (AC4).

