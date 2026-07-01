---
title: Unsigned macOS app bundle
type: story
status: in-progress
author: unknown
date: 2026-07-01
tags: []
related:
- implements: RFC-054
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context
<!-- guidance: the background and motivation; why this slice matters now -->

As an internal reviewer with no Rust toolchain, I want a double-clickable `lazyspec.app` I can open on macOS, so that I can read a lazyspec project without cloning the repo, installing cargo, or running `serve` from a terminal.

Everything RFC-054 builds — the in-process protocol bridge (story #1), project selection (#2), and live reload (#3) — only reaches its target audience once it ships as an artifact that audience can open. Until then the shell runs only from `cargo run --features app` on a developer machine. This story is the packaging step: it turns whatever the `app` feature currently produces into a distributable `.app` bundle and documents how a first-time user gets past Gatekeeper.

Two properties make this a thin slice rather than a big one. First, the web assets are already embedded at compile time — `src/web/assets.rs` pulls `static/lazyspec.css` via `include_str!` and fonts via `include_bytes!` — so "no external asset path at runtime" is a property to verify and guard, not new plumbing to build. Second, this story deliberately ships an **unsigned** bundle: code signing and notarization are explicitly deferred to a future story (RFC-054 non-goal and ADR). The value here is "openable at all", not "openable without friction".

This packages whatever exists. It depends on story #1 (the bridge) at minimum so that a launched bundle renders a project; #2 and #3 improve what the bundle contains but are not preconditions for producing one.

## Acceptance Criteria
<!-- guidance: one given/when/then per criterion; each independently verifiable -->

- **Given** the repository checked out on macOS with the Tauri build tooling available,
  **When** a maintainer runs the documented bundle command (`cargo tauri build`, or the configured `tauri-bundler` invocation),
  **Then** the build succeeds and emits a `lazyspec.app` bundle at a documented output path.

- **Given** the produced `lazyspec.app`,
  **When** a reviewer with no Rust toolchain and no checkout launches it,
  **Then** the app opens its window and renders a lazyspec project through the in-process bridge (story #1) — the RFC-052 list, document page, and relationship tree — with no terminal or browser.

- **Given** the running bundle rendering a project,
  **When** the webview requests the stylesheet, fonts, and htmx assets that `serve` ships,
  **Then** each asset is served from the bytes embedded in the binary at compile time, and no request reads a path outside the `.app` bundle (verifiable by launching a copy of the `.app` moved away from the source tree — assets still load).

- **Given** the unsigned `lazyspec.app` freshly downloaded or copied onto a machine,
  **When** the reviewer double-clicks it the first time,
  **Then** Gatekeeper blocks it — and following the README's first-run instruction (right-click the app, choose Open, confirm the prompt) launches it successfully; subsequent double-clicks open it directly.

- **Given** the README,
  **When** a reviewer reads the app section,
  **Then** it states the bundle is unsigned, gives the exact right-click-Open first-run steps to clear Gatekeeper, and names the command that produces the bundle.

- **Given** a build of the project without `--features app`,
  **When** the default and `web`-only builds are compiled,
  **Then** they succeed unchanged and pull in no Tauri or bundler dependencies (the bundle config lives behind the `app` feature).

## Scope

### In Scope
<!-- guidance: what this slice will deliver; keep it to one shippable increment -->

- Tauri bundle configuration (`tauri.conf.json` / `Cargo.toml` bundle metadata) sufficient for `cargo tauri build` — or the equivalent `tauri-bundler` invocation — to produce `lazyspec.app`.
- Confirming and, where needed, wiring the embedded web assets (CSS, fonts, htmx/JS from `src/web/assets.rs`) so the running bundle resolves every asset from bytes compiled into the binary — no runtime asset path.
- README section: the bundle command, an explicit "this build is unsigned" note, and the right-click-Open first-run steps for Gatekeeper.
- Keeping the bundle config and its dependencies behind the existing `app` cargo feature so non-`app` builds are untouched.

### Out of Scope
<!-- guidance: what is deliberately deferred, so reviewers know the boundary -->

- **Code signing and notarization.** Explicitly deferred to a future story (RFC-054 ADR). This story ships an unsigned bundle opened via right-click-Open.
- **Windows and Linux packaging.** macOS `.app` only.
- **Auto-update, DMG/installer packaging, or distribution channel.** Producing the `.app` is the deliverable; how it is delivered to reviewers is not.
- **The bridge, project picker, and watcher themselves** (RFC-054 stories #1–#3). This story packages them; it does not build or change them.
- **Bundling a doc snapshot into the app.** The store stays external (the chosen folder), per RFC-054.