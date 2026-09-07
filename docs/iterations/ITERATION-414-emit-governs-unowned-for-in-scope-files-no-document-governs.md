---
title: Emit governs-unowned for in-scope files no document governs
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-268
- blocks: ITERATION-415
---

## Objective

Files under `[governs] scope` that no document's glob matches become findings at the configured severity. Silent unless `unowned` is set.

## Satisfies

STORY-268 AC1, AC2, AC3, AC4, AC5.

## Context

- Story + ACs: STORY-268
- Variant, severity source, and why the default is off: RFC-068 §Design "Configuration" and "Validation" `GovernsUnowned`, §Decisions 4, §Risks "Day-one flood"
- `GovernsConfig` lands in ITERATION-407, compiled globs in ITERATION-408, object findings in ITERATION-411.
- Touch: `src/engine/validation.rs` -- variant, `Display`, `rule()` slug `governs-unowned`, rule struct. The rule needs `Config` to read severity; find how a rule reaches config today and follow it, do not thread a new parameter through the registry.
- `unowned: None` short-circuits before any filesystem walk. `scope: []` with `unowned` set emits nothing rather than walking the whole root -- the author narrowed it to nothing on purpose.
- Error severity exiting non-zero is the existing errors-versus-warnings split, not new logic (AC2).

## Tasks

1. Test-first: `scope = ["src/**"]`, `unowned = "warning"` -- one warning per unowned file, none for files a glob covers (AC1).
2. Test-first: `unowned = "error"` puts the same findings on the errors side and `validate` exits non-zero (AC2).
3. Test-first: no `unowned` key gives no findings whatever `scope` says (AC3); a narrowed `scope` leaves files outside it silent (AC4).
4. Add the variant, `Display` and slug; implement and register the rule.
5. Test-first: the `--json` finding carries `rule`, `file`, `message`, so `jq 'select(.rule=="governs-unowned") | .file'` lists the files (AC5).

## Out of scope

- Setting this repo's own `[governs]` config and seeding pins -- AC6, ITERATION-415. Turning it on before pins exist is the flood RFC-068 §Risks describes.
- A dedicated command for unowned files. RFC-068 §Non-goals.
- `governs-no-match` -- STORY-267.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-004: the walk goes through a `TempDir` fixture, not this repo.

## Verification

On a temp project, pin one module and leave a sibling module unpinned under the same `scope`: exactly the sibling's files are reported, and dropping `unowned` from the config silences them.
