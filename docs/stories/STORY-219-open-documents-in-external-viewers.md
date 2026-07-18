---
title: "Open documents in external viewers"
type: story
status: complete
author: "agent"
date: 2026-07-17
tags: []
related: []
---

## Value

As a lazyspec user, I can open the selected document in an external viewer — a terminal viewer like `glow` for local docs, the browser for remote-store docs (github-issues → the issue page, milestones → the milestone page, filesystem → the blob on the default branch).

## Context

- No open-in-external mechanism exists anywhere: no opener dep, no keybind, no CLI subcommand. Only external spawns are $EDITOR (`run_editor`, `src/tui/infra/event_loop.rs:49-65`) and agent commands.
- URL derivation already exists: `src/engine/github_url.rs` (`github_url` + `resolve_repo_coords`) resolves deep links for github-issues/milestones/filesystem; web view already consumes it (`src/web/routes.rs:364-379`). GitRef/ClickUp currently return `None`.

## Acceptance Criteria

- AC1: TUI keybind opens the selected doc externally: browser URL when `github_url` resolves one, else a configured viewer command (e.g. `[tui] viewer = \"glow\"`) on the doc's file, terminal suspend/resume mirroring `run_editor`.
- AC2: CLI parity: `lazyspec show <id> --open` (or equivalent) does the same resolution, with `--json` printing the resolved target instead of spawning.
- AC3: browser launch works cross-platform (macOS `open`, Linux `xdg-open`).
- AC4: stores with no web URL (git-ref, clickup for now) fall back to the viewer command; no panic, clear message when neither resolves.
- AC5: README documents keybind, config, and flag; keybind table updated.

## Out of scope

ClickUp/git-ref web URL derivation; embedded rendering inside the TUI.
