---
title: ClickUp comment adapter
type: story
status: rejected
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a reviewer working ClickUp-task-backed documents, I want native ClickUp task comments to appear as lazyspec comments — including reactions and tags — so that task discussion shows in the same thread view.

A thin adapter over the existing ClickUp store backend, pulled in by `lazyspec fetch`. Sibling to the GitHub-issues adapter, but ClickUp's native shape differs (tags, task-comment structure).

## Scope

- ClickUp comment adapter: a native task comment maps 1:1 to a `Comment`.
- Side-channel data projected onto the attribute map dynamically, no config: reactions → `attributes.reactions.*`, tags → `attributes.tags`.
- Adapter-sourced attributes bypass authored-path validation, surfaced as-is (read-only).
- Fetched during `lazyspec fetch` for clickup-typed documents.

Out of scope: GitHub (its own story); writing back to ClickUp (read-only mirror).

## Acceptance Criteria

- **Given** a ClickUp-backed document with native task comments, **when** I run `lazyspec fetch`, **then** each appears as a `Comment` in `comments <doc>`.
- **Given** a native comment with reactions, **then** they surface under `attributes.reactions.*` with no config declaring them.
- **Given** task tags, **then** they surface as `attributes.tags` verbatim.
- **Given** adapter-sourced attributes, **then** they are preserved (open map) and read-only.
- **Given** both a GitHub-issues and a ClickUp document in the same project, **then** each surfaces its native side-channel shape and both render identically in `comments --json`.

## Notes

Same open-map discipline as the GitHub adapter; only the native source shape differs. The adapter knows its source's shape and emits attributes on read.

