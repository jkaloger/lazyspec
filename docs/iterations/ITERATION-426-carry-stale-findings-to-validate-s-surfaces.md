---
title: Carry stale findings to validate's surfaces
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-273
---

## Objective

`validate`'s JSON, its human render, its exit code and the TUI validation panel all carry the `stale` finding, none of them knowing it exists.

## Satisfies

STORY-273 AC1, AC2, AC5 -- end to end, on the surfaces those ACs name. Closes STORY-273.

## Context

- Story + ACs: STORY-273. Object finding shape: STORY-266, `src/engine/validation.rs:191` `to_json`.
- Most of this is already true by construction. The slice's work is proving it and writing it down:
  - AC1's fields: `#[serde(untagged)]` (`validation.rs:32`) serialises the variant's own fields flat, so the finding is `{rule, message, path, staleness: {...}}` and `staleness` is the RFC-069 §Outputs object verbatim -- the same object `show --json` prints.
  - AC2's exit code: `Severity::Error` routes to `result.errors` (`validation.rs:1553`), and a non-empty `errors` is `run_full`'s exit 2 (`src/cli/validate.rs:58`). No new code, one test.
  - AC5's TUI half: `refresh_validation` (`src/tui/state/app.rs:920`) maps every finding through `to_string()`, so the panel gets `message` and nothing per-rule. Test mirrors `app.rs:4726`.
  - `status --json` (`src/cli/status.rs:24`) embeds the same objects, free.
- **AC5's web half has no target.** `src/web/server.rs:76` routes list, fragment, search, graph, doc and static -- there is no web validation view for a finding to reach. STORY-266 §Notes recorded that gap; ITERATION-411 put building one out of scope and nothing since has built one. Do not build one here: it is a surface, not a finding, and it wants its own story. The clause is satisfied vacuously and stays satisfied the day a web validation view lands, because that view will read the same objects the TUI panel reads.
- No mock git needed end to end. A document dated `2020-01-01` with no `reviewed` anchors on its date, bands `stale` off the thresholds, and issues zero subprocesses; one dated today is `fresh`. `validate.rs:199` already builds fixtures this way.
- `run_full` (`validate.rs:37`) runs `validate_full`, then `run_json` or `run_human` runs it a second time. Two full validations per invocation, and now two git diffs per pinned document. Pass the one result down.

## Tasks

1. Test-first, AC1: `run_json` over a store holding stale, aging and fresh documents gives exactly one finding with `rule == "stale"`, carrying `path`, `staleness.band == "stale"`, and a `message` equal to the variant's `Display`.
2. Test-first, AC2: `finding = "error"` puts it in `errors` and `run_full` exits 2; `finding = "warning"` puts it in `warnings` and exits 0.
3. Test-first, AC5: `refresh_validation` puts the message into the TUI's `validation_warnings`, with no `stale` branch anywhere in `src/tui/`.
4. `run_full` computes `validate_full` once and passes the result to `run_json` and `run_human`.
5. README §`validate` findings: `stale`, its `path` and `staleness` fields, and that its severity is configurable so it reaches either array -- `unsatisfied-edge` (`README.md:368`) already sets that wording.

## Out of scope

- Building a web validation view. Not this story's, per Context.
- `list`, `context`, `search`. `doc_to_json` gains nothing, as in ITERATION-424.
- Human `validate` wording beyond what `Display` prints. `run_human` (`validate.rs:110`) prints `message` for every rule and gets no rule-specific line.
- Caching. Stamping `reviewed` (STORY-274). TUI and web band badges (STORY-275).

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-006: JSON and human carry the same facts. STORY-266's whole point is that a new rule reaches every surface without any surface naming it -- a `stale` branch in the TUI or in `run_human` is the failure this slice exists to prevent.

## Verification

`cargo run --quiet -- validate --json | jq '[.errors[], .warnings[]] | map(select(.rule == "stale")) | length'` matches the count the TUI panel shows for this repo. Check the exit code on a scratch project, not here: this repo already exits 2 on pre-existing errors, so it proves nothing about AC2.
