---
title: Status Command
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: STORY-020
---




## Changes

- Added `src/cli/status.rs` with `run_json` and `run_human`
- Added `Status` subcommand to CLI with `--json` flag
- JSON output: `{ "documents": [...], "validation": { "errors": [], "warnings": [] } }`
- Human-readable output: compact table grouped by type (RFC, STORY, ITERATION, ADR)
- Reuses `doc_to_json` (ITERATION-006) and `validate_full` (ITERATION-008)

## Test Plan

- `status_json_has_documents_and_validation` — AC1/AC2: JSON has documents array and validation object
- `status_json_includes_all_documents` — AC1: all 3 docs present
- `status_json_documents_use_full_schema` — AC2: full 8-field schema
- `status_human_grouped_by_type` — AC4: grouped table with all types and titles
- `status_empty_project` — AC5: empty arrays in JSON, empty string in human output

## Notes

AC3 (inline validation) is implicitly covered by `status_json_has_documents_and_validation` which checks the validation object is present. All 5 Story ACs covered.
