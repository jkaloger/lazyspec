---
title: Single Binary Architecture
type: adr
status: accepted
author: jkaloger
date: 2026-03-04
tags:
- architecture
related:
- related-to: RFC-001
---


## Context

lazyspec needs both a CLI for scripting/agent use and a TUI for interactive browsing. These could be separate binaries, a library with separate frontends, or a single binary.

## Decision

Single binary with shared engine. No arguments launches the TUI. Subcommands run CLI operations.

The engine module contains the document model, store, config parsing, and template rendering. CLI and TUI modules are thin consumers that compose engine primitives. This means the store, document model, and validation logic are written once and shared.

## Consequences

- One `cargo install` gets both interfaces
- Engine changes automatically apply to both CLI and TUI
- Binary size is larger than it would be with separate binaries (includes ratatui even when running CLI commands)
- Testing is simpler since engine logic is tested independently of either interface
