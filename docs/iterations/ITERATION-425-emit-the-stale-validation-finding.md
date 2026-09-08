---
title: Emit the stale validation finding
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-273
- blocks: ITERATION-426
---

## Objective

`validate` emits a `stale` finding per stale-band document, at `[staleness].finding` severity.

## Satisfies

STORY-273 AC1, AC3, AC4, AC6 -- at rule level, through `validate_full`. AC2's severity mapping is decided here; its exit code and every surface land in the next slice.

## Decision: off is a value, not an absence

RFC-069 §Design "Configuration" writes `finding = "warning"   # warning | error; omit to disable (default warning)`. Self-contradictory: absence cannot mean off and warning at once. AC3 spends absence on warning. So off needs its own spelling.

Chosen:

```toml
[staleness]
finding = "off"   # off | warning | error. Default warning
```

Why: default stays on -- rot is reported unless someone opts out (AC3) -- and opting out becomes a line written on purpose, greppable in review, not a key nobody notices missing. `[governs] unowned` (`config.rs:1257`) can spell off as absence because its default is off; staleness defaults to on, so the two knobs cannot share one representation.

Shape: new `StalenessFinding { Off, Warning, Error }`, `#[serde(rename_all = "lowercase")]`, `#[default] Warning`. `StalenessDriver` (`config.rs:1338`) is the template, down to an unknown value being a config error. `fn severity(&self) -> Option<Severity>` maps `Off -> None`. RFC-069 §Interfaces' `finding: Option<Severity>` stays true: that is what the rule reads, not what the TOML holds.

## Context

- Story + ACs: STORY-273. Variant and config key: RFC-069 §Interfaces. Band and `compute`: §Design "Computation", shipped by STORY-272.
- Touch:
  - `src/engine/config.rs:1308` `StalenessConfig` -- one more field. No writer work: `config_write.rs` writes `[[types]]` and `[[edges]]` rows only, never a global table, so `[staleness]` is parse, default, `JsonSchema` derive.
  - `src/engine/validation.rs:33` `ValidationIssue` -- `Stale { path, staleness }`. Slug at `:159`, `Display` at `:242`, a sample beside `:3581` or `every_variant_has_a_sample` fails.
  - `src/engine/validation.rs:1130` `GovernsUnownedRule` is the gate pattern whole: `let Some(severity) = ... else { return Vec::new() };` before any walk. `GovernsNoMatchRule::new(Box::new(GitCli))` (`:1540`) is the pattern for a rule that needs git, since `Checker::check` (`:475`) is handed store and config and nothing else.
  - `src/cli/show.rs:113` `staleness_line` -- the parenthetical the message reuses.
- Message: `docs/specs/SPEC-001-store.md is stale (drift, 12 files since 0123456, 140d)`. `GovernsUnowned` sets the `{path} is ...` shape; the parenthetical is `show`'s line verbatim. So `Display for Staleness` goes on the type in `src/engine/staleness.rs` and `staleness_line` becomes `format!("staleness: {staleness}")`. One wording, two surfaces, no second copy to drift.
- Skip `validate_ignore` documents, as every document rule does (`validation.rs:500`, `:1098`).
- Sort findings by path. `store.docs` is a map and finding order must not be its iteration order -- `:1094` says the same about globs.
- Only `Band::Stale` emits. `aging` is a fact `show` reports, never a finding (AC1, AC6).

## Tasks

1. Test-first: absent `finding` -> `Warning` (AC3); `"error"` -> `Error`; `"off"` -> `Off` and `severity()` is `None`; `"loud"` -> config error, not a panic.
2. Add `StalenessFinding` and the field.
3. `Display for Staleness`; `staleness_line` reads it. Its tests (`show.rs:506`) stay green unedited.
4. `ValidationIssue::Stale`: variant, slug `"stale"`, `Display`, sample.
5. Test-first, `StaleRule` over the mock git: a stale document gives one finding carrying `path` and the staleness (AC1); fresh and aging give none (AC1, AC6); a `validate_ignore` document gives none; `finding = "error"` gives `Severity::Error` and `"warning"` gives `Severity::Warning` (AC2).
6. Test-first, AC4: `finding = "off"` gives zero findings *and* the mock records zero `diff_stat` and zero `read_commit_timestamp` calls. Assert off the recorded calls -- the gate is control flow, not a filter over computed findings.
7. Implement `StaleRule`, register it in `default_checkers` (`:1529`).
8. README §Staleness: the `finding` row, and amend "No other command computes a band" (`README.md:1043`) -- `validate` does now, and `status --json` embeds its result.

## Out of scope

- Exit code, `validate --json` shape, human render, TUI panel -- next slice. This one asserts severity on the rule's own output.
- README §`validate` findings -- next slice, with the JSON shape that section describes.
- Caching. RFC-069 non-goal. This rule calls `compute` per document per `validate`, and that is the accepted cost.
- Stamping `reviewed` (STORY-274). TUI and web band badges (STORY-275).

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-002: the rule owns a `Box<dyn GitRefOps>` because `Checker` hands it none, exactly as `GovernsNoMatchRule` does. DICTUM-004: no real git in tests.

## Verification

`cargo run --quiet -- validate --json | jq '[.warnings[] | select(.rule == "stale")] | length'` is non-zero on this repo; the same command with `finding = "off"` in `.lazyspec.toml` is `0`.
