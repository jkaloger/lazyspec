---
title: "Custom document types"
type: rfc
status: draft
author: "@jkaloger"
date: 2026-03-06
tags:
- config
- types
- validation
---

## Summary

Replace the hardcoded `DocType` enum and validation rules with a configuration-driven type system. Users can replace the default document types wholesale with their own set via `.lazyspec.toml`. When no custom types are configured, the current defaults (RFC, ADR, Story, Iteration) apply unchanged.

## Problem

Document types are deeply embedded as a Rust enum (`DocType` in `engine/document.rs`) with match arms spread across at least six files: `document.rs`, `config.rs`, `validation.rs`, `create.rs`, `init.rs`, and `store.rs`. Adding a single new type requires modifying all of them.

This makes lazyspec unusable for teams whose workflow doesn't map to the RFC/Story/Iteration hierarchy. A game studio might want `pitch -> design-doc -> task`. A platform team might want `proposal -> epic -> ticket -> spike`. Today, none of that is possible without forking.

The `Directories` struct in `config.rs` has one named field per type, which means the config schema itself can't accommodate new types without a code change.

## Design Intent

### Type definitions in config

Replace the per-type fields in `[directories]` with a `[[types]]` array. Each entry defines a document type:

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
icon = "▲"

[[types]]
name = "iteration"
plural = "iterations"
dir = "docs/iterations"
prefix = "ITERATION"
icon = "◆"

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"
icon = "■"
```

Each type has four required fields and one optional field:

| Field    | Required | Purpose                                              |
|----------|----------|------------------------------------------------------|
| `name`   | yes      | Lowercase identifier used in frontmatter `type:` field |
| `plural` | yes      | Used in CLI output and grouping                      |
| `dir`    | yes      | Directory path relative to project root              |
| `prefix` | yes      | Uppercase prefix for filenames (e.g. `RFC-001-...`)  |
| `icon`   | no       | Single character used in TUI graph mode (e.g. `"●"`) |

When `icon` is omitted, the TUI assigns one from a default glyph set (`●`, `■`, `▲`, `◆`, `★`, `◎`) based on the type's position in the config list.

> [!NOTE]
> When `[[types]]` is absent from config, the defaults match today's behavior exactly. Existing projects don't need to change anything.

### Validation rules in config

Replace the hardcoded validation logic with a `[[rules]]` array:

```toml
[[rules]]
name = "iterations-need-stories"
child = "iteration"
parent = "story"
link = "implements"
severity = "error"

[[rules]]
name = "adrs-need-relations"
type = "adr"
require = "any-relation"
severity = "error"
```

Two rule shapes:

**Parent-child rule** (has `child`, `parent`, `link`): Every document of type `child` must have a relationship of type `link` to a document of type `parent`. This covers the current "iterations must implement stories" and could express "stories must implement rfcs" if a team wants that.

**Relation-existence rule** (has `type`, `require`): Every document of type `type` must have at least one relation. This covers the current "ADRs must have relations" rule.

Status-based validation (rejected parent, superseded parent, orphaned acceptance, all-children-accepted) remains hardcoded. These rules operate on universal relationship semantics, not on specific type names, so they work with any type configuration. The parent-child hierarchy for status checks is inferred from the configured parent-child rules.

> [!WARNING]
> Default rules must match today's validation behavior. If a user provides `[[rules]]`, those replace the defaults entirely. No merging.

### Internal representation

Replace the `DocType` enum with a string newtype:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocType(pub String);
```

The `Config` struct changes from named directory fields to a vector of type definitions:

```rust
pub struct TypeDef {
    pub name: String,
    pub plural: String,
    pub dir: String,
    pub prefix: String,
    pub icon: Option<String>,
}

pub struct Config {
    pub types: Vec<TypeDef>,
    pub rules: Vec<ValidationRule>,
    pub templates: Templates,
    pub naming: Naming,
}
```

`Config::default()` returns the current four types and two rules.

### What changes per file

| File | Current | After |
|------|---------|-------|
| `document.rs` | `DocType` enum with 4 variants | `DocType(String)` newtype |
| `config.rs` | `Directories` struct with named fields | `Vec<TypeDef>` + `Vec<ValidationRule>` |
| `validation.rs` | Hardcoded match on `DocType::Iteration`, etc. | Iterates `config.rules` to check constraints |
| `create.rs` | Match `DocType` to directory field | Lookup `TypeDef` by name |
| `store.rs` | Iterates 4 hardcoded directory fields | Iterates `config.types` |
| `init.rs` | Creates 4 hardcoded directories | Iterates `config.types` |
| `template.rs` | Uses `DocType::Display` for prefix | Uses `TypeDef.prefix` |
| `tui/app.rs` | Hardcoded `doc_types` vec of 4 variants | Populated from `config.types` |
| `tui/ui.rs` | Hardcoded graph icon match per variant | Uses `TypeDef.icon` with fallback glyphs |

### Migration

No migration needed. The `[directories]` config section becomes deprecated but can be supported for one version by converting it internally to `[[types]]` entries. Or we can just break it, since lazyspec is pre-1.0.

### CLI changes

`lazyspec create <type>` already accepts a string argument. The `FromStr` impl on `DocType` changes from a hardcoded match to a lookup against `config.types`. Invalid types produce the same error with a hint listing available types.

`lazyspec init` creates directories for all configured types instead of the hardcoded four.

`lazyspec validate` evaluates configured rules instead of hardcoded ones.

No new commands needed.

## What this doesn't cover

- Custom statuses (Draft, Review, Accepted, etc. stay hardcoded)
- Custom relation types (Implements, Supersedes, etc. stay hardcoded)
- Per-type template schemas or required frontmatter fields
- Workflow enforcement (e.g. "must go through Review before Accepted")

These could be future work but are out of scope here.

## Stories

1. **Config-driven type definitions** -- Parse `[[types]]` from `.lazyspec.toml`, fall back to defaults. Replace `Directories` struct and `DocType` enum.
2. **Config-driven validation rules** -- Parse `[[rules]]` from `.lazyspec.toml`, fall back to defaults. Replace hardcoded type checks in `validation.rs`.
3. **Propagate types through CLI** -- Update `create`, `init`, `store`, and `template` to use `Config::types` instead of matching on `DocType` variants.
