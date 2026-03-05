---
title: Expanded Validation
type: iteration
status: draft
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-022-expanded-validation.md
---


## Changes

- Added `ValidationIssue` enum with 6 variants (3 existing as errors + 3 new)
- Added `ValidationResult` struct with separate `errors` and `warnings` vecs
- Added `validate_full()` to Store, implementing superseded parent (warning), rejected parent (error), orphaned acceptance (warning)
- Added `--warnings` flag to `validate` command
- Updated JSON output to `{ "errors": [], "warnings": [] }` format
- Added `run_full`, `run_json`, `run_human` to validate CLI
- Kept original `validate()` and `run()` for backward compatibility

## Test Plan

- `superseded_parent_warning` — AC1: accepted doc implementing superseded doc produces warning
- `rejected_parent_error` — AC2: doc implementing rejected doc produces error
- `orphaned_acceptance_warning` — AC3: accepted iteration with draft parent story produces warning
- `warnings_dont_affect_exit_code` — AC4: warnings alone = exit 0
- `validate_json_has_separate_arrays` — AC6: JSON has separate errors/warnings arrays
- `validate_without_warnings_flag_hides_warnings` — AC5: no --warnings hides warnings
- `validate_with_warnings_flag_shows_warnings` — AC5: --warnings shows warnings

## Notes

All 6 Story ACs covered. Original `validate()` method preserved for existing test compatibility.
