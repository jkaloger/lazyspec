---
title: "context --json emits target as a path string, not a record"
type: bug
status: reported
author: "Jack Kaloger"
date: 2026-09-28
tags: []
related: []
---

## Expected

`context <id> --json` returns the anchor document as the same record shape as `chain`, `forward`, and `related` entries, so a consumer reads `.target.id` and `.chain[].id` the same way.

## Actual

```
$ lazyspec context CHG-001 --json | jq -c '{target: (.target|type), chain: (.chain|map(type))}'
{"target":"string","chain":["object"]}
$ lazyspec context CHG-001 --json | jq '.target'
"openspec/changes/CHG-001-lifestyle-filter/index.md"
```

Every jq over the context shape has to special-case the target.

## Cause

`src/cli/context.rs:56` serialises `resolved.target.path.to_string_lossy()` under `"target"`. The tree renderer only needs the path to stamp the "you are here" marker, and the JSON emitter reused that.

## Fix

Emit the target's full `DocMeta` record under `target`, the same serialisation the other three arrays use. RFC-070's `--pack` adds `tier`, `band`, `governs`, `body` to every record; fix the shape before that lands so pack consumers never see the string form.
