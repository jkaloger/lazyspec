---
title: GitHub milestones and issues should inherit remote counterpart's status lifecycle
type: bug
status: triaged
author: unknown
date: 2026-07-18
tags: []
related:
- related-to: BUG-007
---

## Summary

GitHub-backed docs (issues, milestones) carry lazyspec-local lifecycles disconnected from the remote's own state. An issue's open/closed (and issue-type/project state) and a milestone's open/closed should drive — or at least map into — the doc's lifecycle, instead of lazyspec tracking a parallel status that can disagree with the remote.

## Reproduction

1. github-issues-backed type; close the issue on GitHub.
2. Doc's lazyspec status unchanged (still draft/review/etc.).
3. Milestone likewise: closed milestone stays wherever its local status was.

## Expected

Remote counterpart's state is the source of truth for github-backed lifecycles: open/closed inherited on sync, transitions pushed back where lazyspec initiates them.

## Actual

Local status model derived at birth (see BUG-007) and maintained independently of remote state.

## Fix direction

Define a lifecycle mapping per github-backed type (remote state → lifecycle state), apply on sync both directions. Resolve alongside BUG-007 — one status-model rework.
