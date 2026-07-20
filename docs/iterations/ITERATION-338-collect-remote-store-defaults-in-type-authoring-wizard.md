---
title: Collect remote-store defaults in type-authoring wizard
type: iteration
status: complete
author: unknown
date: 2026-07-20
tags: []
related:
- implements: STORY-229
---

## Objective

When the type-authoring wizard picks a remote store, prompt for that store's
sensible defaults so the written `TypeDef` is sync-ready instead of hard-nulled.

## Satisfies

STORY-229 — extends its type-authoring wizard with a new capability
(remote-store defaults). No prior STORY-229 AC covers this; new behaviour.

## Context

- Story: STORY-229 (init wizard UX polish — this adds capability, not polish)
- RFC: docs/rfcs/RFC-062 (interactive config wizard + type authoring)
- Prompter seam + conventions: src/cli/wizard.rs (Prompter/ScriptedPrompter)
- Touch:
  - src/cli/config.rs — CollectedType (add remote fields), collect_type_interactive
    (store-conditional prompts), type_def_from_parts (thread fields, stop hard-nulling),
    apply_collected_type, AddType clap args + run_add_type parity flags
  - src/main.rs (~641) — wire new add-type flags
  - README.md — document new prompts + flags

## Tasks

1. Extend `CollectedType` with `github_issue_tag`, `github_issue_type`,
   `clickup_list_id` (`clickup_task_type` already threads).
2. In `collect_type_interactive`, after store select, branch on chosen store:
   - `clickup-tasks` → prompt clickup_list_id (blank allowed, fill later) + clickup_task_type
   - `github-issues` → prompt github_issue_tag + github_issue_type
   - others → collect nothing
3. Thread the fields through `type_def_from_parts` (remove the unconditional
   `None`s for github_issue_tag/type + clickup_list_id) and `apply_collected_type`.
4. Add `--github-issue-tag`, `--github-issue-type`, `--clickup-list-id` flags to
   `config add-type` + `run_add_type`; wire in src/main.rs (non-interactive parity).
5. Test-first: ScriptedPrompter cases — clickup-tasks type captures list id + task
   type; github-issues type captures tag + type; filesystem type prompts neither.
6. README: new prompts + add-type flags.

## Out of scope

- github-milestones defaults (no meaningful per-type field).
- clickup_custom_field_map / label_override collection.
- Engine/TUI/web — CLI-only; no sync-logic change (fields already consumed by
  clickup_cache.rs / sync.rs).

## Principles/conventions

- .lazyspec convention: --json/non-interactive byte-parity, layering (CLI only),
  fakes only at the Prompter seam (principle 4).

## Verification

- clickup-tasks type built via wizard now writes clickup_list_id → no longer
  errors at clickup_cache.rs:47 / sync.rs:234.

