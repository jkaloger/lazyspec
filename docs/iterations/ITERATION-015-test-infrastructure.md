---
title: Test Infrastructure
type: iteration
status: draft
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-028-engine-and-cli-quality.md
---




## Problem

17 test files repeat the same setup patterns: create tempdir, create doc directories, write YAML frontmatter fixtures, load Config + Store. There are no shared test helpers. Setup functions like `setup_with_chain`, `setup_app`, `write_doc`, `setup_dirs` exist as file-local copies with slight variations.

## Changes

Stub. Full task breakdown to be written when this iteration is picked up. High-level scope:

- Create `tests/common/mod.rs` with shared fixture builders
- A `TestFixture` struct that manages tempdir lifetime and provides helpers like `add_rfc(status)`, `add_story(status, implements)`, `add_iteration(status, implements)`
- Migrate existing test files to use shared helpers, removing file-local duplicates
- No behavior changes to the tests themselves, just infrastructure consolidation

## Test Plan

Existing test suite is the verification. Every test must continue to pass after migration. No new tests.

## Notes

- Independent of ITERATION-013 and ITERATION-014. Can be done at any time.
- Rough estimate: 17 test files to audit, probably 8-10 have duplicated setup worth extracting.
