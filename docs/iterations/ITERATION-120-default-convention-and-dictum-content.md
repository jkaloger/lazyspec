---
title: Default Convention and Dictum Content
type: iteration
status: accepted
author: agent
date: 2026-03-27
tags: []
related:
- implements: docs/stories/STORY-093-default-convention-and-dictum-content.md
---



## Changes

### Task 1: Add convention and dictum to default_types

ACs addressed: AC-1 (convention type entry), AC-2 (dictum type entry)

Files:
- Modify: `src/engine/config.rs`

`build_type_def` assumes `singleton: false` and `parent_type: None`. Convention and dictum need those fields set, so add them directly to `default_types()` rather than extending the helper. Construct two `TypeDef` literals in the `default_types` vec:

- Convention: `name = "convention"`, `plural = "convention"`, `dir = "docs/convention"`, `prefix = "CONVENTION"`, `icon = "📜"`, `singleton = true`, `parent_type = None`
- Dictum: `name = "dictum"`, `plural = "dicta"`, `dir = "docs/convention"`, `prefix = "DICTUM"`, `icon = "⚖"`, `parent_type = Some("convention")`, `singleton = false`

Both use `NumberingStrategy::default()` and `subdirectory: false`.

Verify: `cargo test config_test` passes, and `cargo run -- init --json` in a temp dir shows both types in the generated `.lazyspec.toml`.

### Task 2: Scaffold skeleton files during init

ACs addressed: AC-3 (index.md created), AC-4 (example.md created), AC-5 (date/author on index.md), AC-6 (date/author on example.md), AC-7 (no overwrite)

Files:
- Modify: `src/cli/init.rs`
- Modify: `tests/cli_init_test.rs`

After directory creation and before writing `.lazyspec.toml`, iterate over `config.documents.types` and scaffold skeleton files for types that define them. The approach:

1. Add a `skeleton_files` method (or similar) to `Config` or handle inline in `init::run`. For each type, check if it has a known skeleton. For now, hardcode the convention and dictum skeletons rather than building a general template system (YAGNI).

2. For the convention type (`singleton == true && name == "convention"`): write `docs/convention/index.md` with the frontmatter from the RFC (type: convention, status: draft, author: "unknown", date: today's date, tags: []) and the preamble body text.

3. For the dictum type (`parent_type == Some("convention") && name == "dictum"`): write `docs/convention/example.md` with the frontmatter from the RFC (type: dictum, status: draft, author: "unknown", date: today's date, tags: [example]) and the placeholder body text.

4. Before writing each file, check if it already exists. If so, skip it (AC-7).

5. Use `chrono::Local::now().format("%Y-%m-%d")` for the `{date}` substitution (the crate is already in use).

## Test Plan

### test: init creates convention skeleton files (AC-3, AC-4, AC-5, AC-6)

Call `init::run` on a fresh tempdir. Assert:
- `docs/convention/index.md` exists and contains `type: convention`, `status: draft`, `author: "unknown"`, and a valid date
- `docs/convention/example.md` exists and contains `type: dictum`, `status: draft`, `author: "unknown"`, `tags: [example]`, and a valid date
- Both files contain the expected body text

Isolated, deterministic, fast. Verifies the happy path for skeleton creation.

### test: init does not overwrite existing convention files (AC-7)

Create a tempdir, manually write `docs/convention/index.md` with custom content. Call `init::run`. Assert the file content is unchanged.

Isolated, deterministic. Tests the skip guard.

### test: default_types includes convention and dictum (AC-1, AC-2)

Load `Config::default()`. Find the convention and dictum entries in `config.documents.types`. Assert all field values match the AC specifications (name, plural, dir, prefix, icon, singleton, parent_type).

This can be added to the existing `tests/config_test.rs`.

## Notes

The skeleton content is taken verbatim from the RFC-034 "Default content" section. The `{date}` template variable is the only substitution needed; `{author}` is hardcoded to "unknown" per the ACs.

The skeleton approach is intentionally non-general. If future types need scaffolding, that can be extracted into a template system. For now, two hardcoded skeletons in `init.rs` are sufficient.
