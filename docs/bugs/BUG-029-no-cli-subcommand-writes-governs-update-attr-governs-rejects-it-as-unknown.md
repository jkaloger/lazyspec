---
title: No CLI subcommand writes governs; update --attr governs rejects it as unknown
type: bug
status: fixed
author: Jack Kaloger
date: 2026-09-14
tags: []
related:
- related-to: RFC-068
reviewed: 1d003a74de918cb9c35468fa07e74d3ec874683c
---

## Expected

`governs` is a first-class frontmatter field (RFC-068). Some CLI subcommand writes it, so an agent or human can pin a document to code without hand-editing YAML. At minimum `update --attr governs=…` either works or names the command that does.

## Actual

No subcommand writes `governs`. Every read path knows the field: `why`, `show`, `validate`, TUI preview header, web render. The only write is `fix --governs`, which rewrites an *existing* zero-match glob to its suggestion. Nothing creates the first glob.

`update --attr governs=…` is rejected as an unknown attribute:

```bash
lazyspec create rfc "Gov test" --json
lazyspec update RFC-001 --attr 'governs=src/**' --json
# Error: unknown attribute 'governs' for type 'rfc'
# exit 1
```

Confirmed on 0.12.1. Same for `reviewed` sibling, but that one is deliberate (RFC-069 forbids forging a review). `governs` has no such reason.

## Root cause

Two lists gate what `update` may write, and `governs` is in neither.

- `src/cli/update.rs:5` — `RESERVED_ATTR_KEYS` names status, title, body, author, reviewed. Anything else passes through as a custom attribute.
- `src/engine/fs_ops.rs:300` — `RESERVED_UPDATE_KEYS` partitions updates. Reserved keys take the in-place line edit. Everything else goes to `apply_attrs`.
- `src/engine/document.rs:266` — `apply_attrs` looks the key up in `type_def.attributes`. `governs` is a struct field on `DocMeta` (line 326), never a declared attribute, so lookup fails and it bails as unknown.

So the field is parsed first-class on the way in (`RawFrontmatter.governs`, line 355) and serialised first-class on the way out (cache round-trip test, line 896), but the mutation seam only knows scalars from the type schema. `governs` is a list, so even adding it to the reserved set is not enough: `reserved_line` writes one scalar.

Not a regression. RFC-068 line 54 specified the field and said "a module rename is one glob edit", assuming hand-editing. A write path was never scoped. Gap, not breakage.

## Fix

`governs` is a list field like `tags` and `provenance`. Both already have subcommands: `tag add/remove`, `provenance add/remove/list`. Same shape fits here.

Options, in order of preference:

1. **`lazyspec govern add|remove|list <ID> <GLOB>…`** — mirrors `tag`. Validates each glob through `compile_governs` before writing so a bad glob is refused, not stored. Filesystem path edits the `governs:` block in place; git-ref path round-trips frontmatter as it does today.
2. **`update --governs <GLOB>` repeatable, replaces the whole list** — smaller, but a full replace is a footgun for multi-glob docs and does not match how the other list fields are edited.

Either way `update --attr governs=…` should join `RESERVED_ATTR_KEYS` with a message pointing at the real command, the way `reviewed` points at `pin`.

TUI and web have no governs editor either. Per project rule, the engine op lands first and CLI/TUI/web each grow the feature. This card covers CLI and the engine seam. TUI/web editing is a follow-on story.

## Also flagged, no action

`create` has no `--governs` flag. Not needed once `govern add` exists; `create` then `govern add` is two calls, same as tags today.
