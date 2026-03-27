---
title: Singleton and Parent Type Constraints
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/rfcs/RFC-034-convention-and-dictum-document-types.md
---



## Context

RFC-034 introduces convention (singleton project manifesto) and dictum (tagged principles as children of convention). These types require two new structural constraints on `TypeDef`: a singleton flag that limits a type to at most one document, and a parent_type field that confines a type's documents to the parent type's folder. This story covers the engine-level plumbing for both constraints across config deserialization, the create command guard, and the validation pipeline.

## Acceptance Criteria

### AC: singleton-config-deserialization

- **Given** a `.lazyspec.toml` with a type entry containing `singleton = true`
  **When** the config is loaded
  **Then** the `TypeDef` for that type has `singleton` set to `true`

- **Given** a `.lazyspec.toml` with a type entry that omits `singleton`
  **When** the config is loaded
  **Then** the `TypeDef` for that type has `singleton` defaulting to `false`

### AC: parent-type-config-deserialization

- **Given** a `.lazyspec.toml` with a type entry containing `parent_type = "convention"`
  **When** the config is loaded
  **Then** the `TypeDef` for that type has `parent_type` set to `Some("convention")`

- **Given** a `.lazyspec.toml` with a type entry that omits `parent_type`
  **When** the config is loaded
  **Then** the `TypeDef` for that type has `parent_type` set to `None`

### AC: singleton-create-guard

- **Given** a type with `singleton = true` and no existing documents of that type
  **When** I run `lazyspec create <type> "Title"`
  **Then** the document is created successfully

- **Given** a type with `singleton = true` and one existing document of that type
  **When** I run `lazyspec create <type> "Another"`
  **Then** the command fails with an error indicating the singleton already exists, including the path of the existing document

### AC: singleton-validation-error

- **Given** a type with `singleton = true`
  **And** more than one document of that type exists in the store (e.g. manually added)
  **When** I run `lazyspec validate --json`
  **Then** a validation error is reported indicating the singleton constraint is violated

### AC: parent-type-validation-location

- **Given** a type with `parent_type = "convention"` where convention's dir is `docs/convention`
  **And** a document of that type exists inside `docs/convention/`
  **When** I run `lazyspec validate --json`
  **Then** no parent_type validation error is reported for that document

- **Given** a type with `parent_type = "convention"`
  **And** a document of that type exists outside `docs/convention/`
  **When** I run `lazyspec validate --json`
  **Then** a validation error is reported indicating the document must reside within the parent type's directory

### AC: parent-type-requires-singleton-parent

- **Given** a type with `parent_type` pointing to another type that is not a singleton
  **When** I run `lazyspec validate --json`
  **Then** a validation error is reported indicating the parent type must be a singleton

## Scope

### In Scope

- `singleton: bool` field on `TypeDef` (default `false`), with serde deserialization
- `parent_type: Option<String>` field on `TypeDef` (default `None`), with serde deserialization
- Pre-creation guard in the create command that refuses to create a second document of a singleton type
- Validation error variant for singleton violation (more than one document of a singleton type)
- Validation error variant for parent_type location violation (document outside parent's directory)
- Validation error when parent_type references a non-singleton type
- Unit and integration tests for the above

### Out of Scope

- `lazyspec convention` CLI subcommand (Story 2)
- Default convention/dictum content scaffolding during `lazyspec init` (Story 3)
- Skill preflight integration and boot hook configuration (Story 4)
- Any changes to relationship types or the link graph
