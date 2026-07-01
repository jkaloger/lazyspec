---
title: release-plz version bump + crates.io publish
type: iteration
status: accepted
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-189
---## Objective

release-plz bumps Cargo.toml + tags + publishes crate on merge to main. No GH Release, no artifacts here.

## Context

- Story: STORY-189 (release-plz slice, AC1-AC2)
- RFC: RFC-024 §5 (deferred release workflow, now specced)
- Touch: `release-plz.toml` (new, repo root), `.github/workflows/release-plz.yml` (new)
- release-plz's own doc'd recipe: single workflow, two jobs, both on `push: main` -- `release-plz-pr` (open/update release PR w/ version bump + changelog), `release-plz-release` (if HEAD is a release-plz commit: tag + publish)
- crates.io Trusted Publishing (OIDC) setup on crates.io side is manual, out-of-band -- assume done, do not gate on it here

## Satisfies

STORY-189 AC1, AC2.

## Tasks

1. `release-plz.toml`: `git_release_enable = false` (GH Release owned by ITERATION-2, not here). Leave changelog/version-bump defaults -- best-effort commit parsing, no lint gate added.
2. `.github/workflows/release-plz.yml`: `release-plz-pr` job (release-pr command) + `release-plz-release` job (release command), both `on: push: branches: [main]`. Perms: `contents: write`, `pull-requests: write`, `id-token: write` (OIDC for step 3).
3. Wire crates.io publish thru Trusted Publishing in the release job -- no `CARGO_REGISTRY_TOKEN` secret anywhere in this workflow.
4. Verify locally: `release-plz update --dry-run` against current HEAD -- confirm it reads Cargo.toml's `0.9.0` and proposes a sane next version from recent commit history.

## Out of scope

- Cross-compile CLI builds, macOS `.app` build, GH Release + artifacts -> next iteration (STORY-189 AC3-AC6).
- Commit-msg format enforcement / commitlint -- best-effort per decision, not this slice.
- crates.io Trusted Publishing trust-relationship registration (crates.io website) -- manual, out-of-band.

## Principles/conventions

CONVENTION.md engine/CLI/TUI layering -- N/A, no `src/` touched.

## Verification

`release-plz update --dry-run` output shows next version + changelog entry matching latest commits, no `CARGO_REGISTRY_TOKEN` referenced anywhere in the new workflow file.