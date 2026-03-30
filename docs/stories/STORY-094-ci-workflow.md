---
title: CI workflow
type: story
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-024
---


## Context

RFC-024 establishes a Nix-based CI pipeline for lazyspec using GitHub Actions. This story covers the workflow file itself, assuming the Nix flake already exists and passes its checks.

## Acceptance Criteria

- Given a pull request targeting `main`,
  when the PR is opened or updated,
  then the CI workflow triggers automatically.

- Given a push to `main`,
  when the push event fires,
  then the CI workflow triggers automatically.

- Given the CI workflow has triggered,
  when the Nix setup step runs,
  then `DeterminateSystems/nix-installer-action` provisions Nix on the runner.

- Given Nix is installed on the runner,
  when the cache step runs,
  then `DeterminateSystems/magic-nix-cache-action` provides transparent Nix store caching.

- Given the CI environment is ready,
  when the check job runs,
  then `nix flake check` executes clippy, tests, and formatting checks.

- Given the CI environment is ready,
  when the build job runs,
  then `nix build` produces the lazyspec package.

- Given the CI environment is ready,
  when the validate job runs,
  then `nix develop --command cargo run -- validate` dogfoods lazyspec against its own spec documents.

- Given all jobs in the workflow,
  when any job runs,
  then it uses `ubuntu-latest` as the runner image with no manual Rust toolchain setup.

## Scope

### In Scope

- `.github/workflows/ci.yml` workflow file
- Trigger configuration for PRs to `main` and pushes to `main`
- Nix provisioning via `DeterminateSystems/nix-installer-action`
- Nix store caching via `DeterminateSystems/magic-nix-cache-action`
- Three jobs: `nix flake check`, `nix build`, and validate (dogfooding)
- All jobs on `ubuntu-latest`

### Out of Scope

- The Nix flake itself (`flake.nix`, devShell, flake checks) -- covered by the "Nix flake" story
- Release workflows or cross-compilation
- PR comment posting or advanced dogfooding beyond `validate`
- Manual Rust toolchain configuration (e.g. `dtolnay/rust-toolchain`)
