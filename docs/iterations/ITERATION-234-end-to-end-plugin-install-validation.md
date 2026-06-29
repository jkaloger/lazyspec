---
title: End-to-end plugin install validation
type: iteration
status: draft
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: STORY-175
---

## Goal

E2E proof. Plugin installs + works from clean project. Validates `source:"."` + `strict:false` before RFC relied on.

## Tasks

- Scratch project. `/plugin marketplace add jkaloger/lazyspec` → `/plugin install lazyspec@lazyspec`.
- Confirm 10 skills resolve (`lazy scaffold co-write generate advance execute review systematic-debugging configure-type create-audit`).
- Add `.lazyspec.toml` to scratch. Submit prompt → confirm preamble injected (hook fires).
- Confirm loader picks up BOTH root `skills/` AND `hooks/hooks.json` under `source:"."` + `strict:false`. ← RFC gating risk.

## Acceptance criteria

- Scratch install succeeds (2 cmds).
- 10 skills usable post-install.
- With `.lazyspec.toml` → preamble injected on prompt submit.
- Root-as-plugin config loads skills/ + hooks/hooks.json. (Confirms RFC acceptance risk.)

## Out of scope

- Embedded-set (8) vs on-disk (10) reconciliation. Follow-up risk.

## Depends

STORY-172, STORY-173.

