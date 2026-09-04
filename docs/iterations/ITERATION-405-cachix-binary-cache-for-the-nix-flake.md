---
title: Cachix binary cache for the Nix flake
type: iteration
status: complete
author: unknown
date: 2026-09-04
tags: []
related:
- implements: STORY-189
- related-to: STORY-119
---

## Objective

Nix consumers get prebuilt store paths, not a cold crane build.

## Satisfies

STORY-189: release pipeline publishes per-tag artefacts; the Cachix push is one more artefact channel. Touches STORY-119's `flake.nix` (related-to).

## Context

- `publish.yml`: `publish-release` and `publish-pr` install nix (`DeterminateSystems/nix-installer-action@c5a866b6ab867e88becbed4467b93592bce69f8a # v21`) and use `nix develop` for the toolchain only. Nothing runs `nix build .#default`. `build-binaries` is a plain cargo matrix.
- `flake.nix`: crane `packages.default`; no `nixConfig`.
- Cache: cachix.org public cache `lazyspec`. Repo secret `CACHIX_AUTH_TOKEN`. Public key: `lazyspec.cachix.org-1:vPXwfgzSiLee3OEYP+a9Y/3Xlwpzs6WnpLuQjQZlvZ8=`
- Cache hit needs identical derivation: consumers must not override `nixpkgs.follows`.

## Tasks

1. `publish.yml`: `nix-cache` job. `needs: publish-release`, gated on `releases_created == 'true'`. Matrix `ubuntu-latest` + `macos-latest`. Checkout tag with `persist-credentials: false`, nix-installer-action at the existing v21 sha, `cachix/cachix-action` pinned by sha (name `lazyspec`, `authToken: ${{ secrets.CACHIX_AUTH_TOKEN }}`), `nix build .#default`.
2. `flake.nix`: `nixConfig.extra-substituters = ["https://lazyspec.cachix.org"]`, `extra-trusted-public-keys` with the key above. Real key only; never a placeholder (malformed key errors for every consumer who accepts `nixConfig`).
3. README Nix section: cache exists; accept `nixConfig` prompt or add substituter + key to `nix.conf`; flake-input snippet pinned to a tag; `nixpkgs.follows` on the lazyspec input breaks cache hits.

## Out of scope

x86_64-darwin cache (macos-13 runner). Caching `ci.yml` builds. Replacing `build-binaries`.

## Verification

- `actionlint` or yaml parse on `publish.yml`
- `nix flake check` passes
- After next release: `nix build github:jkaloger/lazyspec/vX.Y.Z` on a clean machine substitutes from cache
