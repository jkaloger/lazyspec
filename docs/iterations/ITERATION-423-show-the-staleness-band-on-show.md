---
title: Show the staleness band on show
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-272
- blocks: ITERATION-424
---

## Objective

`show <id>` carries the staleness object in `--json` and one staleness line in human output.

## Satisfies

STORY-272 AC1, AC2, AC3, AC4, AC5, AC6 -- end to end, on the surface the ACs name.

## Context

- Story + ACs: STORY-272
- JSON object and the human line's exact wording: RFC-069 §Design "Outputs"
- `compute` and its config land in the two previous slices. This slice calls, it does not decide.
- Touch:
  - `src/cli/show.rs:202` `run_json` -- `json["staleness"]` beside `body` and `comments`.
  - `src/cli/show.rs:107` `run` -- the line after `pin_rows` (`:146`), which is where `Governs:` and `Reviewed:` already print.
  - Both need a `&dyn GitRefOps`; `run` also needs `config` and `root`, which `run_json` already takes. `src/main.rs:263` and `:275` are the call sites; `GitCli` is imported at `:13` and passed boxed at `:607`.
  - `src/cli/json.rs:35` `doc_to_json` is shared with `list`, `context` and `status`. Staleness does not go there -- AC8 is a later slice but this is where it would be broken.
- The band is unconditional. A document with neither `reviewed` nor `governs` still prints a line, off its `date`.
- The human line prints whatever the anchor is: a sha for a reviewed document, a date for one without.

## Tasks

1. Thread `config`, `root` and a `&dyn GitRefOps` into `show::run`, and the git ops into `run_json`, from `main.rs`.
2. Test-first: `run_json`'s object matches RFC-069 §Outputs for a drifted document and for an age-driven one.
3. Test-first: the human line for both, including the date anchor.
4. Implement.
5. README §`show` flags: the staleness line and the JSON key.

## Out of scope

- `why` -- next slice, with AC7 and AC8.
- The `stale` validation finding -- STORY-273. TUI and web badges -- STORY-275.
- `list`, `context`, `status`, `search`. They keep the `doc_to_json` shape they have.
- `show --open`. It resolves a target and prints no document.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-006: the human line and the JSON key carry the same facts; neither surface knows something the other does not.

## Verification

`cargo run --quiet -- show CONVENTION-001 --json | jq .staleness` against `cargo run --quiet -- show CONVENTION-001 | grep staleness` -- same band, same driver, same counts.
