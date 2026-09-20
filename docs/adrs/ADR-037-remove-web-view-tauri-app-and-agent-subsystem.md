---
title: Remove web view, Tauri app, and agent subsystem
type: adr
status: accepted
author: Jack Kaloger
date: 2026-09-20
tags: []
related:
- related-to: STORY-214
- supersedes: RFC-052
- supersedes: RFC-053
- supersedes: RFC-054
- supersedes: SPEC-019
- supersedes: ADR-014
- supersedes: ADR-015
- supersedes: ADR-016
- supersedes: ADR-017
- supersedes: ADR-018
- supersedes: STORY-005
- supersedes: STORY-052
- supersedes: STORY-132
- supersedes: STORY-133
- supersedes: STORY-134
- supersedes: STORY-135
- supersedes: STORY-136
- supersedes: STORY-182
- supersedes: STORY-183
- supersedes: STORY-184
- supersedes: STORY-185
- supersedes: STORY-186
- supersedes: STORY-188
- supersedes: ITERATION-045
- supersedes: ITERATION-046
- supersedes: ITERATION-047
- supersedes: ITERATION-181
- supersedes: ITERATION-182
- supersedes: ITERATION-183
- supersedes: ITERATION-184
- supersedes: ITERATION-185
- supersedes: ITERATION-242
- supersedes: ITERATION-243
- supersedes: ITERATION-244
- supersedes: ITERATION-245
- supersedes: ITERATION-246
- supersedes: ITERATION-247
- supersedes: ITERATION-248
- supersedes: ITERATION-249
- supersedes: ITERATION-250
- supersedes: ITERATION-251
- supersedes: ITERATION-252
- supersedes: ITERATION-253
- supersedes: ITERATION-254
- supersedes: ITERATION-257
reviewed: 021d5970ba85a507c51decb414a929610fe7b411
---

## Context

STORY-214 already pulled web/app out of CI and release. Source and cargo features stayed in tree — STORY-214 called that out as a separate decision, deferred, not forgotten.

Nothing reaches `web`, `app`, or `agent` code by default. `cargo check --no-default-features` builds clean today — proof the gating already isolates them. Web view (askama + axum + tokio) drew a Tauri app on top (`tauri`, `tauri-plugin-dialog`, `tower`, `http`, `dirs`, `tauri-build`); neither shipped since STORY-214. Agent subsystem (`engine/agent.rs`, TUI agent dialogs) sits behind its own flag, unused, described only by draft docs (SPEC-019, ADR-015/016/018) that were never finished because the feature never shipped either.

DICTUM-005 names `agent` as its worked example of a feature gate. Once the flag goes, the example points at nothing.

## Decision

Remove `src/web/`, `src/app/`, `src/bin/lazyspec-app.rs`, `src/engine/agent.rs`, every `web`/`app`/`agent` cfg-gated call site in the TUI and CLI, and the matching Cargo.toml feature/dependency entries (tokio, axum, askama, tauri, tauri-plugin-dialog, tower, http, dirs, tauri-build). Remove the assets only those subsystems used: `templates/`, `static/`, `icons/`, `tauri.conf.json`, and the `tauri_build` call in `build.rs`. Amend DICTUM-005 to drop `agent` as its feature-gate example. This closes the item STORY-214 deferred.

## Consequences

Smaller dependency tree, faster build, one less surface (web view) that TUI/CLI/web parity work had to account for. The draft doc family this code backed — SPEC-019, ADR-015/016/018, RFC-052/053/054, and the web/app story+iteration trees — stops describing code that exists and moves to superseded instead of rotting as accepted-but-false.

Real cost: `lazyspec skills` ships by default today through the app-adjacent path. Once `app` is gone, a bare `cargo install lazyspec` can no longer install skills — they ship only via the plugin going forward. Unlike the rest of this removal, that subcommand currently works for every installer; this is a genuine capability loss, not dead-code cleanup.

## Revisit when

- A read-only doc web view or native app becomes an actual requirement again, not a speculative one — write a fresh RFC, don't resurrect this code, it will have drifted from the current engine.
- Agent-assisted doc editing moves past draft spec into something teams actually want — same: new RFC/ADR, this implementation is gone and its assumptions (askama templates, the old CLI surface) won't hold.

## Amendments

- **2026-09-20** — The Consequences section's claimed regression ("a bare `cargo install lazyspec` can no longer install skills") does not hold: `lazyspec skills` and its embedded `skills/*/SKILL.md` content were never gated by `web`/`app`/`agent`, and `cargo package --list` confirms those files ship in the published crate regardless. No capability loss on that front from this removal. (STORY-290)
