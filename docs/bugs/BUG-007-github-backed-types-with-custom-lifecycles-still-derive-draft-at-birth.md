---
title: GitHub-backed types with custom lifecycles still derive draft at birth
type: bug
status: triaged
author: unknown
date: 2026-07-18
tags: []
related:
- related-to: BUG-002
---

## Summary

BUG-002/ITERATION-312 fixed `create` to seed the type's first lifecycle state for filesystem and git-ref stores. GitHub-backed types with custom lifecycles still derive `draft` at birth — the status model for github-backed docs predates per-type lifecycles.

## Reproduction

1. github-issues-backed type with lifecycle starting at e.g. `reported`.
2. Create a doc of that type.
3. Doc surfaces as `draft` — a state outside its lifecycle.

## Expected

GitHub-backed docs are born at their type's first lifecycle state, same as filesystem/git-ref after ITERATION-312.

## Actual

Status derivation for github-backed docs hardcodes `draft` (e.g. src/engine/github_url.rs:183) instead of consulting the type's lifecycle.

## Fix direction

Route github-backed status derivation through the same first-lifecycle-state seeding as other stores. Overlaps BUG-008 (inheriting remote lifecycle) — resolve the status model once for both.
