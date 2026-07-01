---
title: Release workflow
type: story
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: RFC-024
---## Context

RFC-024 established Nix-based CI (STORY-118, STORY-119) but explicitly deferred release automation to a follow-up (§5, "Release Workflow (future)"): cross-compiled binaries, GitHub Release creation, and versioning strategy. This story is that follow-up.

Since RFC-024 was written, RFC-054 added a second release artifact: a macOS `.app` bundle behind the `app` cargo feature (Tauri-based read-only web view). ITERATION-257 already produces an unsigned `lazyspec.app` locally via `cargo tauri build --features app`; code signing and notarization are deliberately deferred by RFC-054's ADR ("defer code signing / notarization") and stay deferred here. This story wires that existing bundle command into the release pipeline, it does not revisit signing.

Everything routes through a git tag:

- **Versioning**: `release-plz` watches conventional commits on `main`, opens a release PR that bumps `Cargo.toml` and writes a changelog entry. Merging that PR is the only way a version changes and a tag gets created — `release-plz` creates the tag from the version it just bumped, so tag and `Cargo.toml` can't disagree by construction. No separate manual-tag-then-verify step exists to drift.
- **crates.io publish**: `release-plz`'s own publish step, using crates.io Trusted Publishing (OIDC) — no long-lived `CARGO_REGISTRY_TOKEN` secret.
- **Artifacts + GitHub Release**: a separate `release.yml`, triggered by the tag `release-plz` pushes, builds the cross-compiled CLI binaries and the macOS `.app`, then creates the GitHub Release with `--generate-notes` (GitHub's PR-title-based notes) and attaches the artifacts. `release-plz`'s own GitHub-Release creation is disabled (`git_release_enable = false`) so notes come from one place, not two.

## Acceptance Criteria

- Given a conventional-commit-style change merges to `main`,
  when `release-plz`'s scheduled/triggered run inspects the history,
  then it opens (or updates) a release PR bumping `Cargo.toml`'s version and adding a changelog entry, with no version change happening any other way.

- Given the release PR is merged to `main`,
  when `release-plz`'s release job runs,
  then it tags the merge commit `vX.Y.Z` matching the bumped `Cargo.toml` version, and publishes the crate to crates.io via Trusted Publishing (OIDC, no stored token).

- Given a tag matching `v*` is pushed,
  when the release workflow triggers,
  then it builds the CLI for macOS aarch64, macOS x86_64, and Linux x86_64-gnu, each in a native runner for that OS/arch (tree-sitter's C dependencies need a matching C toolchain per target).

- Given the same tag push,
  when the macOS build leg runs,
  then it also runs `cargo tauri build --features app` to produce the unsigned `lazyspec.app`, matching ITERATION-257's local bundle command.

- Given all build legs succeed,
  when the release workflow's final job runs,
  then it creates a GitHub Release for the tag via `gh release create --generate-notes`, attaching the three CLI archives and the zipped `.app`.

- Given the release workflow fails on any target,
  when the failure occurs,
  then no GitHub Release is created (partial artifact sets are not published) — `release-plz`'s crates.io publish is a separate job from artifact building, so a crates.io publish can complete even if a later build leg fails; this asymmetry is accepted, not engineered around, since crates.io and GitHub Releases have never been forced into lockstep here.

## Scope

### In Scope

- `release-plz` configuration (`release-plz.toml` or workflow-level config) and its GitHub Actions workflow: conventional-commit-driven release PRs, tagging, crates.io publish via Trusted Publishing, `git_release_enable = false`.
- crates.io Trusted Publishing setup (OIDC trust relationship for this repo/workflow, no `CARGO_REGISTRY_TOKEN` secret).
- `.github/workflows/release.yml`: triggered on `v*` tag push, matrix build for macOS aarch64, macOS x86_64, Linux x86_64-gnu CLI binaries, macOS `.app` build via existing `cargo tauri build --features app`, archive naming, GitHub Release creation with `--generate-notes` and artifact attachment.

### Out of Scope

- Code signing or notarization of the macOS `.app` — deferred by RFC-054's ADR, not reopened here.
- Windows or Linux `.app`/Tauri bundling — RFC-054 scoped Tauri packaging to macOS only.
- musl/static Linux target — glibc target only for this story.
- A maintained `CHANGELOG.md` feeding GitHub Release notes — GitHub's auto-generated PR-title notes are the release notes; `release-plz`'s own changelog stays scoped to its release PR body.