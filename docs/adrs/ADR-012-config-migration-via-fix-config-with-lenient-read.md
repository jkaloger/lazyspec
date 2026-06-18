---
title: Config migration via fix --config with lenient read
type: adr
status: accepted
author: jkaloger
date: 2026-06-18
tags:
- config
- migration
- cli
related:
- related-to: RFC-042
---

## Context

Strict load (ADR-011) breaks every existing project on upgrade: none declare `[[relationships]]`, and the 4 builtins were implicit. There is a chicken-and-egg: if every command hard-errors on missing config, no command can read the project to repair it. `init` refuses when `.lazyspec.toml` exists (`src/cli/init.rs:10`, scaffold-only); `fix` already exists to repair incomplete document frontmatter.

## Decision

Extend `fix` to repair config, one layer up from its existing remit. `fix` gains a `--config` flag that scopes it to config-only (skip documents) and uses a lenient config read that bypasses strict load, injecting the standard `[[relationships]]`/`[[rules]]` blocks. `init` stays fresh-scaffold-only. The strict-load error directs users to `fix`.

Rejected: making `init` merge-aware (overloads scaffold semantics with augment) and a dedicated `migrate` command (a third config-writing surface alongside `init` and `fix`).

## Consequences

- Single-command upgrade: `lazyspec fix --config` (preview with `--dry-run`).
- A lenient config read path now exists alongside strict load, used only by `fix`; mirrors how `fix` already tolerates broken frontmatter that normal load rejects.
- The standard migration blocks live in the `fix`/`init` template — the one sanctioned home for defaults under ADR-011.
