---
title: "Backfill priority via lazyspec fix"
type: iteration
status: complete
author: "agent"
date: 2026-04-30
tags: []
related: []
validate_ignore: true
---



## Summary

Standalone iter. Extends `lazyspec fix` to backfill missing `priority` frontmatter on docs whose type has `requires_priority=true`. Default fill value: `should` (mid-tier MoSCoW). Iter 164 introduced the validation rule but left ~286 legacy docs in error state; this closes that gap via tooling, not a one-shot script.

No new RFC/Story. Extends existing CLI command. Engine + config layers already done in iter 164.

## Acceptance Criteria

- AC1: doc of `requires_priority=true` type w/ no `priority:` field → after `lazyspec fix`, file has `priority: should` inserted in frontmatter; body unchanged.
- AC2: doc of `requires_priority=true` type w/ existing `priority: must` → fix is no-op (no overwrite).
- AC3: doc of `requires_priority=false` type w/ no `priority:` field → fix is no-op (no spurious insert).
- AC4: `--dry-run` flag → reports planned fills, file unchanged on disk.
- AC5: `--json` output reports each backfill in `field_fixes[*].fields_added` w/ entry `priority`.
- AC6: post-fix on a repo w/ all required docs covered → `lazyspec validate` reports zero `priority field required` errors.

## Test Plan

All tests in `src/cli/fix/fields.rs` `#[cfg(test)] mod tests` + integration in `tests/cli_fix_test.rs`. Use `tempfile::TempDir` + `FileSystem` trait per DICTUM-004. Deterministic. Reuse iter 164 `Config::default()` (story/iteration require priority, audit doesn't).

### Unit (`src/cli/fix/fields.rs`)

- AC1 — fixture story doc lacking `priority:`. Run `collect_priority_fills(...)`. Assert returned `FieldFixResult.fields_added == ["priority"]` and post-write file YAML parses w/ `priority == "should"`.
- AC2 — fixture story doc w/ `priority: must`. Run fill. Assert `fields_added` empty, file unchanged.
- AC3 — fixture rfc doc (requires_priority=false) w/ no priority. Run fill. Assert `fields_added` empty.

### Integration (`tests/cli_fix_test.rs`)

- AC4 — TempDir w/ story missing priority. Run `lazyspec fix --dry-run --json`. Assert exit 0, JSON shows planned fill, file content on disk unchanged.
- AC5 — same fixture, `lazyspec fix --json` (no dry-run). Assert JSON `field_fixes[0].fields_added` contains `"priority"`, file has `priority: should` after.
- AC6 — TempDir w/ 2 stories + 1 iteration missing priority + 1 rfc (none required). Run `lazyspec fix`. Run `lazyspec validate --json`. Assert `errors` filtered by substring `priority field required` is empty.

## Changes

Tasks self-contained for zero-context subagent.

### 1. Extend `collect_field_fixes` to source from `store.docs` for priority

- ACs: 1, 2, 3.
- File: `src/cli/fix/fields.rs`.
- Intent:
  - Today `collect_field_fixes` iterates `store.parse_errors()` only. Priority backfill needs to walk parsed-but-incomplete docs. Add a second pass: iterate `store.docs.values()`, filter out `validate_ignore`, look up `td = config.type_by_name(doc.doc_type.as_str())`, if `td.resolved_requires_priority() && doc.priority.is_none()` → insert `priority: should` into frontmatter mapping.
  - Reuse existing `fix_file` write path (read content → split frontmatter → mutate mapping → write). Add a sibling fn `fix_priority_file(root, doc_path, dry_run, fs)` or extend `fix_file` w/ a `mode` enum. Pick whichever is fewer lines (Principle 6).
  - Append to existing `FieldFixResult.fields_added` w/ `"priority"`. No new struct.
  - Default value `should` is hardcoded in this iter. No CLI flag (Principle 6 — one consumer).
- Verify: `cargo test --lib cli::fix::fields::tests` covers AC1-3.

### 2. Wire priority pass into `fix::run`

- ACs: 4, 5, 6.
- File: `src/cli/fix.rs`.
- Intent:
  - In `run`, after `collect_field_fixes` for parse-error docs, call the new priority-pass fn (or pass a flag into `collect_field_fixes` indicating it should also walk valid docs for priority). Merge results into `output.field_fixes`. De-dupe by path (don't double-insert if a doc both has parse errors AND missing priority).
  - `dry_run` flow already correct — fix_file checks the flag before writing. Same for priority pass.
  - JSON serialisation auto-flows since `FieldFixResult` is reused.
- Verify: `cargo test --test cli_fix_test` covers AC4, AC5, AC6.

### 3. Backfill the repo

- ACs: 6 (concrete).
- Files: all `docs/stories/*.md` + `docs/iterations/*.md` missing `priority:`.
- Intent:
  - After tasks 1-2 land + green: run `cargo run -- fix --json`. Verify dry-run first, then commit-on-disk pass.
  - Post-fix run `cargo run -- validate --json | jq '[.errors[] | select(. | contains("priority field required"))] | length'` → expect `0`.
- Verify: validate output as above; git diff shows only `priority: should` insertions on legacy docs.

## Notes

### Decisions (locked)

- **Default value `should`** — mid-tier MoSCoW, semantically neutral. Not `must` (would skew priority-based ordering). Not `could` (signals deferrable). Not `wont` (signals deprioritised). `should` = "do unless something pre-empts" matches default expectation for legacy docs that pre-date the field.
- **Hardcode default in fields.rs, no CLI flag** — Principle 6. One consumer (this backfill). Promote to flag or per-type config when second use case arrives.
- **Reuse `FieldFixResult`** — same shape (path + fields_added + written). No new variant.
- **No overwrite** — AC2 mandates idempotency. `priority` present (any value) → skip.

### Open

- Whether to add a `--priority <key>` flag now anyway. Deferred. Reopen if a user wants different defaults per repo.

### Codebase anchors

- Existing `collect_field_fixes`: `src/cli/fix/fields.rs:12`.
- `fix_file` write path: `src/cli/fix/fields.rs:36`.
- `REQUIRED_FIELDS` constant: `src/cli/fix/fields.rs:10` (this iter does NOT add `priority` here — it only applies to `requires_priority` types, not all docs).
- `run` orchestration: `src/cli/fix.rs:87`.
- Store iteration pattern: `store.docs.values()` (verify exact field name during impl).
- `td.resolved_requires_priority()`: `src/engine/config.rs` (iter 164).
- Priority validation rule: `src/engine/validation.rs::PriorityRule` (iter 164).
