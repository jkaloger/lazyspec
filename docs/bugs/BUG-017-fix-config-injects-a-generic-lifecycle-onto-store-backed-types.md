---
title: "fix --config injects a generic lifecycle onto store-backed types"
type: bug
status: reported
author: "Jack Kaloger"
date: 2026-09-02
tags: []
related: []
---

## What happens

`fix --config` on this repository's `.lazyspec.toml` also appends the generic `draft ... superseded` lifecycle onto the `milestone` and `clickup` types, which declared none. Neither type is part of the edge migration.

## Why it matters

A declared lifecycle beats the store's canonical one (STORY-224), so a `milestone` carrying the generic lifecycle would reject its real statuses `open` and `closed`.

## How it was found

ITERATION-380 migrated the repo's own config and diffed the result. The two injected lifecycle blocks change no validation output on this repository -- `validate` is byte-identical with and without them -- so they were dropped from that commit by hand. The append itself is the defect.

## Where to look

`src/engine/ops/fix/config.rs` -- the append half of `collect_config_fixes`, which tops a config up with missing standard blocks. It should not be topping up a lifecycle for a type whose statuses come from a store backend.
