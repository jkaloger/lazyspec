---
title: Attribute write path and github attribute round-trip
type: story
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: RFC-050
---## Context

The github-issues store is a dumb mirror today, and two gaps block any native-field work on top of it. First, there is no CLI to write an attribute value: `update` only exposes `--status`, `--title`, and `--body`, so declared `AttrDef`s (priority, owner, estimate, and the like) can be read via `show`/`status --json` (STORY-152) but never authored from the tool. Second, the github-issues store drops attributes entirely — `issue_body.rs` `serialize()` omits `doc.attributes` and `deserialize()` sets attributes empty — so any attribute that does exist on a github-backed doc is lost the moment it round-trips through GitHub.

This slice is the foundational vertical that closes both gaps: a plain `--attr key=value` write path coerced and validated against the document's declared `AttrDef`s, plus attribute round-trip through the github store's issue-body HTML comment. It unblocks the native issue-type attribute (STORY-157), project membership (STORY-161), and per-board field attributes (STORY-162), all of which need both a write entry point and a github store that preserves attribute values. It deliberately stays on the statically declared `AttrDef` path only — no GitHub-snapshot schema validation and no namespaced `PROJECT-n.field` attributes.

## Acceptance Criteria

- **Given** a filesystem-backed document with a declared string `AttrDef` `owner`
  **When** `lazyspec update <id> --attr owner=jkaloger` is run
  **Then** the value is written to the document's frontmatter and `show --json` reports `owner` as `jkaloger`.

- **Given** a document with an enum `AttrDef` `priority` whose options are `low|med|high`, and an int `AttrDef` `estimate`
  **When** `update --attr priority=urgent` (not an allowed option) or `update --attr estimate=notanumber` (wrong kind) is run
  **Then** the command fails with a non-zero exit and a validation error naming the offending key, and the document is left unchanged.

- **Given** a github-issues-backed document
  **When** an attribute is written via `update --attr key=value` and the document is subsequently read back from the store
  **Then** the attribute survives the write/read cycle, having been serialized into and parsed back out of the issue-body HTML comment.

- **Given** a document with two declared `AttrDef`s `owner` and `estimate`
  **When** `update --attr owner=jkaloger --attr estimate=3` is run (the flag supplied more than once)
  **Then** both attributes are applied in a single command invocation, `owner` stored as a string and `estimate` as an int (each coerced to its own declared kind, not uniformly to string).

- **Given** any document after an `--attr` write succeeds
  **When** `show --json` is run for that document
  **Then** the emitted `attributes` map reflects the newly written, type-coerced value (e.g. an int kind appears as a JSON number, not a string).

## Scope

### In Scope

- A repeatable `--attr <key>=<value>` flag on `lazyspec update`, parsed into key/value pairs and threaded to the store via the existing `DocumentStore` `&[(&str, &str)]` updates contract.
- Coercion of each supplied value to its declared `AttrDef` kind (int/float/string/bool/date/enum) and validation via the existing `AttributeSchemaChecker` — kind mismatch, unknown enum option, and required-attribute rules all enforced before any write.
- Filesystem store: writing the coerced `AttrValue` into `DocMeta.attributes` and persisting it to frontmatter.
- github-issues store round-trip: `issue_body.rs` `serialize()` emits `doc.attributes` into the HTML comment and `deserialize()` parses them back, so attributes are no longer dropped on github-backed docs.
- `--json` output and exit codes consistent with existing `update` behaviour (success object on write, error on validation failure).

### Out of Scope

- Dynamic / native schema validation of attribute values against the cached GitHub snapshot (STORY-155).
- Namespaced `PROJECT-n.<field>` per-board field attributes and the native issue-type attribute (STORY-162, STORY-157).
- Any new GraphQL or native GitHub mutation surface; this slice touches only the existing issue-body HTML comment and frontmatter serialization.
- Attribute deletion / clearing semantics beyond setting a value.
