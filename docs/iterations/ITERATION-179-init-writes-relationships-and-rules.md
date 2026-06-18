---
title: Init writes relationships and rules
type: iteration
status: accepted
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-127
---

## Context

ADR-011 makes config load strict: the engine carries no built-in document types, relationship vocabulary, or validation rules. A loaded project that does not declare `[[types]]` and `[[relationships]]` is a hard error. `init` is therefore the sole home for the starter set, and a fresh checkout must behave identically to the pre-refactor builtins.

This iteration makes `init`'s scaffold emit two new config blocks: `[[relationships]]` (the 4 historical relationships + inverses) and `[[rules]]` (the 3 historical rules). It implements STORY-127 under RFC-042.

Dependency on STORY-126: this story consumes the config shapes STORY-126 defines. STORY-126 introduces the `[[relationships]]` TOML block and its `RelationshipDef { name: String, inverse: Option<String> }` shape, and makes `[[types]]`/`[[rules]]`/`[[relationships]]` serializable (today `types` and `rules` carry `#[serde(skip)]` in `src/engine/config.rs`, so `Config::to_toml()` emits neither — it emits `[directories]` instead). The contract this iteration must emit:

```toml
[[relationships]]
name = "implements"
inverse = "implemented-by"   # omit => symmetric

[[rules]]
shape = "parent-child"       # serde tag on ValidationRule
child = "story"
parent = "rfc"
link = "implements"
severity = "warning"
```

Init must write blocks matching these shapes. This iteration does NOT define the shapes or the strict-load path (STORY-126 / STORY-125), does NOT touch the engine defaults, and does NOT migrate existing configs (STORY-128).

## Test Plan

All tests are CLI/integration tests in `tests/integration/cli_init_test.rs`, each with its own `TempDir` (DICTUM-004: isolated, behavioral, deterministic, fast). They drive `lazyspec::cli::init::run(root)` and assert on the written `.lazyspec.toml`, then re-parse/load it. No test code is written in this iteration; the descriptions below are the plan.

- **AC1 — `init` emits the 4 historical relationships with inverses.**
  - Name: `init_writes_relationships_block`.
  - Arrange: new `TempDir`, empty dir, no `.lazyspec.toml`.
  - Act: `init::run(root)`; read `.lazyspec.toml`.
  - Assert: parse the TOML (via `Config::parse`) and assert the relationship registry contains exactly `implements` (inverse `implemented-by`), `supersedes` (inverse `superseded-by`), `blocks` (inverse `blocked-by`), `related-to`. Assert the raw text contains a `[[relationships]]` table. Prefer asserting on the parsed registry over substring matching so formatting changes don't break the test, but include one substring check on `[[relationships]]` to pin the block exists.

- **AC2 — `related-to` is symmetric (no inverse).**
  - Name: `init_related_to_is_symmetric_no_inverse`.
  - Arrange/Act: as AC1.
  - Assert: in the parsed registry, the `related-to` entry has `inverse == None`. Additionally assert the serialized text does not attach an `inverse` key to the `related-to` table (no `inverse` line within the related-to block).

- **AC3 — `init` emits the 3 historical rules.**
  - Name: `init_writes_rules_block`.
  - Arrange/Act: as AC1.
  - Assert: re-parse and assert `config.rules` equals (by value, `ValidationRule` derives `PartialEq`) the three historical rules: `ParentChild { name: "stories-need-rfcs", child: "story", parent: "rfc", link: "implements", severity: Warning }`, `ParentChild { name: "iterations-need-stories", child: "iteration", parent: "story", link: "implements", severity: Error }`, `RelationExistence { name: "adrs-need-relations", doc_type: "adr", require: "any-relation", severity: Error }`. Also assert raw text contains `[[rules]]`. This is the strongest guard that init matches `default_rules()` verbatim.

- **AC4 — an init-ed project loads under strict load and validates identically to pre-refactor builtins.**
  - Name: `init_project_loads_strict_and_validates_clean`.
  - Arrange: new `TempDir`; `init::run(root)`.
  - Act: load config from the written file (the strict load path from STORY-126/125 — assert it returns `Ok`, i.e. no missing-`[[types]]`/`[[relationships]]` strict-load error). Then run `validate` over the freshly init-ed project.
  - Assert: load succeeds (`is_ok()`), and the parsed `config.documents.types` + `config.rules` + relationship registry equal the historical `default_types()` / `default_rules()` / 4-relationship set respectively (round-trip equality = "identical to pre-refactor builtins"). Validate over the empty-but-scaffolded project produces no NEW errors attributable to a missing taxonomy (the convention/dictum skeletons init already writes are the only docs; assert no strict-load or unknown-relationship errors). This is the key behavioral test that the emitted set is complete and consistent.

- **AC5 — `init` refuses when `.lazyspec.toml` already exists (unchanged).**
  - Name: `init_does_not_overwrite_existing_config` (already exists in `cli_init_test.rs`; keep/extend).
  - Arrange: write a sentinel `.lazyspec.toml` (e.g. `"# custom config"`).
  - Act: `init::run(root)`.
  - Assert: returns `Err` whose message contains `already exists`; the sentinel file is byte-for-byte unchanged (read back and `assert_eq!` to the sentinel) — proves init wrote nothing.

Note: the existing `init_creates_config_and_directories` test asserts the file `contains("[directories]")`. STORY-126 changes the emitted shape from `[directories]` to `[[types]]`; coordinate that assertion with STORY-126. For THIS iteration, do not regress that test — if STORY-126 has already flipped it, the relationships/rules assertions slot in alongside; if not, the new `[[relationships]]`/`[[rules]]` blocks are additive and that test still passes.

## Changes

The single source of truth for what `init` writes is `Config::to_toml()` (`src/engine/config.rs`), invoked by `init::run` at `src/cli/init.rs:23` (`fs::write(&config_path, config.to_toml()?)`). `init` does not hand-roll a template string — it serializes `Config::default()`. Therefore the work is to ensure `Config::default()` carries the relationships + rules AND that they serialize (STORY-126 removes the `#[serde(skip)]`). The relationship + rule *content* is copied verbatim from the historical `default_rules()` and the historical relationship enum.

1. **(AC1, AC2) Ensure `Config::default()` carries the 4 historical relationships and they serialize into `[[relationships]]`.** File: `src/engine/config.rs`. Add a `default_relationships()` helper returning the registry STORY-126's `RelationshipDef` describes, populated from the historical `RelationType` enum + inverse aliases (`src/engine/document.rs`):
   - `implements` -> inverse `implemented-by`
   - `supersedes` -> inverse `superseded-by`
   - `blocks` -> inverse `blocked-by`
   - `related-to` -> no `inverse` (symmetric)
   Wire it into `Config::default()` alongside `default_rules()`. The field must be serializable (STORY-126 provides the field + serde shape; this story populates it). The `related-to` `RelationshipDef.inverse` must be `None` so `#[serde(skip_serializing_if = "Option::is_none")]` (or equivalent provided by STORY-126) omits the key in TOML.
   - Verify: `cargo run -- init` in a temp dir; inspect `.lazyspec.toml` for a `[[relationships]]` array with 4 entries and an `inverse` on all but `related-to`.

2. **(AC3) Ensure `Config::default()`'s `rules` serialize into `[[rules]]` with the 3 historical rules verbatim.** File: `src/engine/config.rs`. `default_rules()` already returns the exact three (lines ~385-408). The only change required here is that `rules` serializes (STORY-126 drops `#[serde(skip)]` from `Config.rules`). Confirm the serialized TOML matches the `ValidationRule` serde tagging (`#[serde(tag = "shape")]`, variant renames `parent-child` / `relation-existence`, `#[serde(rename = "type")]` for `doc_type`, lowercase `severity`). Emitted blocks:
   - `shape="parent-child"  name="stories-need-rfcs"      child="story"     parent="rfc"   link="implements" severity="warning"`
   - `shape="parent-child"  name="iterations-need-stories" child="iteration" parent="story" link="implements" severity="error"`
   - `shape="relation-existence" name="adrs-need-relations" type="adr" require="any-relation" severity="error"`
   - Verify: `cargo run -- init` in temp dir; inspect for 3 `[[rules]]` tables; `Config::parse` the file and assert `rules == default_rules()`.

3. **(AC4) Confirm a freshly init-ed project loads under strict load and round-trips identically to builtins.** Files: `src/engine/config.rs` (serialization), exercised through `src/cli/init.rs`. With blocks 1+2 emitting, re-parsing the written file under the STORY-125/126 strict load path must yield `types == default_types()`, `rules == default_rules()`, and the relationship registry == the 4 historical relationships. No code change beyond 1+2 is expected; this task is the verification gate. If round-trip reveals a mismatch (e.g. a serde field name drift, or `[directories]` still being emitted and shadowing `[[types]]`), fix the serialization in `config.rs` so init emits the canonical `[[types]]`/`[[relationships]]`/`[[rules]]` set.
   - Verify: `cargo run -- init` in temp dir, then `cargo run -- validate --json` from that dir — assert `errors`/`parse_errors` arrays are empty (no strict-load/unknown-relationship errors). The validate JSON shape is `{ errors, warnings, parse_errors }`.

4. **(AC5) Preserve the "already exists" guard.** File: `src/cli/init.rs:9-12` (`if config_path.exists() { bail!(".lazyspec.toml already exists"); }`). No change — this story must not alter it. Task exists to pin the behavior with the AC5 test (refuses + writes nothing).
   - Verify: in a temp dir containing a sentinel `.lazyspec.toml`, `cargo run -- init` exits non-zero with "already exists" and leaves the sentinel untouched.

5. **(AC1-AC4) Add/extend integration tests in `tests/integration/cli_init_test.rs`.** Implement the AC1-AC4 tests described in the Test Plan (AC5 test already present; extend it to assert the sentinel is unchanged). Mirror the existing style: `TempDir`, `lazyspec::cli::init::run(root)`, read file, `Config::parse`, assert on parsed values. Reconcile the existing `init_creates_config_and_directories` assertion (`contains("[directories]")`) with whatever STORY-126 lands (`[[types]]`).
   - Verify: `cargo test --test integration cli_init` passes.

Implementation order: 1 -> 2 -> 3 (verification gate) -> 5 (tests) -> 4 (pin). Blocks 1 and 2 depend on STORY-126 having removed `#[serde(skip)]` from `Config.rules` and added the `[[relationships]]` field/serde; if STORY-126 is not yet merged, these tasks are blocked on it.

## Notes

Real file paths (all verified to exist):
- `src/cli/init.rs` — `init::run`; writes `config.to_toml()?` to `.lazyspec.toml` at line 23. No template string; init serializes `Config::default()`. The "already exists" guard is lines 9-12. Convention/dictum skeletons are written by `scaffold_skeleton_files` (unaffected).
- `src/engine/config.rs` — `default_types()` (line 351), `default_rules()` (line 385), `Config::default()` (line 435), `Config::to_toml()` (line 592). `ValidationRule` enum + serde tagging is lines 13-32. `Severity` lines 6-11.
- `src/engine/document.rs` — historical relationship vocabulary: `RelationType` enum (line 116), `ALL_STRS = ["implements","supersedes","blocks","related-to"]` (line 131), `INVERSE_STRS = ["implemented-by","superseded-by","blocked-by"]` (line 133), `resolve_rel_keyword` (line 145) maps inverse aliases to canonical+flipped, `related-to` symmetric. These are the verbatim source for `default_relationships()`.
- `src/engine/validation.rs` — consumes `config.rules` (lines ~259, ~391); the existing rule semantics must be preserved by the emitted `[[rules]]`.
- `tests/integration/cli_init_test.rs` — existing init integration tests to mirror/extend.

Discovered relationship definitions (verbatim, for `default_relationships()`):
| name | inverse |
| --- | --- |
| implements | implemented-by |
| supersedes | superseded-by |
| blocks | blocked-by |
| related-to | (none — symmetric) |

Discovered rule definitions (verbatim, from `default_rules()`):
| shape | name | fields | severity |
| --- | --- | --- | --- |
| parent-child | stories-need-rfcs | child=story, parent=rfc, link=implements | warning |
| parent-child | iterations-need-stories | child=iteration, parent=story, link=implements | error |
| relation-existence | adrs-need-relations | type=adr, require=any-relation | error |

Key decisions / findings:
- **The story premise that "init already writes the `[[types]]` block" does not match current `main`.** Today `init` emits `[directories]` (via `Directories`), and `types`/`rules` carry `#[serde(skip)]`, so neither is serialized. Making `[[types]]`/`[[rules]]`/`[[relationships]]` serializable is STORY-126's job; this iteration assumes that and adds the relationship + rule content. If STORY-126 lands a different field name for the relationship registry than assumed, adjust `default_relationships()` accordingly — the contract is the TOML shape in Context, not a Rust field name.
- **No hand-rolled template.** Because init serializes `Config::default()`, the cleanest implementation is to populate `Config::default()` and rely on `to_toml()`; this keeps init and the loaded-config view in lockstep and is what makes AC4's round-trip equality hold by construction.
- **`related-to` symmetry** is expressed as `inverse: None` and must be omitted from the TOML (not written as empty), matching `resolve_rel_keyword`'s treatment of `related-to` as having no inverse keyword.
- Baseline `cargo run -- validate --json` on this repo is clean (errors/warnings/parse_errors all empty), so AC4's "no new errors" assertion has a clean reference.

