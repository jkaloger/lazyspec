---
title: "Tech Stack"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [tech-stack, engine, cli, tui]
---

## Core

- **Rust 2021 edition**, single binary architecture — engine, CLI, and TUI share one crate
- **anyhow** for error handling — `anyhow::Result<T>` everywhere, no custom error enums unless callers need to match variants

## CLI

- **clap 4** with derive macros for CLI parsing — every command is a variant on the `Commands` enum
- **indicatif** for progress bars

## Serialization

- **serde** + **serde_yaml** for frontmatter, **toml** for config, **serde_json** for `--json` output

## TUI

- **ratatui** + **crossterm** — ratatui owns rendering, crossterm owns terminal events
- **crossbeam-channel** for event loop threading

## Parsing

- **tree-sitter** with language grammars (Rust, TypeScript) for symbol extraction in `@ref` directives
- **pulldown-cmark** for markdown parsing

## Other

- **sqids** for hash-based document numbering
- **tempfile** for test fixtures

## Dependency Policy

- When adding a dependency, prefer crates already in use. Don't introduce a new crate for something an existing dependency already handles
- Feature flags (`agent`, `metrics`) gate optional functionality — don't put feature-gated code behind runtime checks when compile-time gating works
