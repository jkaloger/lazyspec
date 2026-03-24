---
title: SOLID Refactors - Validation and Config
type: iteration
status: draft
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-084-solid-refactors.md
---


## Context

Implements streams 6a, 6b, and 6c from STORY-084. These three changes share a common theme: replacing scattered, hard-to-extend patterns with well-defined abstractions that allow new rules, types, and config fields to be added without touching existing code. All three changes are self-contained and do not require the filesystem abstraction (6d) to be in place first.

## Changes

### 6a: ValidationRule Trait

- [ ] Define `ValidationRule` trait in `engine/validate.rs` with `check(&self, store: &Store, config: &Config) -> Vec<ValidationIssue>` and `severity(&self) -> Severity`
- [ ] Extract each current validation concern into a named struct implementing the trait:
  - `BrokenLinkRule` (broken link checks)
  - `ParentLinkRule` (parent link consistency)
  - `StatusConsistencyRule` (status propagation)
  - `DuplicateIdRule` (duplicate ID detection)
- [ ] Replace the monolithic `validate_full()` body with a `Vec<Box<dyn ValidationRule>>` that collects and partitions results into errors and warnings
- [ ] Confirm no existing rule behaviour changes: same issues produced for same inputs

### 6b: RelationType FromStr/Display

- [ ] Implement `std::fmt::Display` for `RelationType`, mapping each variant to its canonical string (`"implements"`, `"supersedes"`, `"blocks"`, `"related-to"`)
- [ ] Implement `std::str::FromStr` for `RelationType` returning `Err` for unknown strings
- [ ] Remove the ad-hoc string match blocks from `link.rs` that duplicate this mapping
- [ ] Add a round-trip property: `s.parse::<RelationType>()?.to_string() == s` for all four canonical strings
- [ ] Confirm `lazyspec link` and relation display continue to work correctly

### 6c: Config Decomposition

- [ ] Introduce sub-structs in `engine/config.rs`:
  - `DocumentConfig` -- types, naming, numbering strategy
  - `FilesystemConfig` -- directories, templates
  - `UiConfig` -- tui settings
  - `RulesConfig` -- validation rule configuration
- [ ] Update `Config` to hold `documents: DocumentConfig`, `filesystem: FilesystemConfig`, `ui: UiConfig`, `rules: RulesConfig`
- [ ] Update `Config::load` to construct the sub-structs; keep it as the single construction point
- [ ] Update all `config.field` access sites to use the appropriate sub-struct path (`config.documents.field`, `config.filesystem.field`, etc.)
- [ ] Confirm `lazyspec show`, `lazyspec create`, `lazyspec fix`, and `lazyspec validate` all behave identically after the rename

## Test Plan

- [ ] Run `cargo test` before and after each stream; no regressions
- [ ] For 6a: write a unit test that constructs a minimal `Store` triggering each rule and asserts the expected `ValidationIssue` variants are returned
- [ ] For 6b: write a unit test covering `FromStr`/`Display` round-trips for all four `RelationType` variants and an unknown-string error case
- [ ] For 6c: no new tests required; existing integration tests cover config loading; verify `cargo test` passes after access-site updates
- [ ] Smoke test: `cargo run -- validate` on the docs directory produces the same output before and after

## Notes

The `ValidationRule` trait adds indirection. The tradeoff is acceptable here because the existing monolithic `validate_full()` is already 200+ lines and adding rules requires touching multiple match arms. If the rule count stays low, the trait pays for itself in readability alone.

Config decomposition has a wide but mechanical blast radius. All changes are find-and-replace style access-site updates; no logic changes. Run `cargo check` after each sub-struct migration to catch missed sites before attempting a full build.
