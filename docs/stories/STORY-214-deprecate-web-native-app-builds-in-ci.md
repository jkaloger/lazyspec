---
title: "Deprecate web/native app builds in CI"
type: story
status: in-progress
author: "agent"
date: 2026-07-17
tags: []
related: []
---## Value

As a lazyspec maintainer, CI publishes no binary assets to GitHub releases — releases are immutable (no post-release `--clobber` uploads), and the failing tauri app build is gone. Install path is crates.io (release-plz publish).

## Acceptance Criteria

- AC1: `publish.yml` has no `upload-assets` job — no `cargo tauri build`, no CLI tarball uploads, no `gh release upload --clobber`.
- AC2: release-plz release + release-PR jobs unchanged; crates.io publish remains the artifact channel.
- AC3: README no longer advertises release binaries or the native app; notes deprecation.
- AC4: `app`/`web` cargo features and `src/app`, `src/web`, `src/bin/lazyspec-app.rs` stay in-tree (code deprecated, not deleted).

## Out of scope

Deleting the app/web source code or cargo features — separate decision.