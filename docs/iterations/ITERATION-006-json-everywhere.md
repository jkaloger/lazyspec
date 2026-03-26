---
title: JSON Everywhere
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: STORY-021
---




## Changes

- Added `src/cli/json.rs` with `doc_to_json()` helper producing consistent schema: path, title, type, status, author, date, tags, related
- Added `--json` flag to `show` command (`run_json`) — includes body field
- Added `--json` flag to `create` command (`run_json`) — parses created file back into schema
- Updated `list --json` to use full schema (was only path, title, type, status)
- Updated `search --json` to use full schema plus match_field and snippet
- AC4 (context --json) deferred to STORY-019 iteration since `context` command doesn't exist yet

## Test Plan

- `doc_to_json_includes_full_schema` — all 8 schema fields present
- `doc_to_json_includes_related` — related array with type and target
- `show_json_includes_body` — body field present in show output
- `show_json_output` — full show --json round-trip with all fields
- `create_json_output` — create --json returns created doc schema
- `list_json_includes_full_schema` — list --json now includes author, date, tags, related
- `search_json_includes_full_schema` — search --json includes full schema plus match_field/snippet

## Notes

Type field uses lowercase in JSON (`rfc`, `story`, `iteration`, `adr`) matching the YAML frontmatter format, not the Display impl's uppercase format.
