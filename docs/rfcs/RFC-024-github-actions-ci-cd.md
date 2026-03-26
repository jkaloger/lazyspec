---
title: GitHub Actions CI/CD
type: rfc
status: draft
author: jkaloger
date: 2026-03-15
tags:
- ci
- cd
- github-actions
- releases
related:
- related to: RFC-009
---


## Problem

There's no automated CI or release pipeline. Every PR merge is a trust exercise: tests run locally (if at all), clippy warnings go unchecked, and releases are manual `cargo build` invocations. This doesn't scale, and it means regressions can land silently.

The project is at v0.4.1 with an active development pace. Without CI, the feedback loop for catching breakage is "someone notices later."

## Intent

Establish GitHub Actions workflows for three concerns: continuous integration on PRs, automated binary releases on tags, and a versioning strategy that keeps the release process low-friction.

## Design

### 1. CI Workflow (on PR)

Runs on every pull request targeting `main` and on pushes to `main`.

```yaml
# .github/workflows/ci.yml
name: CI
on:
  pull_request:
    branches: [main]
  push:
    branches: [main]
```

**Jobs:**

| Job | Command | Purpose |
|-----|---------|---------|
| `check` | `cargo check --all-features` | Fast compilation check |
| `test` | `cargo test` | Run all tests |
| `clippy` | `cargo clippy -- -D warnings` | Lint, fail on warnings |
| `fmt` | `cargo fmt --check` | Formatting check |
| `validate` | `cargo run -- validate` | Dogfood: run lazyspec validation on the project's own docs |

The `validate` job is the interesting one. It installs the tool from source and runs it against the repo's `docs/` directory. If we ship broken specs in a PR, CI catches it.

Jobs run on `ubuntu-latest`. The Rust toolchain is pinned to stable via `dtolnay/rust-toolchain`.

Tree-sitter dependencies (`tree-sitter-typescript`, `tree-sitter-rust`) compile C/C++ code, so the CI image needs a C compiler. `ubuntu-latest` includes `gcc` by default, so no extra setup needed.

### 2. Release Workflow (on tag)

Triggered by pushing a version tag (`v*`).

```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags: ["v*"]
```

**Matrix build:**

| Target | Runner | Notes |
|--------|--------|-------|
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | Standard Linux |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` | Cross-compiled via `cross` |
| `x86_64-apple-darwin` | `macos-latest` | Intel Mac |
| `aarch64-apple-darwin` | `macos-latest` | Apple Silicon |
| `x86_64-pc-windows-msvc` | `windows-latest` | Windows |

Each target produces a compressed binary archive (`lazyspec-{version}-{target}.tar.gz` on unix, `.zip` on Windows).

The workflow:
1. Checks out the repo at the tag
2. Installs the Rust toolchain for the target
3. Builds with `cargo build --release` (or `cross build` for cross-compiled targets)
4. Strips the binary
5. Creates a compressed archive
6. Uploads artifacts

A final job collects all artifacts and creates a GitHub Release with release notes extracted from the tag annotation (or auto-generated from commits since last tag).

### 3. Versioning Strategy

Use **tag-based releases** with conventional commit messages. The flow:

1. Develop on feature branches, merge to `main`
2. When ready to release, update `Cargo.toml` version and tag: `git tag v0.5.0`
3. Push the tag: `git push origin v0.5.0`
4. Release workflow triggers automatically

> [!NOTE]
> `release-please` is an option for fully automated version bumps based on conventional commits. It's worth considering once the commit discipline is consistent enough. For now, manual tagging keeps things simple and explicit.

The release workflow extracts the version from the tag and verifies it matches `Cargo.toml`. Mismatch fails the build.

### 4. Caching

Rust builds are slow. The CI workflow uses `Swatinem/rust-cache` to cache the `target/` directory and cargo registry between runs. This cuts repeated CI runs from ~5min to ~1-2min.

### 5. Dogfooding

The `validate` job in CI is the minimum dogfooding step. Future enhancements could include:
- Running `lazyspec status` and posting a summary comment on PRs
- Checking that new RFCs/stories have required relationships
- Diffing document counts before/after to surface what specs a PR adds or changes

These are nice-to-haves and not part of the initial scope.

## Stories

1. **CI workflow** -- `.github/workflows/ci.yml` with check, test, clippy, fmt jobs. Rust toolchain setup, caching. Runs on PR and push to main.

2. **Release workflow** -- `.github/workflows/release.yml` with matrix cross-compilation. Binary archives, GitHub Release creation. Tag-version-Cargo.toml consistency check.

3. **Validate job and dogfooding** -- Add `lazyspec validate` as a CI job. Build from source, run against `docs/`. Fail on validation errors.
