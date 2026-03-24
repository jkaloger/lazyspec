---
title: Drift Detection and Signals
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags:
- certification
- drift-detection
- signals
related:
- implements: docs/rfcs/RFC-031-spec-certification-and-drift-detection.md
---


## Context

Specs pin themselves to code via `@ref` directives with blob hashes, and define behavioural claims via acceptance criteria in `story.md`. When code, tests, or AC change after certification, the system needs to detect this and surface it. This story covers the signal collection and drift reporting machinery that answers: "has anything changed since this spec was last certified?"

Five signal types feed the same question: symbol drift, test drift, test failure, AC mutation, and scope change. No single signal is a verdict. They gain meaning through convergence, and the system presents them per-spec for human triage.

Drift during active development (where an in-progress iteration `implements` or `affects` a spec) is expected and should be tagged as such, so developers are not overwhelmed by noise from their own work.

## Acceptance Criteria

### AC: symbol-drift-detection

Given a spec with blob-pinned `@ref` implementation targets in `index.md`
When `lazyspec drift <spec-id>` is run
Then each pinned impl ref is classified as CURRENT (hash matches), DRIFTED (symbol exists but hash differs), or ORPHANED (symbol not found at HEAD)

### AC: test-drift-detection

Given a spec with blob-pinned `@ref` test targets in `index.md`
When `lazyspec drift <spec-id>` is run
Then each pinned test ref is classified as CURRENT, DRIFTED, or ORPHANED, independently from impl refs

### AC: ac-mutation-detection

Given a certified spec with `story_hashes` in its index frontmatter
When `lazyspec drift <spec-id>` is run
Then each `### AC: <slug>` section in `story.md` is hashed and compared against the stored hash, reporting per-AC whether content is unchanged, modified, added, or removed

### AC: scope-change-detection

Given a certified spec with `story_hashes` recording AC slugs at certification time
When an AC slug is added to or removed from `story.md`
Then `lazyspec drift` reports the added or removed slugs as scope changes distinct from content modifications

### AC: test-failure-signal

Given a spec with `@ref` test targets and a test runner configured in `.lazyspec.toml`
When `lazyspec drift --run-tests <spec-id>` is invoked
Then referenced test functions are executed and pass/fail results are included in the drift report

### AC: drift-command-output

Given a spec with one or more active signals
When `lazyspec drift <spec-id>` is run
Then the output groups signals by type (symbol drift, test drift, AC mutation, scope change) with per-ref and per-AC detail

### AC: drift-command-json

Given a spec with active signals
When `lazyspec drift <spec-id> --json` is run
Then the output is machine-readable JSON containing signal type, ref path, status (CURRENT/DRIFTED/ORPHANED), and per-AC change categories

### AC: status-drift-integration

Given one or more specs with active drift signals
When `lazyspec status` is run
Then each spec's entry includes a signal summary showing counts per signal type and an urgency assessment based on signal convergence

### AC: expected-drift-tagging

Given a draft iteration with an `implements` or `affects` relationship to a spec
When `lazyspec status` reports drift on that spec
Then the drift is tagged as "expected" with the iteration identifier, rather than appearing as unexpected

### AC: unexpected-only-filter

Given specs with both expected and unexpected drift
When `lazyspec status --unexpected-only` is run
Then only specs with unexpected drift signals are shown in the output

### AC: draft-spec-suppresses-story-hash

Given a spec with `status: draft` that has not been certified
When `lazyspec drift` is run against it
Then AC mutation and scope change signals are suppressed (no story_hash comparison), since the AC are still being authored

### AC: convergence-urgency

Given a spec with multiple signal types firing simultaneously (e.g. drifted symbols, changed test, modified AC)
When the drift report or status output is generated
Then urgency is assessed based on signal convergence: more signal types firing means higher urgency

## Scope

### In Scope

- Signal collection for all five types: symbol drift, test drift, test failure, AC mutation, scope change
- `lazyspec drift` command with per-spec signal reporting
- Symbol/test drift via stored blob hash vs current HEAD hash comparison (CURRENT/DRIFTED/ORPHANED states)
- AC mutation detection via per-AC content hashing against stored `story_hashes`
- Scope change detection via AC slug comparison (added/removed since certification)
- Per-AC granular reporting (which specific ACs changed, added, or removed)
- Expected-drift suppression using draft iteration `implements`/`affects` relationships
- `lazyspec status` integration showing drift signals per-spec with urgency assessment
- `lazyspec status --unexpected-only` filter
- `lazyspec drift --run-tests <spec-id>` for optional test execution signal
- `lazyspec drift --json` machine-readable output

### Out of Scope

- The `spec` document type itself, directory structure, migration, and validation rules (Story 1)
- Blob pinning, `@{blob:hash}` syntax, AST normalization, and `lazyspec pin` (Story 2)
- Certification workflow and `lazyspec certify` (Story 4)
- The `affects` relationship type creation (Story 5, though this story consumes it if present)
- Coverage advisories (Story 5)
- Ref index caching (may be needed for performance but is an implementation detail, not a signal concern)
- Agent skills (`/write-spec`, `/certify-spec`, `/audit-cert`)
