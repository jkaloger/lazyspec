---
title: Workflow packs ship seed documents
type: rfc
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- related-to: RFC-074
- related-to: RFC-075
---

## Summary

Let a workflow pack ship seed documents. `init --template` copies seed paths the pack declares, only where the target does not exist, never overwriting. Local-dir and URL packs behave the same.

## Motivation

1. **Packs with content need a manual step.** `init --template` copies `.lazyspec.toml`, `.lazyspec/templates/` and `.lazyspec/hooks/` only (`install_pack`, `src/cli/init.rs:255`). A pack whose method includes seed documents (dictums, conventions) needs the adopter to copy them by hand.
2. **Local-dir packs leave nothing to read from.** A URL pack is cloned to `.lazyspec/cache/config/` (`resolve_pack_source`, `src/cli/init.rs:203`); a local dir is read in place (`src/cli/init.rs:211`) and leaves no cache. Non-template pack files are unreachable after init, so the two sources cannot differ in what they deliver.
3. **No place to declare it.** Config has no pack table (`Config`, `src/engine/config.rs:1120`).

## Goals

- A pack declares its seed paths in its own config.
- Init copies them without overwriting anything.
- Local-dir and URL sources produce identical results.
- Collisions with existing documents are reported, not resolved silently.

## Non-goals

- Copying every file in the pack. Rejected: a pack repo carries READMEs, CI and other files that are not method.
- Syncing seeds after init. Seeds are a starting point the adopter owns.
- Changing `extends`, which shares storage rather than config (RFC-074).

## Design

### Declaration

A top-level `[pack]` table in the pack's `.lazyspec.toml`:

```toml
[pack]
seed = ["docs/convention/", "docs/dictum/"]
```

Paths are relative to the pack root and must stay inside it. The table is inert in a non-pack project; `install_pack` is its only reader.

### Copy rules

For each seed file, in sorted order:

- Target path absent: copy.
- Target path present: skip, report `exists`.
- Target path absent but another document already holds the same ID: skip, report `id-collision` with the holder. IDs are never renumbered by default.

Seeds are copied after config, templates and hooks, so a seed copy failure leaves a usable project.

### Source parity

`install_pack` takes a resolved source dir today. Seeds read from that dir, so local-dir and URL packs need no extra staging.

### Output

`--json` lists `copied` and `skipped` (with reason) beside the existing written files. Text prints a one-line summary and each skip.

## Interfaces

- `[pack] seed` in `.lazyspec.toml` (@draft)
- `init --template` JSON gains `seeded` and `seed_skipped` (@draft)

## Open questions

- Re-init and upgrade: should a second `init --template` against an existing project add newly shipped seeds? Today `ensure_no_config` refuses without `--force`.
- Renumbering: should a colliding seed get a fresh ID instead of being skipped? Skipping loses content; renumbering breaks links inside the seed set.
- JSON shape: nest under one `seed` object, or flat sibling keys?

## Stories

Placeholder. Breakdown to follow once open questions settle: declare `[pack] seed`; copy without overwrite; ID-collision reporting; init output.

## Risks and tradeoffs

- Seeds copied once drift from the pack. Accepted: the adopter owns them.
- A skip-on-collision rule can leave a seed set half-installed. Reporting makes it visible; it does not fix it.
