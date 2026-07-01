---
title: Cross-compile CLI + macOS .app + GitHub Release
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-189
---## Objective

Tag push (`v*`) triggers cross-compiled CLI builds + macOS `.app` build + GH Release w/ artifacts, auto-notes.

## Context

- Story: STORY-189 (build/release slice, AC3-AC6)
- Depends on ITERATION-258 (release-plz pushes the `v*` tag this workflow triggers on)
- Existing `.app` build cmd: ITERATION-257, `cargo tauri build --features app` (unsigned, no change here)
- Touch: `.github/workflows/release.yml` (new)
- Runner labels: macOS arm64 = `macos-14`, macOS x64 = `macos-13`, Linux = `ubuntu-latest`. Native per-target, no cross toolchain (tree-sitter needs matching C compiler)

## Satisfies

STORY-189 AC3, AC4, AC5, AC6.

## Tasks

1. `.github/workflows/release.yml`, `on: push: tags: ['v*']`. Matrix job: `{macos-14, aarch64-apple-darwin}`, `{macos-13, x86_64-apple-darwin}`, `{ubuntu-latest, x86_64-unknown-linux-gnu}` -- `cargo build --release --target <triple>`, archive to `.tar.gz`, name `lazyspec-<tag>-<triple>.tar.gz`.
2. Same macOS legs, extra step: `cargo tauri build --features app`, zip `lazyspec.app` -> `lazyspec-<tag>-<arch>-app.zip`.
3. `needs:` all matrix legs, final job: `gh release create <tag> --generate-notes` + upload all archives as release assets.
4. Fail-closed: no release-create step runs if any matrix leg fails (job dependency via `needs`, no `continue-on-error`).

## Out of scope

- release-plz, versioning, crates.io publish -> ITERATION-258, already done.
- Code signing / notarization of `.app` -- deferred per RFC-054 ADR, not reopened.
- Windows/Linux `.app`, musl target -- out of STORY-189 scope entirely.

## Principles/conventions

CONVENTION.md engine/CLI/TUI layering -- N/A, no `src/` touched.

## Verification

Push a test tag on a throwaway branch/fork (or `act`/local dry-run of the matrix step), confirm 5 artifacts (3 CLI archives + 2 `.app` zips, one per macOS arch) attach to one GH Release, notes are GitHub's auto-generated PR list.