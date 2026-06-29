---
title: End-to-end plugin validation
type: story
status: draft
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: RFC-051
---

## Story

As a maintainer, I want an end-to-end proof that the plugin installs and works from a clean project, so that the RFC's `source: "."` + `strict: false` assumption is verified before the integration is relied on.

Depends on S1 and S2 (both must exist to validate the full bundle).

## Scope

- Install the plugin into a scratch project via `/plugin marketplace add jkaloger/lazyspec` + `/plugin install lazyspec@lazyspec`.
- Confirm all ten skills resolve in the scratch project.
- Confirm the `UserPromptSubmit` convention hook fires on prompt submit (preamble injected) when the scratch project has a `.lazyspec.toml`.
- Confirm the loader picks up root `skills/` AND `hooks/hooks.json` under `source: "."` + `strict: false`.

## Out of scope

- Fixing any divergence between the embedded skill set (8) and on-disk set (10) — noted as a follow-up risk, not this story.

## Acceptance criteria

- Scratch project install succeeds with the two `/plugin` commands.
- All ten skills (`lazy`, `scaffold`, `co-write`, `generate`, `advance`, `execute`, `review`, `systematic-debugging`, `configure-type`, `create-audit`) are listed/usable after install.
- With a `.lazyspec.toml` present, a submitted prompt shows the convention preamble injected.
- The root-as-plugin config (`source: "."` + `strict: false`) is confirmed to load both `skills/` and `hooks/hooks.json` — this is the RFC's gating risk for acceptance.

