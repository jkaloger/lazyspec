---
title: Shorthand ID resolution too strict on case and padding
type: bug
status: reported
author: unknown
date: 2026-09-08
tags: []
related:
- related-to: STORY-065
---

## Summary

Shorthand ID lookup only takes exact case and exact zero-padded width. `RFC-072` works. `rfc-072`, `rfc072`, `rfc72`, `rfc-72`, `RFC-72` all fail with "document not found", even though only one doc could ever match.

## Reproduction

1. `cargo run -- show RFC-072 --json` -> doc prints fine.
2. `cargo run -- show rfc-072 --json` -> `Error: document not found: rfc-072`.
3. `cargo run -- show RFC-72 --json` -> `Error: document not found: RFC-72` (right case, no padding).
4. `cargo run -- show rfc72 --json` -> `Error: document not found: rfc72` (no case, no hyphen, no padding).

## Expected

Any reasonable spelling of an unambiguous ID resolves: case-insensitive prefix, hyphen optional, numeric segment padding optional.

## Actual

`resolve_unqualified` and the parent-match arm of `resolve_shorthand` (src/engine/store.rs:284-371) do a raw `str::starts_with(id)` against `canonical_name(&d.path)`, the literal filename. No normalization runs on the input before the compare, so case and padding must match the file on disk exactly.

## Fix direction

Normalize both sides before matching in `resolve_unqualified` / `resolve_shorthand`: fold case, and pad the numeric tail up to the configured `[naming]` width (`{n:03}`) when the input's numeric segment is shorter. Sqids IDs (STORY-065) are already lowercase alphanumeric and un-padded, so padding only applies when the segment is all-digits.
