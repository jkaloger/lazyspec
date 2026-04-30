---
title: Priority field and TOML config
type: story
status: complete
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
- blocks: STORY-121
- blocks: STORY-123
- blocks: STORY-114
priority: should
---







## Summary

Introduce a typed `priority` frontmatter field with a configurable vocabulary defined in `lazyspec.toml`. The default vocabulary is MoSCoW (`must=4, should=3, could=2, wont=1`). Doc-type configuration gains a `requires_priority` flag (defaults: story and iteration require priority; other types treat it as optional). Frontmatter validation rejects unknown priority keys and reports missing values for required types. The engine exposes the configured weights so downstream consumers (graph layer, critical-path command, TUI) can read them.

This story also migrates per-type **terminal status sets** off the hardcoded match introduced in STORY-120's iter 1 onto a new `TypeDef::terminal_statuses` config field. Same `TypeDef` plumbing as `requires_priority`; one PR cleanly replaces the hardcode with config-driven lookup. Decision recorded during STORY-120 grilling pass (Q4): defer config plumbing to this story since `TypeDef` is already being touched here.

## Scope

### In Scope

- A typed `priority` frontmatter field on documents.
- Parsing `[priorities.<key>] weight = N` blocks from `lazyspec.toml`.
- Default MoSCoW vocabulary applied when no `[priorities.*]` blocks are present.
- User overrides that redefine the vocabulary entirely (different keys, counts, weights).
- A `requires_priority` flag on doc-type config; defaults set so story and iteration require it, all other types do not.
- A `terminal_statuses` field on doc-type config; defaults seed the RFC-041 spec values (RFC/Story `{complete, superseded, rejected}`; Iteration/Audit `{complete}`; ADR/Convention/Dictum `{accepted, superseded}`).
- Migrate `engine::sequencing::is_terminal` from STORY-120's hardcoded match onto the new `terminal_statuses` config lookup. Hardcode is removed once tests pass against config-driven resolution.
- Frontmatter validation: reject unknown priority keys; report missing priority for required doc types; accept missing priority for non-required doc types.
- Engine surface that exposes the resolved weight map to downstream consumers.

### Out of Scope

- The engine `Graph` and `critical_path` implementations (Story 1).
- The CLI `critical-path` command surface (Story 3).
- TUI priority editing or budget panel (Story 4c).
- TUI colour theming for priorities.
- The leaf-claimable heuristic (Iteration vs Story decomposition signal). Tracked separately under STORY-120 ADR follow-up; may join this story if the impl converges naturally.

## Acceptance Criteria

1. **Given** a project with no `[priorities.*]` blocks in `lazyspec.toml`,
   **When** the engine loads configuration,
   **Then** the resolved priority vocabulary equals the MoSCoW default (`must=4, should=3, could=2, wont=1`).

2. **Given** a `lazyspec.toml` that defines its own `[priorities.*]` blocks with different keys and weights,
   **When** the engine loads configuration,
   **Then** the resolved priority vocabulary contains only the user-defined keys and weights, with no MoSCoW keys merged in.

3. **Given** a document whose frontmatter `priority` value is not a key in the resolved vocabulary,
   **When** validation runs,
   **Then** the document is reported as invalid with an error identifying the unknown priority key.

4. **Given** a document of a type configured with `requires_priority = true` and no `priority` field in its frontmatter,
   **When** validation runs,
   **Then** the document is reported as invalid with an error indicating the priority field is required for that type.

5. **Given** a document of a type configured with `requires_priority = false` and no `priority` field in its frontmatter,
   **When** validation runs,
   **Then** the document is accepted without a priority-related error.

6. **Given** a loaded configuration with a resolved priority vocabulary,
   **When** a downstream consumer requests the configured weights from the engine,
   **Then** the consumer receives a mapping from each priority key to its weight that matches the configuration.

7. **Given** a project with no per-type `terminal_statuses` overrides in `lazyspec.toml`,
   **When** the engine evaluates terminal status for documents of each built-in type,
   **Then** the resolved terminal sets equal the RFC-041 defaults (RFC/Story `{complete, superseded, rejected}`; Iteration/Audit `{complete}`; ADR/Convention/Dictum `{accepted, superseded}`).

8. **Given** a `lazyspec.toml` that overrides `terminal_statuses` for a doc type,
   **When** the engine evaluates terminal status for a document of that type,
   **Then** the override is honoured and the built-in default is not merged in.

9. **Given** the migration of `is_terminal` to config-driven resolution,
   **When** STORY-120's existing tests for ACs 10 and 11 run,
   **Then** all tests still pass with no behavioural change observable to callers, and the hardcoded match in `engine::sequencing` is removed.
