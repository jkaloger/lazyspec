---
title: GitHub-issues comment adapter
type: story
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a reviewer working GitHub-issue-backed documents, I want native GitHub issue comments to appear as lazyspec comments — including reactions and labels — so that the discussion on the issue is visible in the same thread view as everything else.

A thin adapter over the existing github-issues store backend, pulled in by `lazyspec fetch`.

## Scope

- GitHub-issues comment adapter: a native issue comment maps 1:1 to a `Comment` (envelope from the issue-comment metadata; body verbatim).
- Side-channel data projected onto the attribute map dynamically, with no config: reactions → `attributes.reactions.{+1,heart,…}`, labels → `attributes.labels`.
- Adapter-sourced attributes bypass authored-path schema validation and are surfaced as-is (read-only mirror of the remote source of truth).
- Fetched during `lazyspec fetch` for github-issues-typed documents.

Out of scope: ClickUp (its own story); writing reactions back (read-only); posting to GitHub (native issue-comment write is a separate concern if ever wanted).

## Acceptance Criteria

- **Given** a github-issues-backed document with native issue comments, **when** I run `lazyspec fetch`, **then** each native comment appears as a `Comment` in `comments <doc>`.
- **Given** a native comment with three 👍 reactions, **then** it surfaces `attributes.reactions.+1 = 3` with no config entry declaring `reactions`.
- **Given** issue labels, **then** they surface as `attributes.labels` verbatim.
- **Given** an adapter-sourced attribute not in the authored schema, **then** it is preserved (not rejected) — the open-map rule: unknown authored attributes are rejected, unknown adapter attributes are kept.
- **Given** adapter-sourced attributes, **then** they are read-only — lazyspec does not write reactions/labels back.

## Notes

Contrast with authored attributes: the schema constrains what may be authored, not what may appear. GitHub reactions/labels flow through verbatim.

