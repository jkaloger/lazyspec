---
title: "Dependency Policy"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [tech-stack, engine, cli, tui]
---

- When adding a dependency, prefer crates already in use. Don't introduce a new crate for something an existing dependency already handles
- Feature flags (`agent`, `metrics`) gate optional functionality. Use compile-time gating (`#[cfg(feature = "...")]`), not runtime checks
- `Cargo.toml` is the authoritative dependency inventory. This dictum governs policy for changing it, not a restated bill of materials
