---
title: Bulk Update
type: story
status: draft
author: jkaloger
date: 2026-03-05
tags:
- cli
- update
related:
- implements: RFC-008
---



## Context

After auditing project health, fixing status drift means running `lazyspec update` once per document. When 11 documents need the same status change, that's 11 invocations. The `update` command should accept multiple paths so agents and humans can fix drift in a single call.

## Acceptance Criteria

### AC1: Multiple paths accepted

**Given** two or more valid document paths
**When** `lazyspec update <path1> <path2> --status accepted` is run
**Then** all specified documents are updated with the given status

### AC2: Partial failure continues

**Given** a mix of valid and invalid document paths
**When** `lazyspec update <valid> <invalid> <valid> --status accepted` is run
**Then** valid documents are updated, invalid paths are reported as errors, and the command does not abort on the first failure

### AC3: Exit code reflects failures

**Given** a bulk update where some paths fail
**When** the command completes
**Then** exit code is non-zero if any path failed, zero if all succeeded

### AC4: JSON output

**Given** a bulk update
**When** `lazyspec update <paths> --status accepted --json` is run
**Then** output is a JSON object with `updated` (array of successfully updated paths) and `failed` (array of objects with path and error message)

### AC5: Single path backward compatibility

**Given** a single document path
**When** `lazyspec update <path> --status accepted` is run
**Then** behavior is identical to current single-path update

### AC6: All field flags work in bulk

**Given** multiple paths and the `--title` or `--status` flags
**When** `lazyspec update <path1> <path2> --status accepted --title "New Title"` is run
**Then** all specified fields are updated on all specified documents

## Scope

### In Scope

- `update` accepting `Vec<String>` paths instead of single `String`
- Continue-on-failure semantics
- JSON output with updated/failed arrays
- Backward compatibility with single-path usage

### Out of Scope

- Glob/wildcard path expansion (e.g. `docs/stories/*.md`)
- Dry-run mode
- Interactive confirmation for bulk changes
