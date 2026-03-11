---
title: Config-driven validation rules
type: iteration
status: accepted
author: agent
date: 2026-03-07
tags: []
related:
- implements: docs/stories/STORY-038-config-driven-validation-rules.md
---



## Changes

### Task 1: Add ValidationRule and Severity to config

**ACs addressed:** AC-1 (defaults when no `[[rules]]`), AC-4 (defaults replaced not merged), AC-6 (invalid severity error)

**Files:**
- Modify: `src/engine/config.rs`
- Modify: `tests/config_test.rs`

**What to implement:**

Add a `Severity` enum with `Error` and `Warning` variants (derive `Deserialize`, `Serialize`, `Clone`, `Debug`, `PartialEq`). Use `#[serde(rename_all = "lowercase")]`.

Add a `ValidationRule` enum with two variants, using `#[serde(tag = "shape")]`:

@ref src/engine/config.rs#ValidationRule@17c1f1ae4aaa0f4a54dcd276cdf3178894ca1cad

Add `rules: Option<Vec<ValidationRule>>` to `RawConfig`. Add `pub rules: Vec<ValidationRule>` to `Config` (with `#[serde(skip)]` like `types`).

Add a `default_rules()` function returning two rules matching current behavior:
- ParentChild: name "iterations-need-stories", child "iteration", parent "story", link "implements", severity Error
- RelationExistence: name "adrs-need-relations", doc_type "adr", require "any-relation", severity Error

Update `Config::default()` to set `rules: default_rules()`. Update `Config::parse()` to use `raw.rules.unwrap_or_else(default_rules)` -- no merging, full replacement when present.

**How to verify:**
```
cargo test config_test
```

### Task 2: Replace hardcoded type checks with rule-driven validation

**ACs addressed:** AC-2 (parent-child rule fires), AC-3 (relation-existence rule fires), AC-5 (status-based validation infers hierarchy)

**Files:**
- Modify: `src/engine/validation.rs`

**What to implement:**

Replace the hardcoded "unlinked iteration" check (currently lines ~107-121) and "unlinked ADR" check (currently lines ~123-127) with a loop over `config.rules`:

For each `ParentChild` rule: find all documents whose `doc_type` matches `rule.child`. For each, check it has a relation of the type matching `rule.link` pointing to a document of type `rule.parent`. If missing, emit `UnlinkedIteration`-style issue at the configured severity. Rename `UnlinkedIteration` to something generic or add a new variant -- recommend adding `MissingParentLink { path, rule_name, child_type, parent_type }` and `MissingRelation { path, rule_name, doc_type }` variants to `ValidationIssue`.

For each `RelationExistence` rule: find all documents whose `doc_type` matches `rule.doc_type`. For each, check it has at least one relation. If none, emit issue at configured severity.

For the parent-child hierarchy checks (AllChildrenAccepted, UpwardOrphanedAcceptance, OrphanedAcceptance): instead of hardcoding RFC→STORY→ITERATION, build the hierarchy from the configured `ParentChild` rules. Collect all `(parent_type, child_type)` pairs from the rules and use those for the status-based checks. This means the status checks automatically adapt to custom hierarchies.

The `validate_full` function needs access to `Config`. If it doesn't already take one, add `config: &Config` as a parameter and thread it from the caller.

**How to verify:**
```
cargo test
```

### Task 3: Thread Config into validate and update callers

**ACs addressed:** AC-2, AC-3 (rules actually evaluated end-to-end)

**Files:**
- Modify: `src/engine/validation.rs` (add `config: &Config` param to `validate_full` if not already present)
- Modify: `src/cli/validate.rs` or wherever `validate_full` is called (pass config)
- Modify: `src/tui/app.rs` (if it calls validation, pass config)

**What to implement:**

Check current callers of `validate_full`. Add `config: &Config` parameter. Each caller already has access to config, so thread it through. This is mechanical.

If `validate_full` already takes config, this task merges into Task 2 and can be skipped.

**How to verify:**
```
cargo test
```

### Task 4: Add tests for config-driven rules

**ACs addressed:** All ACs

**Files:**
- Modify: `tests/config_test.rs`
- Modify: `tests/cli_validate_test.rs` or `tests/cli_expanded_validate_test.rs`

**What to implement:**

Config parsing tests:
- Default config produces 2 rules matching current behavior
- `[[rules]]` TOML with a parent-child rule parses correctly
- `[[rules]]` TOML with a relation-existence rule parses correctly
- Custom `[[rules]]` fully replaces defaults (provide one rule, assert only one rule present)
- Invalid severity value (e.g. `severity = "fatal"`) returns parse error

Validation behavior tests:
- Custom parent-child rule (e.g. child="story", parent="rfc", link="implements") fires when story has no RFC link
- Custom relation-existence rule fires for configured type with no relations
- Default rules still produce same errors as before (iterations without stories, ADRs without relations)
- Status-based checks (rejected parent, all-children-accepted) work with custom hierarchy inferred from rules

**How to verify:**
```
cargo test
```

## Test Plan

**Config parsing (Task 1, Task 4):**
- Test `default_rules()` returns 2 rules: iterations-need-stories (ParentChild) and adrs-need-relations (RelationExistence)
- Test `[[rules]]` TOML with `shape = "parent-child"` deserializes correctly
- Test `[[rules]]` TOML with `shape = "relation-existence"` deserializes correctly
- Test providing `[[rules]]` replaces defaults entirely (not merged)
- Test invalid severity value returns serde error
- Test no `[[rules]]` section falls back to defaults

**Validation behavior (Task 2, Task 4):**
- Test iteration without story link still produces error with default rules (regression)
- Test ADR without relations still produces error with default rules (regression)
- Test custom parent-child rule fires for matching documents
- Test custom relation-existence rule fires for matching documents
- Test status-based checks (RejectedParent, AllChildrenAccepted) infer hierarchy from configured rules
- Test custom rules with `severity = "warning"` produce warnings not errors

All tests are unit/integration level, deterministic, and fast. The status-based hierarchy inference test is the most complex (trades Writable for Predictive) but is necessary to verify AC-5.

## Notes

The `ValidationIssue` enum currently has type-specific variants (`UnlinkedIteration`, `UnlinkedAdr`). These get replaced with generic variants (`MissingParentLink`, `MissingRelation`) that carry the rule name and type info. This changes the display messages but not the semantics. Existing tests will need assertion updates for the new variant names and message format.

The `Directories` struct is still present on `Config` for backwards compatibility (ITERATION-028 kept it). This iteration doesn't touch it.
