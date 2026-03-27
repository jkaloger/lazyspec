---
title: Nix flake
type: story
status: draft
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-024
---


## Context

RFC-024 establishes a Nix flake as the single source of truth for building, developing, and testing lazyspec. This story covers the flake itself: packages, dev shell, checks, and direnv integration. The CI workflow that consumes this flake is a separate story.

## Acceptance Criteria

- Given a clean checkout of the repo,
  when a developer runs `nix build`,
  then `packages.default` builds lazyspec via `rustPlatform.buildRustPackage` using the pinned Rust stable toolchain from rust-overlay, and tree-sitter C/C++ dependencies compile without error.

- Given a clean checkout of the repo,
  when a developer runs `nix develop`,
  then `devShells.default` provides cargo, clippy, rustfmt, and rust-analyzer on `$PATH`.

- Given a clean checkout of the repo with direnv installed,
  when a developer enters the repo directory,
  then `.envrc` (`use flake`) activates the dev shell automatically.

- Given a clean checkout of the repo,
  when a developer runs `nix flake check`,
  then checks execute clippy with `-D warnings`, `cargo test`, and `cargo fmt --check`, and all three must pass for the check to succeed.

- Given a change to `Cargo.lock`,
  when `cargoHash` in `flake.nix` is not updated to match,
  then `nix build` fails with a hash mismatch, prompting the developer to update it.

## Scope

### In Scope

- `flake.nix` at repo root using `nixpkgs-unstable` and `oxalica/rust-overlay`
- `packages.default` via `rustPlatform.buildRustPackage` with pinned Rust stable toolchain
- `devShells.default` with cargo, clippy, rustfmt, rust-analyzer
- Flake `checks` for clippy (`-D warnings`), `cargo test`, `cargo fmt --check`
- `nativeBuildInputs` for tree-sitter C/C++ compilation (`stdenv.cc`)
- `.envrc` with `use flake` for direnv integration
- Pinned Rust stable toolchain via `rust-overlay`

### Out of Scope

- GitHub Actions CI workflow (covered by the "CI workflow" story)
- Release workflow and cross-compilation
- Any `.github/` configuration files
