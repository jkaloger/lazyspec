---
title: "Spec-kit and OpenSpec Document Support"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-15
tags:
  - formats
  - interop
  - spec-kit
  - openspec
related:
  - related to: docs/rfcs/RFC-013-custom-document-types.md
  - related to: docs/rfcs/RFC-002-ai-driven-workflow.md
---

## Problem

lazyspec speaks one dialect: its own frontmatter format with `title`, `type`, `status`, `tags`, `related`. This works well for projects that start with lazyspec, but creates friction in two increasingly common scenarios:

1. Teams adopting spec-driven development (SDD) tooling like GitHub's spec-kit or Fission-AI's OpenSpec already have specifications in those formats. They can't use lazyspec to browse, validate, or manage those documents without converting everything.

2. Teams wanting to share specs across tools hit a format wall. A spec-kit `constitution.md` or OpenSpec `spec.md` can't participate in lazyspec's relationship graph, validation, or TUI views.

The custom type system from RFC-013 lets you define *new types*, but it still expects lazyspec's frontmatter schema. There's no way to map a foreign document format into lazyspec's data model.

## Context: The Formats

### spec-kit (GitHub)

A CLI and methodology for spec-driven development. Key documents:

- `constitution.md` -- non-negotiable project principles
- `proposal.md` -- why and what
- `spec.md` -- detailed requirements (formal language: SHALL, MUST)
- `tasks.md` -- implementation steps

Documents use YAML frontmatter but with a different schema. spec-kit focuses on a single-project workflow where specs are the source of truth and code is generated from them. Documents are organized by feature directory.

### OpenSpec (Fission-AI)

A lightweight SDD framework. Key concepts:

- Single unified spec document per system (`spec.md`)
- Delta specs for proposed changes (`delta-*.md`) marked as ADDED/MODIFIED/REMOVED
- GIVEN/WHEN/THEN scenarios embedded in requirements
- Requirements use normative language (SHALL, MUST)

OpenSpec consolidates state into one living document rather than distributing across many files.

## Intent

Make lazyspec's existing `[[types]]` config expressive enough to ingest foreign document formats without any format-specific code. Instead of hardcoded adapters that know about spec-kit or OpenSpec, the type system gains config primitives for detection, field mapping, default values, value translation, and relationship inference. Users configure how their documents map into lazyspec's model; lazyspec doesn't need to know what "spec-kit" is.

Documents stay in their original format on disk. This is a read layer that makes foreign documents visible in the TUI and CLI through configuration alone.

## Design

### Configurable Statuses

Before extending types, statuses need to become configurable. Today `Status` is a hardcoded enum of five variants with hardcoded colors and hardcoded validation semantics. Foreign documents use different terminology ("approved", "wip", "proposed", "applied"), and the current parser rejects anything it doesn't recognise.

Statuses become config-driven with a `[[statuses]]` array:

```toml
[[statuses]]
name = "draft"
color = "yellow"
phase = "open"

[[statuses]]
name = "review"
color = "blue"
phase = "open"

[[statuses]]
name = "accepted"
color = "green"
phase = "closed"

[[statuses]]
name = "rejected"
color = "red"
phase = "terminal"

[[statuses]]
name = "superseded"
color = "gray"
phase = "terminal"
```

When `[[statuses]]` is absent from config, the five defaults above apply (matching today's behavior exactly).

**`phase`** gives validation the semantic it needs without hardcoding status names:

| Phase | Meaning | Validation behavior |
|-------|---------|-------------------|
| `open` | Work in progress | Default. No special validation. |
| `closed` | Done, successful | Used for "all children closed" checks, "parent not closed" warnings. |
| `terminal` | Done, unsuccessful | Children with `closed` status under a `terminal` parent trigger warnings. |

This is the same logic that exists today, just parameterized. Validation checks `phase` instead of matching on `Status::Accepted` or `Status::Rejected` directly.

**`color`** accepts named colors: `red`, `green`, `blue`, `yellow`, `cyan`, `magenta`, `gray`, `white`. These map to ratatui's `Color` enum. The TUI's `status_color` function becomes a config lookup instead of a match statement.

**Unknown statuses** (present in a document but not in config) get `phase = "open"` and `color = "white"` by default. They load without error, just without semantic meaning. This is important for foreign documents that may use statuses you haven't mapped yet.

@ref src/engine/document.rs#Status

### Extended Type Config

The existing `[[types]]` definition from RFC-013 gains new optional fields. Native lazyspec types don't use them, so existing configs are unaffected.

```toml
# Native type (unchanged from RFC-013)
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

# Foreign type -- spec-kit proposal
[[types]]
name = "proposal"
plural = "proposals"
dir = "specs"
prefix = ""
icon = "💡"
glob = "**/proposal.md"          # file detection pattern
numbering = "none"               # don't auto-number these
readonly = true                  # lazyspec won't create/edit these

[types.field_map]
title = "title"                  # 1:1 mapping (explicit but same name)
status = "status"
tags = "labels"                  # their "labels" -> our "tags"

[types.value_map.status]         # translate foreign status values
"approved" = "accepted"
"wip" = "draft"
"in-review" = "draft"

[types.defaults]                 # fill missing fields
status = "accepted"
tags = []
author = "unknown"
```

@ref src/engine/config.rs#TypeDef

### Detection: `glob` and `match_field`

Today, lazyspec finds documents by scanning a directory for files with the configured `prefix`. This works for `RFC-001-*.md` but not for foreign documents that use different naming conventions.

Two new detection mechanisms:

**`glob`** -- A file pattern relative to `dir`. When present, lazyspec uses this instead of prefix matching to discover documents of this type.

```toml
glob = "**/proposal.md"          # any proposal.md in any subdirectory
glob = "delta-*.md"              # files starting with delta-
glob = "**/*.spec.md"            # any .spec.md file
```

**`match_field`** -- Detect type by a frontmatter field value instead of filename. Useful when multiple document types live in the same directory and are distinguished by content.

```toml
dir = "specs"
match_field = { field = "kind", value = "constitution" }
```

When both `glob` and `match_field` are present, both must match (AND logic). When neither is present, the existing prefix-based detection applies.

Detection priority: `match_field` > `glob` > `prefix`. If a file matches multiple types, the first matching type in config order wins.

### Field Mapping: `[types.field_map]`

Maps foreign frontmatter field names to lazyspec's expected fields. Each key is a lazyspec field, each value is the name of the corresponding field in the foreign document.

```toml
[types.field_map]
title = "name"           # their "name" field -> our "title"
status = "state"         # their "state" field -> our "status"
tags = "labels"          # their "labels" array -> our "tags"
author = "owner"         # their "owner" field -> our "author"
```

Supported lazyspec fields: `title`, `status`, `tags`, `author`, `date`, `related`.

When `field_map` is absent, lazyspec expects its native field names (current behavior). When a mapped field doesn't exist in the document's frontmatter, the defaults apply.

### Value Mapping: `[types.value_map.<field>]`

Translates foreign field values to lazyspec's expected values. Most useful for `status`, where different tools use different terminology.

```toml
[types.value_map.status]
"approved" = "accepted"
"wip" = "draft"
"pending" = "draft"
"closed" = "rejected"
```

Values not in the map pass through unchanged. This means if a foreign doc already uses `"accepted"`, no mapping is needed for that value.

### Defaults: `[types.defaults]`

Fill in fields that are missing entirely from the foreign document's frontmatter.

```toml
[types.defaults]
status = "accepted"
tags = []
author = "unknown"
```

Defaults apply after field mapping and value mapping. The precedence chain is: raw frontmatter -> field_map -> value_map -> defaults.

### Relationship Inference: `[types.infer_relations]`

Automatically generate relationships based on file proximity or naming patterns. This replaces the hardcoded "delta-specs relate to their spec.md" logic from the adapter approach.

```toml
[types.infer_relations]
sibling = "related-to"           # files in same directory are related
parent_glob = "spec.md"          # relate to spec.md in same directory
parent_rel = "implements"        # relationship type for parent link
```

**`sibling`** -- When set, documents of this type that share a parent directory get a relationship of the specified type to each other.

**`parent_glob` + `parent_rel`** -- When set, each document of this type gets a relationship of type `parent_rel` to the file matching `parent_glob` relative to the document's directory. If no match is found, no relationship is created (no error).

Both are optional. When absent, no relationships are inferred (the document only has relationships explicitly declared in its frontmatter, if any).

### Numbering: `"none"` Strategy

RFC-027 introduces sqids as an alternative to incremental numbering. Foreign documents need a third option: no numbering at all. Their filenames are determined by the source tool, not lazyspec.

```toml
numbering = "none"
```

When `numbering = "none"`, `lazyspec create` is disabled for this type (or creates a file with just the slug, no numeric/sqids prefix). The `readonly` flag makes this explicit.

### Read-only Types: `readonly`

```toml
readonly = true
```

When `true`, `lazyspec create` refuses to create documents of this type, and the TUI disables the `n` (create) and `d` (delete) keys when viewing them. The `s` (status) and `t` (tag) keys are also disabled since writing back to foreign frontmatter in a format-preserving way is non-trivial.

This is the safe default for foreign documents. A future RFC could add write-back support for specific formats.

### Worked Example: spec-kit

A team using GitHub spec-kit alongside lazyspec. Note the custom statuses that accommodate spec-kit's terminology natively, avoiding value_map entirely:

```toml
# Statuses: keep lazyspec defaults + add spec-kit's "approved" and "wip"
[[statuses]]
name = "draft"
color = "yellow"
phase = "open"

[[statuses]]
name = "review"
color = "blue"
phase = "open"

[[statuses]]
name = "accepted"
color = "green"
phase = "closed"

[[statuses]]
name = "rejected"
color = "red"
phase = "terminal"

[[statuses]]
name = "superseded"
color = "gray"
phase = "terminal"

[[statuses]]
name = "approved"
color = "green"
phase = "closed"

[[statuses]]
name = "wip"
color = "yellow"
phase = "open"

# Native lazyspec types
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

# spec-kit types (read-only)
[[types]]
name = "constitution"
plural = "constitutions"
dir = "specs"
prefix = ""
glob = "constitution.md"
icon = "⚖"
numbering = "none"
readonly = true

[types.defaults]
status = "accepted"

[[types]]
name = "proposal"
plural = "proposals"
dir = "specs"
prefix = ""
glob = "**/proposal.md"
icon = "💡"
numbering = "none"
readonly = true

[types.defaults]
status = "draft"

[[types]]
name = "spec"
plural = "specs"
dir = "specs"
prefix = ""
glob = "**/spec.md"
icon = "📋"
numbering = "none"
readonly = true

[types.infer_relations]
sibling = "related-to"

[types.defaults]
status = "accepted"
```

With `"approved"` and `"wip"` defined as first-class statuses (with appropriate phases and colors), spec-kit documents load naturally. No value mapping needed because validation understands the phase, and the TUI knows the color.

### Worked Example: OpenSpec

```toml
# Statuses: defaults + OpenSpec's terminology
[[statuses]]
name = "draft"
color = "yellow"
phase = "open"

[[statuses]]
name = "accepted"
color = "green"
phase = "closed"

[[statuses]]
name = "proposed"
color = "cyan"
phase = "open"

[[statuses]]
name = "applied"
color = "green"
phase = "closed"

# Types
[[types]]
name = "openspec"
plural = "openspecs"
dir = "."
prefix = ""
glob = "spec.md"
icon = "📄"
numbering = "none"
readonly = true

[types.defaults]
status = "accepted"

[[types]]
name = "delta"
plural = "deltas"
dir = "."
prefix = ""
glob = "delta-*.md"
icon = "Δ"
numbering = "none"
readonly = true

[types.infer_relations]
parent_glob = "spec.md"
parent_rel = "modifies"

[types.field_map]
status = "state"

[types.defaults]
status = "proposed"
```

Here `field_map` is still useful (OpenSpec uses `state` not `status`), but `value_map` is unnecessary because `"proposed"` and `"applied"` are defined directly as statuses.

### Preset Config Files

The repo ships ready-to-use config files for common SDD workflows:

```
presets/
  spec-kit.toml                  # full config for spec-kit interop
  openspec.toml                  # full config for openspec interop
```

Each preset is a complete `.lazyspec.toml` with statuses, types, and field mappings pre-configured. Users copy the preset into their project root and adjust as needed:

```bash
cp presets/spec-kit.toml .lazyspec.toml
```

The presets are the worked examples above, packaged as files. They include comments explaining each section so users can modify them without reading the full RFC.

### Example Documents

The repo also ships example documents demonstrating each preset in action:

```
examples/
  spec-kit-interop/
    .lazyspec.toml               # copied from presets/spec-kit.toml
    docs/rfcs/                   # native lazyspec docs
      RFC-001-auth-design.md
    specs/                       # spec-kit docs
      constitution.md
      features/
        user-auth/
          proposal.md
          spec.md
          tasks.md
  openspec-interop/
    .lazyspec.toml               # copied from presets/openspec.toml
    spec.md
    delta-add-caching.md
```

Running `lazyspec status` in either example directory shows foreign documents integrated into the standard views.

### Loading Pipeline Changes

The store's document loading pipeline changes minimally:

1. For each configured type, discover files using `glob` (new) or `prefix` (existing)
2. Parse frontmatter
3. Apply `field_map` to remap field names
4. Apply `value_map` to translate field values
5. Apply `defaults` for missing fields
6. Build `Document` struct
7. Apply `infer_relations` to generate additional relationships

Steps 3-6 are new. They slot into the existing `load_documents` path as a post-processing step on the parsed frontmatter, before the `Document` is constructed.

@ref src/engine/store.rs#Store

### What This Doesn't Cover

- Write-back to foreign formats (creates, edits, status changes)
- Automatic format detection without config (you must configure types)
- Translating between formats (spec-kit -> openspec or vice versa)
- Frontmatter schemas beyond the fields lazyspec already understands
- Custom relation types (still hardcoded: implements, supersedes, blocks, related-to)

## Stories

1. **Configurable statuses** -- Replace the `Status` enum with a config-driven `[[statuses]]` array. Phase-based validation (`open`/`closed`/`terminal`). Config-driven color mapping in the TUI. Sensible defaults matching today's five statuses. Unknown statuses load gracefully.

2. **Detection primitives: `glob` and `match_field`** -- Extend type discovery to support glob patterns and frontmatter field matching alongside prefix-based detection. Detection priority ordering. `numbering = "none"` and `readonly` flag.

3. **Field mapping, value mapping, and defaults** -- `[types.field_map]`, `[types.value_map]`, `[types.defaults]` config parsing and application in the document loading pipeline. Post-processing step between frontmatter parse and Document construction.

4. **Relationship inference** -- `[types.infer_relations]` with `sibling` and `parent_glob`/`parent_rel` support. Directory-relative relationship generation during store load.

5. **Preset configs and example documents** -- `presets/spec-kit.toml` and `presets/openspec.toml` with worked configs. Example directories for both formats with sample documents. Integration tests verifying the full pipeline.
