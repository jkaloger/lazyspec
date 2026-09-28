---
title: Adopt a workflow pack with one command
type: story
status: in-progress
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-074
reviewed: 4eb2881ee37a9f9cd3657e405169388670ee0e9d
---

## Value

As someone standing up a lazyspec project, I run `init --template <path-or-url>` and get a pack's config and templates, so an OpenSpec- or Autobots-shaped workflow is one command to adopt. Documents stay local; nothing is shared by reference.

## Acceptance Criteria

- AC1: `init --template <dir>` copies that directory's `.lazyspec.toml` and `.lazyspec/templates/` (flat and directory templates alike) into the current project. `--json` lists files written.
- AC2: `init --template <url>` clones into `.lazyspec/cache/config/` (the path `extends` already uses), then copies as AC1.
- AC3: Existing `.lazyspec.toml` → refuse with an error naming `--force`. `--force` overwrites.
- AC4: `--template starter` keeps working as a built-in name. `extends` is untouched; `--template` never sets `extends` or moves document storage. Passing `--template` skips the wizard, as `starter` does today.
- AC5: README documents pack layout (`.lazyspec.toml`, `.lazyspec/templates/`, `README.md`, optional plugin manifest for skills) and `init --template`.
- AC6: An `openspec` pack repo exists as first consumer: a `change` type with `change/{index,proposal,design,tasks}.md` directory template and a `delta` type for `create --parent`. `init --template <that url>` followed by `create change "x"` yields the four files.

## Out of scope

Skill distribution (plugin marketplace). Running the RFC-062 wizard on top of a seeded template. Any change to `extends`.
