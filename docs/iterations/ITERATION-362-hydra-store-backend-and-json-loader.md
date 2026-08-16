---
title: Hydra store backend and JSON loader
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-08-17
tags: []
related:
- implements: STORY-252
- blocks: ITERATION-363
---

## Objective

`.hydra/*.json` trees load as documents with correct ids and derived status, so `lazyspec list` shows them.

## Satisfies

STORY-252 AC1, AC3, AC5, AC8, AC9, AC10. AC2, AC4, AC6, AC7, AC11 deferred — see Out of scope.

## Context

- Story + ACs: STORY-252
- Design, id scheme rationale, failure-mode rules, config defaults: RFC-066 §Design
- Interview record: `.hydra/hydra-store.json`
- Conventions: CONVENTION-001 and its dicta (`lazyspec convention`)
- Touch: `src/engine/config.rs` (`StoreBackend`, `Display`, `canonical_lifecycle`), `src/engine/store.rs` (`load_with_fs` dispatch, path resolution), `src/engine/store/hydra.rs` (new), `src/engine/store.rs` match sites and any other `StoreBackend` match arms the compiler surfaces

## Tasks

1. Add the `Hydra` variant to `StoreBackend` and fix every match site the compiler reports. It resolves to `root.join(&type_def.dir)`, not `.lazyspec/cache`.
2. Define the deserialization types for a hydra tree JSON (tree slug, intent, heads with slug/question/answer/rationale/rejected/parent/blocked_by/cauterised_by). Derive them from the actual `.hydra/hydra-store.json` in this repo, and tolerate unknown fields so a newer hydra schema does not fail to parse where it does not have to.
3. Write `src/engine/store/hydra.rs` with a `load_hydra_directory` reading every `*.json` in the dir through the `&dyn FileSystem` already threaded into `load_with_fs`. Set `DocMeta.id`, `title`, `path`, `doc_type`, `status`, `virtual_doc` directly — do not route through `extract_id`.
4. Implement id derivation and status derivation as separately testable functions.
5. Wire the failure modes: missing dir short-circuits to zero docs via the existing `fs.exists` guard; a per-file parse failure pushes a `ParseError` and continues to the next file.
6. Add the hydra `[[types]]` entry to this repo's `.lazyspec.toml` via `lazyspec config` so the feature is dogfooded.
7. Tests through the fake `FileSystem` and `Store::load_with_fs`: id casing, all three status derivations, missing dir, one bad file among good ones.

## Out of scope

- Body rendering, including the ASCII tree → next iteration. This iteration may leave the body empty or minimal; AC2 and AC4 are not satisfied here.
- AC6 (file watch), AC7 (web/graph id resolution), AC11 (read-only enforcement) → third iteration.
- STORY-253 entirely.

## Principles/conventions

`lazyspec convention` — in particular the engine/CLI/TUI layering rule and the "no indirection until two uses" dictum. The test seam is the existing `FileSystem` trait; do not add a `HydraOps` trait (RFC-066 §Test seam).

## Verification

`cargo run -- list --type hydra --json` lists `HYDRA-HYDRA-STORE` with status `complete`, and nothing appears under `.lazyspec/cache/`.

