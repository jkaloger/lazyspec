---
title: Unsigned macOS app bundle via cargo tauri build
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-188
---## Objective

`cargo tauri build --features app` produces an unsigned `lazyspec.app` that renders a project through the in-process bridge, with every web asset resolving from bytes compiled into the binary — no external path.

## Satisfies

STORY-188 AC1 (bundle command emits `.app`), AC2 (launched bundle renders via bridge), AC3 (assets resolve from embedded bytes, verified from a moved-away `.app`), AC4 (Gatekeeper right-click-Open first run), AC5 (README states unsigned + steps + command), AC6 (non-`app` builds pull no bundler deps).

## Context

- Story + ACs: STORY-188 (`implements` RFC-054).
- Packaging design, unsigned/right-click-Open rationale, signing-deferred ADR: RFC-054 §Packaging and §"ADR: defer code signing / notarization".
- Bridge this bundle launches (dependency): ITERATION-252 — `app::run()` entry, `app` feature, custom-scheme handler over `web::server::router`.
- Assets already embedded at compile time (property to verify, not new plumbing): `src/web/assets.rs:10,21` — `include_str!`/`include_bytes!` from `CARGO_MANIFEST_DIR`. No `tauri.conf.json` asset-bundling entry is needed for CSS/fonts; the bytes are in the binary.
- Feature/bundle-dep wiring to extend behind `app`: `Cargo.toml` (`[features] app`, optional `tauri`/`tauri-build` deps from ITERATION-252).
- README app section to add/extend: `README.md`.

## Principles / conventions

- Convention doc (layering): `app -> web -> engine` (principle 3); config gated behind the `app` feature (principle 5) — mirrors ITERATION-252.
- No new transport or rendering path — this slice is packaging config + docs only.

## Tasks

1. Add Tauri bundle config so `cargo tauri build --features app` emits `lazyspec.app`: `tauri.conf.json` (bundle identifier, product name `lazyspec`, macOS target, `active = true`) and/or `[package.metadata.bundle]` in `Cargo.toml`, per RFC-054 §Packaging. Set signing identity to none (unsigned).
2. Keep all bundle config and any `tauri-build`/bundler deps behind the `app` feature; confirm `cargo build` (default) and `cargo build --features web` pull no bundler deps (AC6).
3. Verify the embedded-asset property: no `tauri.conf.json` `bundle.resources`/external asset dir is added for CSS/fonts — they load from `src/web/assets.rs` bytes. Guard against a regression that would introduce a runtime path (AC3).
4. Run the bundle command; confirm `lazyspec.app` at the documented output path (`target/release/bundle/macos/` or as configured) (AC1).
5. Add the README app section: the exact bundle command, an explicit "this build is unsigned" note, the right-click-Open first-run Gatekeeper steps, and the documented output path (AC5). Keep `CLAUDE.md`'s CLI/README-sync rule in mind.

## Out of scope

- Code signing / notarization — deferred (RFC-054 ADR); this ships unsigned.
- Windows/Linux packaging; DMG/installer; auto-update; distribution channel.
- The bridge, project picker, watcher (ITERATION-252 and STORY-186/187) — packaged, not built or changed here.
- Any new asset-embedding plumbing — assets are already `include_*!`'d; this slice only verifies and guards that.

## Verification

- `cargo tauri build --features app` emits `lazyspec.app`; copy it outside the source tree, launch it, and confirm CSS/fonts/htmx still load (AC3) and the RFC-052 list renders (AC2).
- First launch of the copied `.app` is blocked by Gatekeeper; right-click-Open per the README clears it; subsequent double-clicks open directly (AC4).
- `cargo build` and `cargo build --features web` succeed with no Tauri/bundler in the dependency tree (AC6).