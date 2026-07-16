---
title: Remove release asset uploads and tauri build from publish workflow
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-214
---

## Objective

Delete `upload-assets` job from publish.yml — no binaries on GitHub releases (immutable), no tauri build. crates.io stays the artifact channel.

## Satisfies

STORY-214 AC1–AC4.

## Context

- Touch: `.github/workflows/publish.yml` — whole `upload-assets` job (:68-164): nix build, CLI tarball + `gh release upload --clobber`, `cargo tauri build --features app` + app zip
- Keep: `publish-release` (release-plz → crates.io) + `publish-pr` jobs; `publish-release` job `outputs` block only feeds `upload-assets` — drop it too
- Releases immutable: no post-release asset mutation

## Tasks

1. Delete `upload-assets` job; drop now-unused `outputs` from `publish-release`.
2. README: drop release-binary/native-app install mentions; deprecation note; crates.io install path. `app`/`web` features + `src/app`, `src/web`, `src/bin/lazyspec-app.rs` stay.
3. `actionlint`/yaml sanity check if available; else careful review.

## Out of scope

Deleting app/web code or cargo features. ci.yml (no app build there).

## Verification

publish.yml parses; `grep -riE 'tauri|release upload' .github/` → none.

