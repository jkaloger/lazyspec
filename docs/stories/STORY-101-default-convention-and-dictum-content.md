---
title: Default Convention and Dictum Content
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/rfcs/RFC-034-convention-and-dictum-document-types.md
---



## Context

RFC-034 introduces convention and dictum document types. For these types to be useful out of the box, `lazyspec init` needs to register them in the default config and scaffold starter content. This story covers the default type definitions and the skeleton files that `init` creates.

## Acceptance Criteria

- Given the default type configuration,
  When a user inspects the built-in types,
  Then a `convention` type entry exists with `name = "convention"`, `plural = "convention"`, `dir = "docs/convention"`, `prefix = "CONVENTION"`, `icon = "📜"`, and `singleton = true`

- Given the default type configuration,
  When a user inspects the built-in types,
  Then a `dictum` type entry exists with `name = "dictum"`, `plural = "dicta"`, `dir = "docs/convention"`, `prefix = "DICTUM"`, `icon = "⚖"`, and `parent_type = "convention"`

- Given a fresh project with convention type configured,
  When a user runs `lazyspec init`,
  Then `docs/convention/index.md` is created with valid convention frontmatter (type: convention, status: draft) and a preamble explaining the document's purpose

- Given a fresh project with convention type configured,
  When a user runs `lazyspec init`,
  Then `docs/convention/example.md` is created with valid dictum frontmatter (type: dictum, status: draft, tags: [example]) and placeholder content explaining the dictum format

- Given the skeleton convention `index.md`,
  When the file is created by `init`,
  Then the `date` field is populated with the current date and `author` defaults to "unknown"

- Given the skeleton dictum `example.md`,
  When the file is created by `init`,
  Then the `date` field is populated with the current date and `author` defaults to "unknown"

- Given an existing project where `docs/convention/index.md` already exists,
  When a user runs `lazyspec init`,
  Then the existing convention and dictum files are not overwritten

## Scope

### In Scope

- Convention and dictum entries in the `default_types` configuration
- Skeleton `docs/convention/index.md` content created by `lazyspec init`
- Skeleton `docs/convention/example.md` content created by `lazyspec init`
- Template variable substitution for `{date}` and `{author}` in skeleton files

### Out of Scope

- `singleton` and `parent_type` fields on `TypeDef`, and their enforcement in create/validation (Story 1)
- `lazyspec convention` CLI subcommand and its flags (Story 2)
- Skill preflight dictum loading and boot hook configuration (Story 4)
