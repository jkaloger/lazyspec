---
title: RFC-027 sqids implementation quality review
type: audit
status: complete
author: agent
date: 2026-03-17
tags: []
related:
- related-to: RFC-027
- related-to: ITERATION-074
---




## Scope

Code quality review of the sqids numbering implementation introduced by RFC-027. Audit type: code quality. Focused on `src/cli/fix.rs` which contains the renumber and external reference scanning logic.

## Criteria

- No unnecessary code duplication (DRY)
- Safe filesystem traversal (skip noise directories)
- Correct handling of mixed-format document directories (no ID collisions)

## Findings

### Finding 1: scan_external_references doesn't skip .git/target/node_modules dirs

**Severity:** medium
**Location:** `src/cli/fix.rs:359` (`scan_dir_for_references`)
**Description:** The directory walker only skips directories listed in `config.types[].dir` (the managed doc directories). It does not skip conventional noise directories like `.git`, `target/`, `node_modules/`, `.venv/`, etc. On real projects this causes unnecessary I/O and risks false-positive reference matches inside vendored or build-artifact files.
**Recommendation:** Add a hardcoded skip-list for common noise directories (`.git`, `target`, `node_modules`, `.venv`, `dist`, `build`) or, better, make the skip-list configurable.

### Finding 2: run_renumber_json duplicates logic from run_renumber

**Severity:** low
**Location:** `src/cli/fix.rs:508` (`run_renumber_json`) and `src/cli/fix.rs:98` (`run_renumber`)
**Description:** `run_renumber_json` is a near-copy of `run_renumber`. Both call `collect_renumber_fixes` + `scan_external_references`, construct an identical `RenumberOutput`, and serialize it. The only difference is the output sink: one prints to stdout, the other returns a `String`. Any future change to the output structure must be applied in both places.
**Recommendation:** Extract the shared logic into a single function that returns the `RenumberOutput`, then have `run_renumber` and `run_renumber_json` call it and handle presentation separately.

### Finding 3: Potential ID collision in sqids-to-incremental conversion

**Severity:** high
**Location:** `src/cli/fix.rs:262-267` (inside `collect_renumber_fixes`, `RenumberFormat::Incremental` branch)
**Description:** When converting sqids-format docs to incremental, the code filters to only sqids docs and then assigns numbers starting at 1 (`let new_num = (i + 1) as u32`). It does not account for existing incremental docs in the same type. If `RFC-001-foo.md` already exists alongside `RFC-abc-bar.md`, the sqids doc gets assigned `RFC-001`, colliding with the existing file.
**Recommendation:** Before assigning incremental numbers, collect the set of already-used incremental IDs for the type and start numbering above the maximum, or interleave the new docs into gaps.

## Summary

Three findings across one file. The ID collision (finding 3) is the highest priority since it can cause data loss through file overwrites. The missing directory skip-list (finding 1) is a practical concern for any project with `node_modules` or a `target/` build directory. The DRY violation (finding 2) is low severity but straightforward to fix and reduces future maintenance risk.
