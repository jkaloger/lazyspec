---
title: End-to-end plugin install validation
type: iteration
status: complete
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

## Verification

Static verification (performed; no push required):

- `plugin.json`, `marketplace.json`, `hooks/hooks.json` are valid JSON and match the Claude Code plugin/marketplace schema (checked against code.claude.com/docs/en/plugins-reference + plugin-marketplaces, 2026-06-29).
- Loader discovery simulated: `skills/*/SKILL.md` resolves exactly 10 skills (`advance co-write configure-type create-audit execute generate lazy review scaffold systematic-debugging`), each with matching `name:` frontmatter; loose `skills/README.md` and `skills/MIGRATION-2026-06-23.md` carry no `SKILL.md` and are ignored.
- Hook noop confirmed: in a directory without `.lazyspec.toml`, `lazyspec convention --preamble` exits 1 with empty stdout; the guarded form `... 2>/dev/null || true` exits 0 with empty stdout (no injection). In this repo (has `.lazyspec.toml`) the preamble is emitted on stdout (injection path).
- `hooks/claude-code-settings.json` (lease coordination) is a distinct filename, valid, and not auto-loaded; coexists.

Gating risk resolved: the RFC's `strict` value was inverted. Verbatim docs define `strict: false` as making the marketplace entry the entire definition (a co-located `plugin.json` declaring components then fails to load with a "conflicting manifests" error); `strict: true` (default) makes `plugin.json` the authority. Artifact and RFC corrected to `strict: true` + `source: "./"`.

Residual live step (manual, requires the changes pushed to `jkaloger/lazyspec`): a user runs `/plugin marketplace add jkaloger/lazyspec` then `/plugin install lazyspec@lazyspec` in Claude Code and confirms the 10 skills load and the preamble fires. The two `/plugin` slash commands execute in the Claude Code client and fetch the remote repo, so they cannot be exercised from the build environment against uncommitted local files.

