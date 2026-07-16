---
title: 'Mutation correctness fixes: link panic, git-ref update, shorthand ambiguity, create json id'
type: iteration
status: in-progress
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-210
- blocks: ITERATION-299
---

## Objective

Four mutation correctness fixes: link null-related panic, git-ref update key drop + quoting, shorthand ambiguity, create --json id.

## Satisfies

STORY-210 AC1, AC2, AC3, AC4.

## Context

- Findings: AUDIT-018 C2, C3, C6, F5
- Touch: `src/cli/link.rs:123` (cf. correct handling `unlink_inner:463`), `src/engine/git_ref_store.rs:184-187` (cf. `set_provenance:262-274` pattern), `src/engine/store.rs:196-222` (cf. `resolve_unqualified:239-262`), `src/cli/create.rs` (json output path)

## Tasks

1. link.rs: bare `related:` (YAML null) → coerce to empty sequence. Test.
2. git_ref_store update: serde_yaml frontmatter round-trip; missing key inserted, values quoted. Tests: new key, `Plan: phase 2` title.
3. resolve_shorthand PARENT/child: exact-id preference + ambiguity error. Test RFC-1 vs RFC-12.
4. create --json: resolve id from written path before serialize. Test asserts non-empty id.
5. `cargo test`.

## Out of scope

Other resolve paths (already guarded). Non-git-ref update paths.

## Verification

Each AC has failing-then-passing test. No `unwrap()` on `as_sequence_mut` remains in link.rs.

