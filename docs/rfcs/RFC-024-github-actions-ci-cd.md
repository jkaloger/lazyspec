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
- nix
- releases
related:
- related to: RFC-009
---


## Problem

There's no automated CI or release pipeline. Every PR merge is a trust exercise: tests run locally (if at all), clippy warnings go unchecked, and releases are manual `cargo build` invocations. This doesn't scale, and it means regressions can land silently.

The project is at v0.5.0 with an active development pace. Without CI, the feedback loop for catching breakage is "someone notices later."

Beyond CI, local development environments are not reproducible. Contributors need to manually install the right Rust toolchain, ensure a C compiler is available for tree-sitter, and hope their versions match. Nix solves both problems: a single `flake.nix` defines the build, the dev shell, and the CI checks.

## Intent

Establish a Nix flake as the single source of truth for building, developing, and testing lazyspec. Use that flake in GitHub Actions for CI. The same environment that passes locally passes in CI, with no drift between the two.

Three concerns: a Nix package build, a reproducible dev shell, and a GitHub Actions CI workflow that delegates to `nix flake check` and `nix build`.

Release workflows (cross-compilation, binary archives, GitHub Releases) are out of scope for now and will be addressed in a follow-up.

## Design

### 1. Nix Flake

A `flake.nix` at the repo root using `nixpkgs-unstable` and `oxalica/rust-overlay` for toolchain pinning.

The flake provides:

- `packages.default` -- the lazyspec binary, built with `rustPlatform.buildRustPackage`. The `rust-overlay` pins the Rust toolchain to a specific stable version so builds are reproducible regardless of what nixpkgs ships. Tree-sitter crates require C/C++ compilation, so `stdenv.cc` is included in `nativeBuildInputs`.

- `devShells.default` -- a development shell with the pinned Rust toolchain (cargo, clippy, rustfmt, rust-analyzer), plus any native dependencies needed for the build. Running `nix develop` drops you into a shell where `cargo build`, `cargo test`, and `cargo clippy` all work without any manual toolchain setup.

- `checks` -- flake checks that run clippy (with `-D warnings`), `cargo test`, and `cargo fmt --check`. These are invoked by `nix flake check` and form the basis of the CI pipeline.

```nix
# Sketch of flake.nix structure
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "clippy" "rustfmt" "rust-src" ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in {
        packages.default = rustPlatform.buildRustPackage {
          pname = "lazyspec";
          version = "..."; # read from Cargo.toml
          src = ./.;
          cargoHash = "sha256-...";
          nativeBuildInputs = [ pkgs.pkg-config ];
          # tree-sitter C deps handled by stdenv
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          packages = [ rustToolchain pkgs.rust-analyzer ];
        };

        checks = {
          clippy = /* cargo clippy -- -D warnings */;
          test = /* cargo test */;
          fmt = /* cargo fmt --check */;
        };
      });
}
```

> [!NOTE]
> `cargoHash` must be updated whenever `Cargo.lock` changes. This is the trade-off of `buildRustPackage` over crane/naersk -- simpler setup, but a manual hash update step. In practice this means: change a dependency, run `nix build`, copy the new hash from the error message.

### 2. CI Workflow

A single GitHub Actions workflow at `.github/workflows/ci.yml`. Runs on PRs targeting `main` and pushes to `main`.

```yaml
name: CI
on:
  pull_request:
    branches: [main]
  push:
    branches: [main]
```

The workflow uses `DeterminateSystems/nix-installer-action` for Nix setup and `DeterminateSystems/magic-nix-cache-action` for transparent caching of the Nix store between runs. This avoids rebuilding the entire dependency closure on every CI run.

Jobs:

| Job | Command | Purpose |
|-----|---------|---------|
| `check` | `nix flake check` | Runs all flake checks: clippy, tests, fmt |
| `build` | `nix build` | Verifies the package builds and produces a binary |
| `validate` | `nix develop --command cargo run -- validate` | Dogfood: run lazyspec validation on the project's own docs |

The `validate` job builds lazyspec from source inside the dev shell and runs it against the repo's `docs/` directory. If a PR ships broken specs, CI catches it.

All jobs run on `ubuntu-latest`. Nix handles the toolchain, so no `dtolnay/rust-toolchain` or manual compiler setup is needed.

### 3. Caching

`magic-nix-cache-action` uses GitHub Actions cache to store Nix store paths between runs. First builds pull the full closure; subsequent builds only rebuild what changed. Combined with `buildRustPackage`'s cargo vendor step, this keeps CI times reasonable.

### 4. Dogfooding

The `validate` job is the minimum dogfooding step. Future enhancements could include running `lazyspec status` and posting a summary comment on PRs, or diffing document counts before/after to surface what specs a PR adds or changes. These are not part of the initial scope.

### 5. Release Workflow (future)

Cross-compiled binary releases, GitHub Release creation, and versioning strategy are deferred. The Nix flake provides a foundation: `nix build` already produces a binary for the host platform. Extending to cross-compilation (via `pkgsCross` or a matrix of systems) is a natural follow-up but adds complexity that isn't needed yet.

## Stories

1. Nix flake -- `flake.nix` with `packages.default` (buildRustPackage + rust-overlay), `devShells.default`, and flake `checks` for clippy, test, and fmt. Includes `.envrc` for direnv integration.

2. CI workflow -- `.github/workflows/ci.yml` using DeterminateSystems Nix actions. Jobs: `nix flake check`, `nix build`, `nix develop --command cargo run -- validate`. Runs on PR and push to main.

3. Release workflow (future) -- `.github/workflows/release.yml` with cross-compilation, binary archives, and GitHub Release creation. Not part of initial scope.
