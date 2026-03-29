---
title: Certification Workflow
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags: []
related:
- implements: RFC-038
---





## Context

Specs describe the system as-is, pinned to code via `@ref` directives and verified by acceptance criteria. Certification is the human act of asserting "these AC are true right now," backed by converging signals: refs resolve, blob hashes match, tests pass. The `lazyspec certify` command orchestrates this workflow, mutating spec files on disk so the developer can review and commit in a single pass.

This story covers the `certify` command itself, the frontmatter it writes, test execution integration, the `--skip-tests` escape hatch, re-certification after drift, and CI validation via `lazyspec validate --strict`.

## Acceptance Criteria

### AC: certify-resolves-refs

Given a spec with `@ref` directives
When `lazyspec certify <spec-id>` is run
Then all `@ref` targets are resolved at HEAD, and any unresolvable ref causes certification to fail with an error identifying the broken ref

### AC: certify-pins-unpinned-refs

Given a spec with unpinned `@ref` directives (no `@{blob:hash}` suffix)
When `lazyspec certify <spec-id>` is run
Then each unpinned ref is pinned by computing its normalized blob hash from the working tree and writing `@{blob:hash}` into the directive

### AC: certify-computes-story-hashes

Given a spec with linked Story documents containing `### AC: <slug>` sections
When `lazyspec certify <spec-id>` is run
Then AC are collected from all linked stories, a content hash is computed for each AC section, and stored as a `story_hashes` map (slug to hash) in the spec's frontmatter

### AC: certify-writes-frontmatter

Given all refs resolve, all pins are current, and tests pass (or are skipped)
When `lazyspec certify <spec-id>` completes successfully
Then `certified_by`, `certified_date`, and `story_hashes` are written to the spec's frontmatter, and the file is mutated on disk without committing

### AC: certify-runs-tests

Given a test runner is configured in `.lazyspec.toml` and the spec has `@ref` test targets
When `lazyspec certify <spec-id>` is run
Then test function names are extracted from the `@ref` test targets, passed as filters to the configured test runner, and all must pass for certification to succeed

### AC: certify-refuses-on-test-failure

Given a spec with `@ref` test targets and a configured test runner
When `lazyspec certify <spec-id>` is run and any referenced test fails
Then certification is refused, the failing test(s) are reported, and no frontmatter is written

### AC: certify-skip-tests

Given a spec with `@ref` test targets
When `lazyspec certify <spec-id> --skip-tests` is run
Then certification proceeds without executing tests, a warning is emitted indicating tests were skipped, and frontmatter is written normally

### AC: certify-no-test-runner-warning

Given no test runner is configured in `.lazyspec.toml`
When `lazyspec certify <spec-id>` is run on a spec with test refs
Then certification falls back to symbol resolution only, emits a warning that certification is partial (no behavioural verification), and writes frontmatter

### AC: recertify-repins-drifted-refs

Given a previously certified spec where one or more pinned refs have drifted (blob hash mismatch)
When `lazyspec certify <spec-id>` is run again
Then drifted refs are re-pinned to their current normalized blob hashes, tests are re-run, and frontmatter is updated with the new certification date and story hashes

### AC: validate-strict-runs-tests

Given a certified spec and a CI environment with `lazyspec validate --strict`
When validation runs post-merge
Then the full test suite for each certified spec is executed as a non-bypassable check, and any test failure causes validation to fail

### AC: validate-strict-checks-certification

Given a spec whose certification has been invalidated by squash merge (blob hash mismatch at HEAD)
When `lazyspec validate --strict` runs in CI
Then validation reports the spec's certification as stale and fails

## Scope

### In Scope

- `lazyspec certify <spec-id>` command implementation
- Ref resolution, unpinned ref pinning (normalized blob hash computation), per-AC content hashing across linked stories
- Frontmatter fields: `certified_by`, `certified_date`, `story_hashes`
- Test runner integration via `.lazyspec.toml` configuration
- `--skip-tests` flag with warning
- Re-certification workflow (re-pin, re-test, update frontmatter)
- `lazyspec validate --strict` for CI (full test suite, certification validity check)

### Out of Scope

- The `spec` document type, directory structure, or migration from `arch` (Story 1)
- Blob pinning syntax or `lazyspec pin` as a standalone command (Story 2)
- Drift detection signals or `lazyspec drift` reporting (Story 3)
- The `affects` relationship type and coverage advisories (Story 5)
