---
title: Open-target resolution and show --open
type: iteration
status: complete
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-219
- blocks: ITERATION-306
---

## Objective

Engine open-target resolution + `show --open`. URL for web-backed docs, file path otherwise.

## Satisfies

STORY-219 AC2, AC3 (CLI half), AC4.

## Context

- URL infra exists: `src/engine/github_url.rs:121-166` (`github_url`), `:70-93` (`resolve_repo_coords`); web view consumer `src/web/routes.rs:364-379`
- GitRef/ClickUp → `None` (`github_url.rs:164`)
- No opener dep yet

## Tasks

1. `engine::ops`: `resolve_open_target(doc) -> OpenTarget { Url(String) | File(PathBuf) }` — `github_url` first, else doc path.
2. `show --open`: spawn browser on Url (macOS `open`, Linux `xdg-open`), viewer cmd from `[tui] viewer` config on File; no viewer configured + no URL → clear error.
3. `show --open --json`: print resolved target, no spawn.
4. Config: `viewer` field in `UiConfig`, round-trip, `config --json`.
5. README: flag + config. Tests: resolution per backend, json output. `cargo test`.

## Out of scope

TUI keybind (next iteration). ClickUp/git-ref URLs.

## Verification

`cargo test`. Manual: `show STORY-X --open` (fs doc → blob URL or viewer), `--json` prints target.

