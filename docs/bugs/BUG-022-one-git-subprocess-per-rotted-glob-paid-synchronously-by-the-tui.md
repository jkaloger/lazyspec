---
title: One git subprocess per rotted glob, paid synchronously by the TUI
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-068
---

Found by the end-of-batch review on the RFC-068 governs batch (`cafb665..eb13d8b`).

`GovernsNoMatchRule::rename_candidates` (`src/engine/validation.rs:1093`) shells out to `git diff` once per rotted glob. Three dead globs on one document run the identical range three times.

The TUI pays this synchronously at `src/tui/state/app.rs:921`, so a document with several rotted pins stalls the interface for as many subprocess round trips.

Not blocking at this repo's pin count, and the per-glob shape is what made ITERATION-419 simple. Worth batching the range into one `git diff` and matching pairs against each glob in memory, or moving the call off the render path, if pin counts grow.
