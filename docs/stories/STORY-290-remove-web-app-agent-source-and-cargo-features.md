---
title: Remove web/app/agent source and cargo features
type: story
status: complete
author: Jack Kaloger
date: 2026-09-20
tags: []
related:
- implements: ADR-037
reviewed: 021d5970ba85a507c51decb414a929610fe7b411
---

## Value

As a lazyspec maintainer, the tree matches what ADR-037 decided — no `web`/`app`/`agent` cfg gates, no dead deps, no docs describing code that no longer exists.

## Acceptance Criteria

- AC1: `src/web/`, `src/app/`, `src/bin/lazyspec-app.rs`, `src/engine/agent.rs` deleted. Every `#[cfg(feature = "web"/"app"/"agent")]` site in `src/cli.rs`, `src/main.rs`, `src/tui/**` removed (code kept, gate dropped, or code deleted if the gate was its only reason to exist).
- AC2: Cargo.toml drops the `web`, `app`, `agent` feature entries and the tokio, axum, askama, tauri, tauri-plugin-dialog, tower, http, dirs, tauri-build dependency entries (incl. the tower/http dev-dependency entries whose only consumers were web-gated tests). `templates/`, `static/`, `icons/`, `tauri.conf.json` deleted; `build.rs` loses its `tauri_build::build()` call.
- AC3: `tests/integration/web_serve_test.rs`, `tui_agent_dialog_test.rs`, `tui_agent_management_test.rs` deleted; scattered gated blocks in `tests/integration/{main,surface_parity_test,tui_graph_test,tui_view_mode_test}.rs` removed.
- AC4: `gate.sh` no longer builds or tests `--features web`; header comment updated to match.
- AC5: README reflects the smaller CLI surface (no `lazyspec-app` binary, no web serve command). CHANGELOG gets an entry.
- AC6: `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings` all pass with the new (smaller) default feature set.
- AC7: Doc sweep — RFC-052, RFC-053, RFC-054, SPEC-019, ADR-014, ADR-015, ADR-016, ADR-017, ADR-018, STORY-005, STORY-051/052/053, STORY-132-136, STORY-182/183/184, STORY-185/186/188, and their iteration trees (ITERATION-045-048, 149, 181, 183-185, 200, 242-254, 257) moved to `superseded`, linked `superseded-by` ADR-037. Cross-cutting parity iterations (284, 296, 307, 322, 351, 364, 378, 416, 417, 445) get their web mention trimmed, not superseded.
- AC8: DICTUM-005 amended to drop `agent` as its feature-gate worked example.

## Out of scope

STORY-262 and STORY-285 — flagged as possibly agent-doc-adjacent, need a human read before deciding whether they're touched. Reservations (`engine/reservation.rs`, `cli/reservations.rs`) — confirmed no agent-id usage, stay untouched. tree-sitter, image/ratatui-image, reqwest, keyring — live deps, not part of this removal.
