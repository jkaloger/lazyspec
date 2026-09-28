---
title: Ship hooks in a workflow pack
type: story
status: draft
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-075
---

## Value

As a pack author, I ship hook scripts with my pack, so adopting the pack brings its checks and automation, not just its templates.

## Acceptance Criteria

- AC1: `init --template` copies the pack's `.lazyspec/hooks/` along with `.lazyspec.toml` and its templates. `--json` lists the copied files.
- AC2: Adopted hooks start untrusted (STORY-295). `init` prints the `hook trust` command to run.
- AC3: The README documents `[[hooks]]`, both events, the stdin/stdout protocol, trust, and `.lazyspec/hooks/` in a pack.
