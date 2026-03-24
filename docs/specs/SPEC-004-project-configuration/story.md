---
title: "Project Configuration"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: default-config-when-file-absent

Given a project root with no `.lazyspec.toml` file
When `Config::load` is called
Then the returned `Config` uses all defaults: five built-in types (rfc, story, iteration, adr, spec), three default validation rules, naming pattern `{type}-{n:03}-{title}.md`, template dir `.lazyspec/templates`, and `ref_count_ceiling` of 15

### AC: custom-types-replace-defaults

Given a `.lazyspec.toml` containing a `[[types]]` array with a single custom type
When `Config::parse` processes the file
Then only the custom type is present in `documents.types` and none of the five defaults appear

### AC: legacy-directories-synthesize-types

Given a `.lazyspec.toml` with a `[directories]` section but no `[[types]]` array
When `Config::parse` processes the file
Then four types (rfc, story, iteration, adr) are synthesized from the directory paths, and no spec type is produced

### AC: sqids-requires-salt

Given a `.lazyspec.toml` where a type has `numbering = "sqids"` but no `[numbering.sqids]` section exists
When `Config::parse` processes the file
Then parsing fails with an error message requiring a `[numbering.sqids]` section with a non-empty salt

### AC: sqids-min-length-bounds

Given a `.lazyspec.toml` with `[numbering.sqids]` where `min_length` is 0 or 11
When `Config::parse` processes the file
Then parsing fails with an error stating `min_length` must be between 1 and 10

### AC: reserved-requires-section

Given a `.lazyspec.toml` where a type has `numbering = "reserved"` but no `[numbering.reserved]` section exists
When `Config::parse` processes the file
Then parsing fails with an error requiring a `[numbering.reserved]` section

### AC: reserved-defaults-applied

Given a `.lazyspec.toml` with a `[numbering.reserved]` section that omits `remote` and `max_retries`
When `Config::parse` processes the file
Then `remote` defaults to `"origin"` and `max_retries` defaults to 5

### AC: reserved-sqids-format-requires-sqids-config

Given a `.lazyspec.toml` with `[numbering.reserved]` where `format = "sqids"` but no `[numbering.sqids]` section
When `Config::parse` processes the file
Then parsing fails with an error indicating the sqids section with a non-empty salt is required

### AC: reserved-empty-remote-rejected

Given a `.lazyspec.toml` with `[numbering.reserved]` where `remote = ""`
When `Config::parse` processes the file
Then parsing fails with an error stating the remote must not be empty

### AC: custom-rules-replace-defaults

Given a `.lazyspec.toml` with a single `[[rules]]` entry
When `Config::parse` processes the file
Then only the user-provided rule is present and none of the three default rules appear

### AC: validation-rule-shapes-parsed

Given a `.lazyspec.toml` with one `parent-child` rule and one `relation-existence` rule
When `Config::parse` processes the file
Then both rules are deserialized with the correct shape, name, severity, and type-specific fields

### AC: naming-pattern-default

Given a `.lazyspec.toml` that omits the `[naming]` section
When `Config::parse` processes the file
Then the naming pattern is `{type}-{n:03}-{title}.md`

### AC: type-lookup-by-name

Given a `Config` with types including one named "rfc"
When `type_by_name("rfc")` is called
Then it returns the matching `TypeDef`, and `type_by_name("nonexistent")` returns `None`
