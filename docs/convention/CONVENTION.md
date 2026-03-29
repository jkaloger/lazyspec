---
title: "Lazyspec Codebase Convention"
type: convention
status: accepted
author: "jack"
date: 2026-03-29
tags: []
---

Lazyspec is a Rust CLI/TUI tool for managing structured project documentation as version-controlled markdown. It's a single binary (engine + CLI + TUI) built for both human and agent consumption. The codebase values idiomatic Rust, clear module boundaries, and testability through trait-based abstractions. All CLI output supports `--json` for agent integration. The TUI is built on ratatui. Documentation is the product — the codebase dogfoods itself.
