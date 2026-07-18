---
title: 'Assignee core field: engine, stores, update flag, JSON'
type: iteration
status: accepted
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-222
- blocks: ITERATION-321
- blocks: ITERATION-322
---

## Objective

`assignee` first-class frontmatter field on `DocMeta`. Parse/serialize all local stores (filesystem, git-ref, cache). Settable via `update <id> --assignee <name>`. Surface in `show --json` + `status --json`. Optional, absent-from-frontmatter when unset.

## Satisfies

STORY-222 AC1, AC2, AC6. AC3/AC4/AC5 deferred — see Out of scope.

## Context

- Story + ACs: STORY-222.
- First-class, NOT custom attribute. STORY-150 / RFC-049 `--attr` is dynamic `attributes: BTreeMap` map. Assignee deliberately first-class: hardcoded field on `DocMeta` + every backend serializer, mirroring `tags`/`status`.
- Field type: `pub assignee: Option<String>` (single). Multi-assignee out of scope. AC6 absent-when-unset => `#[serde(default, skip_serializing_if = "Option::is_none")]` on serialize structs. NOTE divergence: `tags` emits `tags: []` always (no skip); assignee MUST skip when `None`.
- Touch engine: `src/engine/document.rs` — `DocMeta` (~L318), `RawFrontmatter` (~L339, `#[serde(default)]`), `parse_with_schema` (~L468), struct-literal sites (L497-513, ~L864). `src/engine/store_dispatch.rs` — `CacheFrontmatter` (L20-36, `skip_serializing_if`), `render_cache_content` (~L2205). `src/engine/fs_ops.rs` — `RESERVED_UPDATE_KEYS` (L289), `default_template` (L44). `src/engine/git_ref_store.rs` — `build_markdown` template (L120), `update` generic key/value path (L370).
- Touch CLI: `src/cli/update.rs` — add `--assignee <NAME>` flag pushing `("assignee", val)` into `updates` slice. `ops::update::run_with_config` is key-agnostic `&[(&str,&str)]` dispatch — NO `src/engine/ops/update.rs` change (no lifecycle gate for assignee). `src/cli/json.rs` — `doc_to_json` (L19-45) add `"assignee"` key (feeds show + status via `doc_to_json_with_family`).

## Tasks

1. Test-first (engine, `document.rs` tests): parse `assignee: alice` => `DocMeta.assignee == Some("alice")`; parse without key => `None` (AC6); serialize `None` => no `assignee:` line (AC6); round-trip preserve.
2. `document.rs`: add `pub assignee: Option<String>` to `DocMeta` (~L318) + `RawFrontmatter` (~L339, `#[serde(default)]`); assign in `parse_with_schema` (~L468); fix struct-literal sites (L497-513, ~L864).
3. Filesystem (`fs_ops.rs`): add `"assignee"` to `RESERVED_UPDATE_KEYS` (L289) for in-place line replace/insert; do NOT seed `assignee:` in `default_template` (L44) — absent-when-unset. Empty value clears line.
4. git-ref (`git_ref_store.rs`): `build_markdown` template (L120) omit `assignee` when `None`; verify generic `update` path (L370) sets scalar and empty string clears.
5. Cache (`store_dispatch.rs`): add `assignee` to `CacheFrontmatter` (L20-36) with `skip_serializing_if = "Option::is_none"`; populate in `render_cache_content` (~L2205) — else field dropped on materialized-backend cache write.
6. CLI (`update.rs`): `--assignee <NAME>` flag => `("assignee", value)` tuple into `updates`. Empty string clears.
7. CLI json (`json.rs`): `doc_to_json` (L19-45) add `"assignee"` (null when `None`).
8. Test-first (CLI integration): `update X --assignee alice` then `show X --json` and `status --json` show assignee; unset doc => null/absent.
9. Update README `update` command section: document `--assignee` flag + `assignee` in `show`/`status` JSON.

## Out of scope

- AC3/AC4 github-issues + clickup inherit-on-sync + write-through → next slice (blocks this).
- AC5 TUI / web / CLI display (list column + detail view) → surfaces slice.
- Assignee filtering (`store.rs` `Filter` L20-25) — STORY-222 out-of-scope.
- Multi-assignee — single `Option<String>` only.

## Principles / conventions

- CLAUDE.md: run dev via `cargo run`, `--json` machine output, update README on CLI change, account for engine change across tui/web/cli.
- Mirror existing first-class `tags`/`status` handling; do NOT model as `--attr` custom attribute.

## Verification

- Doc with no assignee: frontmatter has NO `assignee:` line (AC6); `show --json` assignee null.
- `update X --assignee bob`: file frontmatter gains `assignee: bob`; `show`/`status --json` reflect (AC1, AC2).
- git-ref-backed doc: settable via `update` only, no hand-edit (AC2).

