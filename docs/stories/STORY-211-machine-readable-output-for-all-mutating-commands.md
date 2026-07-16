---
title: Machine-readable output for all mutating commands
type: story
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- related-to: AUDIT-018
---

## Value

As an agent driving lazyspec, every mutating command gives me machine-readable output (CONVENTION principle 2).

## Acceptance Criteria

- AC1: `delete`, `link`, `unlink`, `ignore`, `unignore` each accept `--json` and emit a structured outcome (doc id/path, action, targets), mirroring the `tag` command pattern.
- AC2: Human output without the flag is unchanged.
- AC3: README/help updated where these flags are documented.

## Out of scope

`setup` and `skills` JSON support (AUDIT-018 F4 tail) — interactive flows, separate design.

