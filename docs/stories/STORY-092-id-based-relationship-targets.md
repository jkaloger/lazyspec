---
title: ID-based relationship targets
type: story
status: accepted
author: agent
date: 2026-03-26
tags: []
related:
- implements: SPEC-001
---




## Context

Relationships in YAML frontmatter currently store file paths as targets (e.g. `- implements: docs/rfcs/RFC-001-my-first-rfc.md`). This is fragile — renaming or moving a file breaks all inbound links. Document IDs (e.g. `RFC-001`) are stable, shorter, and already used everywhere else in the system (CLI arguments, TUI display, search). The relationship target format should match.

## Acceptance Criteria

### AC: id-based-storage

- **Given** a document with a relationship in its frontmatter
  **When** I inspect the YAML
  **Then** the target is a document ID (e.g. `RFC-001`), not a file path

### AC: link-writes-id

- **Given** I run `lazyspec link STORY-092 implements SPEC-001`
  **When** the command writes to the source document's frontmatter
  **Then** the `related` entry contains the target's document ID, not its file path

### AC: id-resolution-at-load

- **Given** a document's frontmatter contains `- implements: RFC-001`
  **When** the store loads and builds the link graph
  **Then** the relationship resolves to the correct document via ID lookup
  **And** forward and reverse links work as before

### AC: broken-link-validation

- **Given** a document's frontmatter contains `- implements: RFC-999` (nonexistent ID)
  **When** I run `lazyspec validate --json`
  **Then** a broken link error is reported referencing the unresolved ID

### AC: json-output-uses-ids

- **Given** a document has relationships
  **When** I run `lazyspec status --json` or `lazyspec show <id> --json`
  **Then** the `related` entries show document IDs as targets, not file paths

### AC: migration

- **Given** existing documents with path-based relationship targets
  **When** I run `lazyspec fix` (or equivalent migration command)
  **Then** all relationship targets are rewritten from paths to document IDs
  **And** no relationships are lost or broken

## Scope

### In Scope

- Change `Relation.target` semantics from path to document ID
- Update `parse_relation` to expect IDs
- Update `build_links` to resolve IDs to paths at load time
- Update `link` command to write IDs instead of paths
- Update `BrokenLinkRule` to validate IDs
- Update JSON serialization to output IDs
- Migration of all existing documents from paths to IDs
- Update tests

### Out of Scope

- New relationship types (covered by STORY-089)
- Changes to `RelationType` enum
- Backward compatibility period accepting both formats
