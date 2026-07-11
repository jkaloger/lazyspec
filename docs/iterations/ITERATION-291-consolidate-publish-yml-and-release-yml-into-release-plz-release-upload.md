---
title: "Consolidate publish.yml and release.yml into release-plz release+upload"
type: iteration
status: complete
author: "jkaloger"
date: 2026-07-11
tags: []
related: []
---## Objective

Auto-build + attach release binaries by folding `release.yml` into `publish.yml` as a downstream job release-plz's own GH release triggers.

## Context

- Files: `.github/workflows/publish.yml`, `.github/workflows/release.yml`, `release-plz.toml`.
- release-plz action outputs: `releases_created` (`"true"`/`"false"`), `releases` (JSON `[{package_name, version, tag}]`).
- Nix toolchain pin (`$CARGO`/`$RUSTC`/`$RUSTDOC`) in `publish.yml` — keep, do not touch.
- Convention: `docs/convention` (`lazyspec convention`).

## Satisfies

Standalone fix. No parent story (iteration-only per owner). Root cause: release-plz pushes tag via `GITHUB_TOKEN` → GitHub recursion guard → `release.yml` `on: push tags` never fires → 0 auto GH releases. `workflow_dispatch`/`repository_dispatch` are guard-exempt but a same-workflow downstream job is simpler and needs no extra token/permission.

## Tasks

1. `release-plz.toml`: set `git_release_enable = true` (was false) → release-plz creates GH release + notes + tag.
2. `publish.yml` `publish-release`: give release-plz step an `id`; expose job `outputs.releases_created` + `outputs.releases` from that step.
3. `publish.yml`: add job `upload-assets`, `needs: publish-release`, `if: needs.publish-release.outputs.releases_created == 'true'`, `permissions: contents: write`. Matrix from `release.yml` (macos-14 aarch64 app, macos-13 x86_64 app, ubuntu-latest x86_64 CLI). It is a fresh job → replicate `publish.yml`'s setup: `nix-installer-action` + `cache-nix-action` (and toolchain pin for the `cargo tauri build` path). Steps: checkout tag `fromJSON(needs.publish-release.outputs.releases)[0].tag`, `nix build .#default` CLI + `nix develop --command cargo tauri build --features app` (matrix.app), archive, `gh release upload <tag> <artifacts>`.
4. Delete `release.yml` (its build matrix now lives in `upload-assets`; its `gh release create` is superseded by release-plz).
5. Update `README` if it documents the release flow.

## Out of scope

- Nix toolchain pinning logic (already correct).
- crates.io publish path (working).
- No PAT / GitHub App token — ephemeral `GITHUB_TOKEN` only.
- Retroactive v0.9.1 GH release (separate manual step if wanted).

## Principles / conventions

- Least privilege: `actions:write` NOT needed (no dispatch); `upload-assets` gets only `contents:write`.
- GH Actions: same-run job ordering via `needs` sidesteps token-recursion guard.
- CI/TUI/web parity: N/A (workflow-only change).

## Verification

- Merge a release PR → `publish-release` publishes crate + creates GH release; `upload-assets` runs, GH release for `v<version>` carries CLI tarball (3 targets) + macOS app zip (aarch64, x86_64).
- No-release push (e.g. docs) → `releases_created == "false"` → `upload-assets` skipped.
