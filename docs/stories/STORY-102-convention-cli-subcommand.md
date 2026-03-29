---
title: Convention CLI Subcommand
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: docs/rfcs/RFC-034-convention-and-dictum-document-types.md
---



## Context

RFC-034 introduces convention and dictum document types. Agents need a way to query convention and dictum content from the CLI, both for boot-time context loading and for selective retrieval during skill execution. This story covers the `lazyspec convention` subcommand that reads from the store and formats output for human and machine consumption.

## Acceptance Criteria

- Given a project with a convention document and dictum children,
  When the user runs `lazyspec convention`,
  Then the output contains the convention preamble followed by all dictum, separated by headings.

- Given a project with a convention document,
  When the user runs `lazyspec convention --preamble`,
  Then only the convention `index.md` content is returned (no dictum).

- Given dictum tagged with "testing" and "architecture",
  When the user runs `lazyspec convention --tags testing`,
  Then only dictum with the "testing" tag are included in output.

- Given dictum tagged with "testing" and "architecture",
  When the user runs `lazyspec convention --tags "testing,architecture"`,
  Then dictum matching either tag are included (OR logic).

- Given a project with a convention and dictum,
  When the user runs `lazyspec convention --json`,
  Then the output is structured JSON containing the preamble and matching dictum.

- Given `--preamble` and `--tags` are both provided,
  When the user runs `lazyspec convention --preamble --tags testing`,
  Then only the preamble is returned (`--preamble` takes precedence over `--tags`).

- Given no convention document exists in the project,
  When the user runs `lazyspec convention`,
  Then a clear error message is returned indicating no convention was found.

- Given a project with a convention but no dictum,
  When the user runs `lazyspec convention`,
  Then the convention preamble is returned with no dictum section.

## Scope

### In Scope

- New `lazyspec convention` CLI subcommand
- `--preamble` flag to return only the convention `index.md`
- `--tags` flag accepting comma-separated values with OR filtering logic
- `--json` flag for structured output
- Default behaviour: preamble followed by all dictum with heading separators
- Reading from the document store (no special file handling)

### Out of Scope

- Engine `singleton` and `parent_type` fields on `TypeDef` (Story 1)
- `lazyspec init` scaffolding for convention/dictum defaults (Story 3)
- Skill preflight integration and boot hook configuration (Story 4)
- Write operations (creating/editing convention or dictum via this subcommand)
