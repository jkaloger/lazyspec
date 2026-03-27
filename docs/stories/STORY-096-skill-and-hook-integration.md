---
title: Skill and Hook Integration
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags:
- convention
- dictum
- skills
- hooks
related:
- implements: docs/rfcs/RFC-034-convention-and-dictum-document-types.md
---



## Context

RFC-034 introduces a hybrid context-surfacing model: convention preamble loads at agent boot via a Claude Code hook, and dictum are pulled selectively by skills during their preflight phase. This story covers the integration points that connect the `lazyspec convention` CLI subcommand (Story 2) to the skill and hook system that consumes it.

## Acceptance Criteria

- Given a Claude Code project with lazyspec configured
  When the `user-prompt-submit` hook fires
  Then `lazyspec convention --preamble` runs and injects the convention preamble into the agent context

- Given no convention document exists in the project
  When the boot hook runs `lazyspec convention --preamble`
  Then the command exits successfully with empty output (no error, no crash)

- Given no convention document exists in the project
  When a skill calls `lazyspec convention --tags <any-tag> --json`
  Then the command returns an empty JSON result (no error, no crash)

- Given a convention with dictum tagged `testing` and `architecture`
  When the `/build` skill runs its preflight phase
  Then it calls `lazyspec convention --tags <relevant-tags> --json` and receives matching dictum

- Given a convention with dictum tagged `rfc`
  When the `/write-rfc` skill runs its preflight phase
  Then it calls `lazyspec convention --tags rfc --json` and receives matching dictum

- Given a convention with dictum tagged `iteration`
  When the `/create-iteration` skill runs its preflight phase
  Then it calls `lazyspec convention --tags iteration --json` and receives matching dictum

- Given a skill calls `lazyspec convention --tags` with tags that match no dictum
  When the command completes
  Then the skill proceeds without injecting any dictum context (graceful no-op)

- Given the hook configuration in `.claude/settings.json`
  When a user inspects the hook setup
  Then the `user-prompt-submit` hook entry references `lazyspec convention --preamble` with type `intercept`

## Scope

### In Scope

- Boot hook configuration for convention preamble injection via `user-prompt-submit`
- Updating `/write-rfc`, `/build`, `/create-iteration`, and other relevant skills to call `lazyspec convention --tags <tags> --json` during preflight
- Graceful degradation: empty/no-op behavior when no convention or matching dictum exist
- Documentation of the hook setup and skill integration pattern

### Out of Scope

- `singleton` and `parent_type` fields on `TypeDef`, engine-level validation (Story 1)
- The `lazyspec convention` CLI subcommand itself and its flags (Story 2)
- Default convention/dictum skeleton files created by `lazyspec init` (Story 3)
