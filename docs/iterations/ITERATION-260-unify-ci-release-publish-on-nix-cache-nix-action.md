---
title: Unify CI/release/publish on Nix + cache-nix-action
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-189
---

## Objective

Build all CI/release/publish jobs through the Nix flake (unified toolchain, no dtolnay), cache `/nix/store` via `cache-nix-action` (magic-nix-cache is sunset).

## Context

- Story: STORY-189 (extends release slice past AC6)
- Prior: ITERATION-258 (publish.yml release-plz), ITERATION-259 (release.yml matrix). This replaces their `dtolnay/rust-toolchain` + raw `cargo` with nix.
- Flake: `flake.nix` -- crane, `eachDefaultSystem`. `packages.default` = CLI binary. `checks.{fmt,clippy,test}`. devShell has `cargo-tauri` on darwin.
- Cache action: `nix-community/cache-nix-action` (wraps `actions/cache` over `/nix/store`, no external service/secret). Replaces `DeterminateSystems/magic-nix-cache-action` in `ci.yml`.
- Touch: `.github/workflows/ci.yml`, `release.yml`, `publish.yml`.
- Nix system != rust triple: `x86_64-linux`/`aarch64-darwin`/`x86_64-darwin` (nix) vs `*-unknown-linux-gnu`/`*-apple-darwin` (archive names). `nix build .#default` builds host system -- triple only used in archive filename, keep as-is.

## Satisfies

STORY-189 (toolchain-unification follow-up; no new external AC). Resolves security review finding: unpinned `dtolnay/rust-toolchain@stable` removed (nix supplies toolchain).

## Tasks

1. `ci.yml`: swap `magic-nix-cache-action` -> `nix-community/cache-nix-action` (SHA-pinned). Keep `nix build .#checks.x86_64-linux.<check>` matrix.
2. `release.yml` build job: drop `dtolnay/rust-toolchain` + `cargo build`. Add nix-installer + cache-nix-action. CLI via `nix build .#default`, copy `result/bin/lazyspec` into archive (keep archive name `lazyspec-<tag>-<triple>.tar.gz`).
3. `release.yml` macOS legs: `.app` via `nix develop --command cargo tauri build --features app` (toolchain from flake devShell, cargo-tauri already in darwin devShell -- drop `cargo install tauri-cli`). Keep zip naming.
4. `publish.yml`: drop `dtolnay/rust-toolchain`. Add nix-installer + cache-nix-action. Provide flake toolchain to PATH for `release-plz/action` (nix `nix profile install` or write to `$GITHUB_PATH`; `nix develop` subshell won't persist to the JS action step).

## Out of scope

- Full pure-nix Tauri `.app` package output -- rejected (hybrid decision); nix supplies toolchain only, `cargo tauri build` stays imperative inside devShell.
- Replacing `release-plz/action` with the nixpkgs `release-plz` binary -- KEEP the action: crates.io OIDC Trusted Publishing token exchange lives in the action, direct binary breaks it.
- Code signing / notarization -- deferred per RFC-054 ADR.
- Cross-compile toolchain -- native per-runner unchanged.

## Principles/conventions

CONVENTION.md layering -- N/A, no `src/` touched. SHA-pin all third-party actions (existing repo convention; cache-nix-action + nix-installer pinned).

## Verification

- `nix build .#default` on each runner OS produces `result/bin/lazyspec`.
- Test tag on fork: 3 CLI archives + 2 `.app` zips attach to one GH Release (parity with ITERATION-259).
- `publish.yml` dry run: release-plz finds cargo/rustc from nix in PATH, `release-pr` + `release` both run.
- Second CI run: cache-nix-action restores `/nix/store` (cache hit in logs).

