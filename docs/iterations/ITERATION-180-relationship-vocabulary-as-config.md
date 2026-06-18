---
title: Relationship vocabulary as config
type: iteration
status: accepted
author: agent
date: 2026-06-18
tags: []
related:
- implements: STORY-126
---

## Context

STORY-126 (under RFC-042, ADR-010) brings relationships to parity with doc types: `RelationType` stops being a closed Rust enum (`Implements`, `Supersedes`, `Blocks`, `RelatedTo`) and becomes a config-driven `RelationType(String)` newtype mirroring `DocType(String)`. A project declares its own vocabulary in a `[[relationships]]` block (`RelationshipDef { name, inverse: Option<String> }`), `link`/`unlink` resolve inverses from that registry instead of hardcoded `INVERSE_STRS`/`resolve_rel_keyword`, `validate` flags docs carrying undeclared relationship names, and a missing `[[relationships]]` block is a hard load error.

**This is the heavy slice: the ~89-site enum → newtype refactor (103 textual references to `RelationType` across ~12 files) spans all three layers (engine, CLI, TUI) and MUST land as one atomic compile unit.** You cannot ship a half-converted enum. Verified file groups carrying `RelationType`:

- **Engine**: `src/engine/document.rs` (the enum + `FromStr`/`Display`/`resolve_rel_keyword`/`ALL_STRS`/`INVERSE_STRS`/`parse_relation`), `src/engine/store.rs`, `src/engine/store/links.rs`, `src/engine/store_dispatch.rs`, `src/engine/context.rs`, `src/engine/issue_body.rs`, `src/engine/config.rs` (new registry), `src/engine/validation.rs` (new check).
- **CLI**: `src/cli/link.rs`, `src/cli/completions.rs`.
- **TUI**: `src/tui/state/forms.rs` (`REL_TYPES` const), `src/tui/state/graph.rs`, `src/tui/state/app.rs`.
- **Tests** (must be updated in lockstep so the suite compiles): `tests/integration/document_test.rs`, `tests/integration/cli_link_test.rs`, `tests/integration/spec_type_test.rs`, `tests/integration/tui_link_editor_test.rs`.

**Three-layer arch reminder (DICTUM, RFC-042):** engine carries no I/O assumptions; CLI and TUI depend on engine, never on each other. The registry lives on `Config` (engine); `link`/`validate` read it through the engine; TUI/CLI render from it. Do not introduce a CLI→TUI or TUI→CLI dependency while wiring the registry.

**Non-mechanical wrinkle (verified, must be called out):** RFC-042 claims `context.rs` "walks relationships generically (no type literals)". The actual code does NOT — `src/engine/context.rs` compares `rel.rel_type != RelationType::Implements` (chain traversal) and `*rel_type == RelationType::RelatedTo` (related ring), and `src/tui/state/graph.rs` filters on `RelationType::RelatedTo`, `src/engine/context.rs`/`store_dispatch.rs` construct `RelationType::Implements`. After the newtype lands these become string-literal comparisons/constructions (`RelationType::new("implements")`, `== &RelationType::new("related-to")`). The chain/graph keeping a literal coupling to the names `"implements"`/`"related-to"` is acceptable and IN PARITY with how `context.rs` is already coupled today; making chain-link names configurable is explicitly OUT OF SCOPE for this story. Just convert variants to string-literal newtypes; do not try to generalize traversal.

## Test Plan

All tests follow DICTUM-004: each owns its own `TempDir`/`Store` (isolated), asserts on public-API behavior and serialized output (structure-insensitive, behavioral), no wall-clock/network (deterministic, fast). Integration tests live in `tests/integration/` and use `crate::common::TestFixture`. Engine unit tests (`Config::parse`, newtype) stay inline in their module.

Seam note / tradeoff: `TestFixture` currently does not write a `.lazyspec.toml` and `TestFixture::config()` returns `Config::default()`. AC1–AC5/AC7 need a custom registry, and AC6 needs a config *file* present-but-missing-`[[relationships]]`. The cleanest seam is to add `TestFixture` helpers (`write_config(toml: &str)` returning a parsed `Config`, and a `config_with_relationships(...)` convenience) rather than hand-rolling TOML in each test. AC1–AC5 exercise the engine `link`/`unlink`/`validate` functions directly with a `Config` carrying the custom registry (fast, no process spawn); AC6/AC7 are better as `Config::parse` unit tests + one CLI `--json` integration test through `Command` to prove the wiring end-to-end.

- **AC1 — custom name links and writes to frontmatter.**
  Test `link_with_custom_relationship_name_writes_frontmatter` (new, `tests/integration/cli_link_test.rs`).
  Arrange: `TestFixture` with two docs; a `Config` whose registry declares `tracks` (a name absent from the old enum), no inverse. Act: call engine `link_with_config(root, store, A, "tracks", B, fs, Some(&config))`. Assert: re-parsing A's frontmatter yields one relation with `rel_type` displaying `"tracks"` and `target == B`'s id. Tradeoff: drives the engine fn directly (fast); a sibling `--json` CLI smoke is covered under AC7.

- **AC2 — unknown name on `link` rejected, nothing written.**
  Test `link_rejects_relationship_absent_from_registry` (new, `cli_link_test.rs`).
  Arrange: `Config` registry with only `implements`; two docs. Act: `link_with_config(..., "tracks", ...)`. Assert: returns `Err`; error string contains `"tracks"`; A's frontmatter is byte-for-byte unchanged (read file before/after, assert equal) — proves "no frontmatter written". Tradeoff: byte-compare is the structure-insensitive way to assert non-mutation without coupling to YAML shape.

- **AC3 — `validate` flags a doc carrying an undeclared relationship name.**
  Test `validate_flags_undeclared_relationship_name` (new, `tests/integration/cli_validate_test.rs`).
  Arrange: write a doc whose frontmatter has `related: [{ tracks: RFC-001 }]`; a `Config` registry that does NOT contain `tracks` (e.g. only `implements`, `related-to`) plus the RFC-001 target so this isn't a broken-link false positive. Act: `store.validate_full(&config)`. Assert: `result.errors` contains the new issue variant (e.g. `UnknownRelationship { path, name }`) naming `"tracks"` and the doc path; assert via `matches!` on the variant, not on the Display string. Tradeoff: a new `ValidationIssue` variant is the structure-insensitive assertion target (mirrors existing `MissingRelation`/`MissingParentLink` tests).

- **AC4 — declared inverse stores once on opposite doc, direction flipped, inverse from config.**
  Test `link_inverse_keyword_flips_using_config_inverse` (new, `cli_link_test.rs`).
  Arrange: registry declares `implements` with `inverse = "implemented-by"`; docs A (adr) and B (rfc). Act: `link_with_config(root, store, B, "implemented-by", A, fs, Some(&config))` — i.e. user types the inverse with B first. Assert: B's frontmatter is unchanged; A's frontmatter gained exactly one relation `implements: B` (stored once, on the opposite doc, flipped). Second assertion: a registry where the inverse name differs from the old hardcoded one (e.g. `tracks`/`tracked-by`) flips correctly — proving the source is config, not `INVERSE_STRS`. Tradeoff: the `tracks`/`tracked-by` case is the load-bearing one (a name the old code could never have known); keep the `implements` case too as a regression guard for #55 behavior.

- **AC5 — relationship with no inverse is symmetric; no separate inverse keyword.**
  Test `link_symmetric_relationship_has_no_inverse_keyword` (new, `cli_link_test.rs`).
  Arrange: registry declares `related-to` with no `inverse`. Act 1: `link_with_config(..., "related-to", ...)` succeeds and stores `related-to: B` on A, not flipped. Act 2: attempt to link using a would-be inverse (e.g. `related-to-by`) → `Err` (no inverse keyword exists/accepted for a symmetric rel). Assert both. Tradeoff: asserting the negative (inverse keyword rejected) is what distinguishes symmetric from directional; without it AC5 is just "a link works".

- **AC6 — missing `[[relationships]]` block is a hard load error.**
  Test `parse_without_relationships_block_is_hard_error` (new, inline in `src/engine/config.rs` tests, mirroring the existing `numbering`/`github` `bail!` tests).
  Arrange: a TOML string with `[[types]]` (and naming) but NO `[[relationships]]`. Act: `Config::parse(toml)`. Assert: returns `Err`; message mentions `[[relationships]]` (and per ADR-012, ideally names `fix` as the remedy — assert on the `[[relationships]]` substring to stay robust). Companion: `parse_with_relationships_block_succeeds` (happy path). Tradeoff: this is a `parse` test not a `load` test because `load` falls back to `Config::default()` when no file exists; the strict error must fire when a file IS present but the block is absent, which is exactly `parse`. Note: `Config::default()` must continue to carry a built-in relationships registry so the ~existing tests using `TestFixture::config()` (= `Config::default()`) keep compiling/passing — the strict error is a `parse`-path concern only.

- **AC7 — `--json` serializes relationships under the configured name.**
  Test `json_output_serializes_relationship_by_configured_name` (new, `tests/integration/cli_json_test.rs`), end-to-end via `Command`.
  Arrange: a real temp project with `.lazyspec.toml` declaring `[[relationships]] name = "tracks"`, plus two docs, with A already carrying `related: [{ tracks: B }]`. Act: run the dev binary `show A --json` (spawned process). Assert: parse stdout JSON; `related[0].type == "tracks"`. Tradeoff: spawning the process is slower but is the only way to prove the full load→serialize path honors the configured name; the rest of the ACs stay in-process. This AC is mostly free once the newtype's `Display` returns the configured string, since `src/cli/json.rs` already serializes via `format!("{}", r.rel_type)` — the test guards against regression.

## Changes

Tasks are sequenced; (a)–(b) are the atomic core and must compile together before any later task. A subagent should treat (a)+(b) as a single landing.

### Task 1 — `RelationshipDef` + registry on `Config`, parse `[[relationships]]`
**ACs:** 1, 6 (foundation for all).
**Files:** `src/engine/config.rs`.
**Do:**
- Add `pub struct RelationshipDef { pub name: String, pub inverse: Option<String> }` (derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`).
- Add a registry field to `Config` — `pub relationships: Vec<RelationshipDef>` (decide serde: mirror how `rules`/`types` are handled; `rules` is `#[serde(skip)]` and rebuilt in `parse`, `types` is real serde via `RawConfig`). Add `relationships: Option<Vec<RelationshipDef>>` to `RawConfig`.
- In `Config::parse`: if `raw.relationships` is `None`, `bail!("[[relationships]] section is required; run `lazyspec fix --config` to add the standard set")` (AC6). If present, store it.
- Add lookup helpers on `Config`: `relationship_by_name(&self, name: &str) -> Option<&RelationshipDef>`; `inverse_of(&self, name: &str) -> Option<&str>` (the declared inverse for a canonical name); and a resolver `resolve_relationship(&self, keyword: &str) -> Result<(String /*canonical name*/, bool /*flipped*/)>` that: returns `(name, false)` if `keyword` matches a declared `name`; returns `(name, true)` if `keyword` matches some declared `inverse`; else `Err` naming the unknown keyword. This replaces `resolve_rel_keyword` with a config-sourced version.
- Update `Config::default()` to populate `relationships` with the built-in set mirroring today's hardcoded vocabulary: `implements`/`implemented-by`, `supersedes`/`superseded-by`, `blocks`/`blocked-by`, `related-to` (no inverse). This keeps every test using `Config::default()` green.
**Verify:** `cargo build`; new inline `config.rs` tests for AC6 (`parse_without_relationships_block_is_hard_error`, `parse_with_relationships_block_succeeds`) plus a `resolve_relationship` table test (canonical not-flipped, inverse flipped, symmetric has no inverse, unknown errors). `cargo test -p lazyspec config`.

### Task 2 — Replace the `RelationType` enum with `RelationType(String)` newtype + mechanical call-site refactor (ATOMIC)
**ACs:** 1, 4, 5, 7 (and unblocks 2, 3).
**Files (one landing):** `src/engine/document.rs` (definition), then the call sites: `src/engine/store.rs`, `src/engine/store/links.rs`, `src/engine/store_dispatch.rs`, `src/engine/context.rs`, `src/engine/issue_body.rs`, `src/cli/link.rs`, `src/cli/completions.rs`, `src/tui/state/forms.rs`, `src/tui/state/graph.rs`, `src/tui/state/app.rs`, and tests `tests/integration/document_test.rs`, `tests/integration/cli_link_test.rs`, `tests/integration/spec_type_test.rs`, `tests/integration/tui_link_editor_test.rs`.
**Do:**
- In `document.rs`: replace `enum RelationType { ... }` and its `impl` with `pub struct RelationType(String)` mirroring `DocType` exactly: `new(&str)` (lowercasing), `as_str`, `Display` writing the inner string, manual `Deserialize` (lowercasing), `FromStr` that is PURE (`Ok(RelationType::new(s))`, never errors — validation moves to `link`/`validate`). Derive `Debug, Clone, PartialEq, Eq, Hash, Serialize`. Keep `Relation { rel_type: RelationType, target: String }` and `parse_relation` (its `key_str.parse()` now always succeeds).
- DELETE from `document.rs`: `RelationType::ALL`, `ALL_STRS`, `INVERSE_STRS`, `ResolvedRelKeyword`, `resolve_rel_keyword`, and the enum-specific unit tests (`canonical_keywords_resolve_*`, `inverse_keywords_*`, `related_to_*`, `unknown_keyword_*`, `keyword_resolution_*`) — their behavior moves to `Config::resolve_relationship` tests (Task 1) and `link` tests (Task 3).
- Mechanically convert every `RelationType::Implements` → `RelationType::new("implements")` (and `Supersedes`/`Blocks`/`RelatedTo` → `"supersedes"`/`"blocks"`/`"related-to"`) at construction and comparison sites. **Specifically:** `context.rs` lines comparing/constructing `Implements` and `RelatedTo`; `graph.rs` filter on `RelatedTo`; `store_dispatch.rs` test constructing `Implements`; `issue_body.rs` tests; `app.rs:1476` `prefix.trim().parse()?` (now infallible — drop the `?` or keep, it still typechecks). For comparisons prefer `rel.rel_type.as_str() == "implements"` for clarity, or `== &RelationType::new("implements")`.
- `src/tui/state/forms.rs` `REL_TYPES`: this `const` built from `ALL_STRS`+`INVERSE_STRS` cannot survive (no consts on a `String` newtype). Replace with a runtime list derived from the `Config` relationships registry (each `name`, plus each `inverse` where present) — thread the registry into wherever `REL_TYPES` is consumed. This is the one TUI site that is NOT purely mechanical; keep it engine-agnostic (TUI reads `Config`, does not import CLI).
- `src/cli/completions.rs:40-42`: replace `RelationType::ALL_STRS.chain(INVERSE_STRS)` with the names+inverses from the loaded `Config` registry (it already does `Config::load`).
- Update the lockstep tests: `document_test.rs` (delete/convert `relation_type_display`, `relation_type_fromstr_*`, `ALL_STRS` loop — replace with newtype roundtrip tests asserting `RelationType::new("anything").to_string() == "anything"` and `FromStr` never errors); convert `RelationType::Implements` etc. in `cli_link_test.rs`, `spec_type_test.rs`, `tui_link_editor_test.rs`.
**Verify:** `cargo build && cargo test` (whole workspace) must be green as ONE unit. `grep -rn "RelationType::Implements\|ALL_STRS\|INVERSE_STRS\|resolve_rel_keyword" src tests` returns nothing.

### Task 3 — `link`/`unlink` consume the registry; inverse from config
**ACs:** 1, 2, 4, 5.
**Files:** `src/cli/link.rs`, `src/main.rs` (ensure `link`/`unlink` are invoked with `link_with_config(..., Some(&config))`, not the `None` convenience wrapper).
**Do:**
- In `link_inner`/`unlink` replace `resolve_rel_keyword(rel_type)?` with `config.resolve_relationship(rel_type)?` (Task 1). This means `link`/`unlink` now REQUIRE a `Config` to resolve — make `config: &Config` non-optional on the inner path, or `bail!` if `None`. Unknown name ⇒ the resolver's `Err` (AC2); ensure nothing is written before the resolve check (it already resolves first, before `rewrite_frontmatter`, so AC2's "no frontmatter written" holds — keep that ordering).
- Use the resolver's `(canonical_name, flipped)`: `rel_str = canonical_name`; flip `(from, to)` when `flipped`. `LinkOutcome.rel_type` becomes `RelationType::new(&canonical_name)`.
- Check `src/main.rs` link/unlink dispatch passes the loaded `config`. Update the two `link.rs` test helpers (`gh_config_with_rfc_type`, `git_ref_config`) to add a `relationships` registry (or call a shared helper) so their `Config` resolves `implements`.
**Verify:** new `cli_link_test.rs` tests for AC1/AC2/AC4/AC5 (see Test Plan). `cargo test -p lazyspec --test main cli_link`.

### Task 4 — `validate` flags docs carrying undeclared relationship names
**AC:** 3.
**Files:** `src/engine/validation.rs`.
**Do:**
- Add `ValidationIssue::UnknownRelationship { path: PathBuf, name: String }` with a `Display` arm (e.g. `"unknown relationship \"{name}\": {path} (not declared in [[relationships]])"`).
- Add a `Checker` (e.g. `UnknownRelationshipRule`): for each non-`validate_ignore` doc, for each `rel` in `meta.related`, if `config.relationship_by_name(rel.rel_type.as_str()).is_none()` push `(Severity::Error, UnknownRelationship { ... })`.
- Register it in `default_checkers()`.
**Verify:** new `cli_validate_test.rs` test `validate_flags_undeclared_relationship_name` (AC3) asserting via `matches!`. Confirm existing validate tests still pass (they use `Config::default()` which now carries the standard registry, so `implements`/`related-to` are declared and won't be flagged). `cargo test -p lazyspec --test main validate`.

### Task 5 — strict hard error on missing `[[relationships]]` (verify wiring)
**AC:** 6.
**Files:** none new beyond Task 1 (the `bail!` lives in `Config::parse`). This task is the explicit confirmation step.
**Do:** confirm the `bail!` from Task 1 fires through `Config::load` when a `.lazyspec.toml` exists without the block, and that the message names `[[relationships]]` (ADR-012 says it should point at `fix`). Confirm `Config::default()` (no file) is unaffected.
**Verify:** AC6 tests from Task 1; one integration test spawning the binary in a temp dir whose `.lazyspec.toml` omits `[[relationships]]` and asserting a non-zero exit + `[[relationships]]` in stderr.

### Task 6 — `--json` serialization by configured name (verify + guard)
**AC:** 7.
**Files:** `src/cli/json.rs` (verify only — it already does `format!("{}", r.rel_type)`).
**Do:** confirm the newtype `Display` (Task 2) returns the configured string, so JSON emits the configured name with no code change. Add the guard test.
**Verify:** AC7 integration test `json_output_serializes_relationship_by_configured_name` spawning `show <doc> --json` in a temp project declaring a custom relationship name. `cargo test -p lazyspec --test main json`.

### README
Per CLAUDE.md: update `README.md` where it documents `link`'s relationship-type argument / config schema to describe the `[[relationships]]` block and that the vocabulary is now config-driven (the `link --help` text "canonical (implements, supersedes, blocks, related-to) or inverse alias (...)" is now derived from config, not hardcoded).

## Notes

**Verified file paths & call-site inventory (103 `RelationType` textual refs; the "~89 sites" of ADR-010):**
- `src/engine/document.rs` — enum, `FromStr`/`Display`, `resolve_rel_keyword`, `ResolvedRelKeyword`, `ALL`/`ALL_STRS`/`INVERSE_STRS`, `Relation`, `parse_relation` (33 refs).
- `src/engine/store.rs` (forward/reverse link maps, 5), `src/engine/store/links.rs` (`related_to`/`referenced_by`, link maps, 7), `src/engine/store_dispatch.rs` (test ctor, 2), `src/engine/context.rs` (chain/related traversal — hardcodes `Implements`/`RelatedTo`, 9), `src/engine/issue_body.rs` (tests, 6), `src/engine/config.rs` (new registry), `src/engine/validation.rs` (new check; existing checks already compare `rel.rel_type.to_string() == link`).
- `src/cli/link.rs` (`resolve_rel_keyword`, `LinkOutcome`, 2), `src/cli/completions.rs` (`ALL_STRS`/`INVERSE_STRS`, 3).
- `src/tui/state/forms.rs` (`REL_TYPES` const built from `ALL_STRS`+`INVERSE_STRS`, 7 — needs runtime registry), `src/tui/state/graph.rs` (`RelatedTo` filter, 3), `src/tui/state/app.rs` (`.parse()`, 1).
- Tests: `tests/integration/document_test.rs` (13), `cli_link_test.rs` (5), `spec_type_test.rs` (3), `tui_link_editor_test.rs` (4).

**Decisions:**
- `RelationType` mirrors `DocType` exactly: pure `FromStr`, validation deferred to `link`/`validate` (ADR-010). The ~89-site change is mechanical EXCEPT `forms.rs`'s `REL_TYPES` const (must become runtime, sourced from `Config`) and `completions.rs` (must read the loaded registry).
- `resolve_rel_keyword` moves onto `Config` as `resolve_relationship`, sourcing inverses from the `inverse` field. #55's store-once/flip-on-display mechanism is unchanged; only the source of inverse names moves.
- `Config::default()` retains the built-in vocabulary so `Config::default()`-based tests and `load`-without-a-file stay green; the strict `[[relationships]]`-required error is a `Config::parse`-path concern (a file present but block absent).
- `context.rs`/`graph.rs` keep a literal coupling to the names `"implements"`/`"related-to"` for chain/graph traversal — this matches today's coupling and is in scope only as the mechanical variant→string conversion. Making chain-link names configurable is out of scope (constraints stay in `[[rules]]`, STORY-127/128).
- AC7 is essentially free: `src/cli/json.rs` already serializes via `format!("{}", r.rel_type)`; once `Display` returns the configured string the configured name flows through. The test is a regression guard.
- Atomicity: Tasks 1+2 must compile/land together; the enum cannot exist half-converted.

