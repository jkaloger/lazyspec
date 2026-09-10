---
title: "update --assignee writes unquoted YAML, silently unloading the document"
type: bug
status: fixed
author: "Jack Kaloger"
date: 2026-09-10
tags: []
related: []
reviewed: 39b82c4e540035ce7009c18fef3a3f64e9eb4d86
---

## Expected

`lazyspec update <ID> --assignee "@jkaloger"` writes a frontmatter line that is valid YAML, so the document keeps loading in `list`, `show`, `tag` and the TUI.

## Actual

The value is written raw and unquoted:

```yaml
assignee: @jkaloger
```

`@` is a YAML reserved indicator, so the frontmatter no longer parses. The document silently disappears from `list`, and `show`/`tag` report `document not found` — the store drops docs it cannot parse rather than surfacing them. Only `validate` names the real error, and only if the caller thinks to run it.

## Repro

```bash
lazyspec create rfc "Repro" --json
lazyspec update RFC-001 --assignee "@jkaloger"
lazyspec show RFC-001     # Error: document not found: RFC-001
lazyspec validate         # parse error ... found character that cannot start any token at line 8 column 11
```

Confirmed on 0.11.3. `--title 'Plan: phase 2'` breaks the same way — any value carrying a YAML indicator character does.

## Root cause

`update_document_with_type` in `src/engine/fs_ops.rs` edits frontmatter as text lines, not as YAML. Both branches that write a reserved key interpolate the value verbatim:

- `src/engine/fs_ops.rs:376` / `:378` — the `INSERTED_WHEN_MISSING` branch (`assignee`, `reviewed`): `format!("{}: {}", key, value)`
- `src/engine/fs_ops.rs:388` — the `RESERVED_UPDATE_KEYS` branch (`status`, `title`, …): same

Custom attributes on the same path are safe: `src/engine/fs_ops.rs:350` routes their values through `serde_yaml::to_string`, which quotes when it must. Reserved keys skip that step.

This is the surviving instance of a bug already fixed elsewhere. `git_ref_store.rs:352` round-trips its whole frontmatter through `serde_yaml` for exactly this reason, citing AUDIT-018 C3: *"YAML-significant values (`Plan: phase 2`) come out properly quoted"*. The filesystem backend never got the same treatment.

## Fix

Emit reserved-key values through `serde_yaml` instead of `format!`, the way `coerced_attrs` already does — one scalar-serialising helper used by both branches. A whole-frontmatter round-trip (matching `git_ref_store`) would also work but reorders and reflows keys the line-editing path deliberately preserves.

Two callers, one helper. A regression test asserting `update --assignee "@x"` round-trips through `Store::load` covers the class.

## Also flagged, no action

`--body-file -` swallowing the newline after the closing `---` was reported alongside this. It does not reproduce on 0.11.3 — `create` and `update`, with and without `--assignee`, all emit the separator correctly. That was [[BUG-016]], fixed by `body_section` in 61c1d20. Whatever the reporter saw is either an older binary or a knock-on of the frontmatter damage above.
