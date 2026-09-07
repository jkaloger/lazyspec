---
title: Document trust and staleness signals
type: rfc
status: draft
author: Jack Kaloger
date: 2026-09-04
tags: []
related:
- blocks: RFC-070
- supersedes: RFC-064
---

## Summary

Compute a coarse staleness band per document on demand: `fresh | aging | stale`, from a review anchor (`reviewed` sha or document date), age, and changes under the document's `governs` globs. Types name whether drift or age drives the band. Surface the facts in `show --json`, a `stale` validation finding, and a detail badge in TUI and web. Supersedes the trust portion of RFC-064 without its citation-edge dependency. Facts and a band, never a score.

## Motivation

1. Every document looks equally authoritative. A spec reviewed today and a spec describing code rewritten twice since render identically. Agents cannot tell which one is lying.
2. Age alone is wrong in both directions. An accepted RFC from a year ago is still a sound decision. A convention untouched for a week is misleading the moment its governed code changes.
3. RFC-064 §3 needed per-citation validity, which needed RFC-060's citation edges, which died. Pins from RFC-068 supply a substrate with zero new authored data: drift is a `git diff --stat` over `governs`.
4. This is the one thing lazyspec knows that an agent cannot get from grep.

## Goals

- `engine::staleness::compute` returns band, driver, anchor, age and drift for any document, on demand, with no store-load cost.
- Types declare `staleness = "drift" | "age"`, default `age`. Drift types go stale on any change under `governs` since `reviewed`. Age types band by thresholds and report drift as a fact.
- `show --json` carries a `staleness` object; human `show` prints one line.
- `validate` emits a configurable `stale` finding.
- `update --status` stamps `reviewed` with `HEAD` on every local transition.
- TUI and web show the band on the selected document's detail surface, computed off the render path on selection.

## Non-goals

- Per-citation validity, citation comments, blob hashes, or any RFC-064 §2 recovery.
- Git analysis beyond diff stat, rename detection and commit time.
- Credibility scores, list columns, list filters, badges on list rows.
- Pack demotion (RFC-070 consumes the band).
- Caching. Two slow commands first.
- MCP or export.
- Stamping on transitions that arrive from a status-authority board.

## Design

### Computation

```rust
@draft pub fn compute(store: &Store, doc: &DocMeta, git: &dyn GitRefOps) -> Staleness;
```

Anchor: `reviewed` sha when present, else the document's `date`. Age: days since the anchor commit's time via the existing `GitRefOps::read_commit_timestamp`, or since `date`. Drift: `git diff --stat <reviewed>..HEAD -- <globs>` when both `reviewed` and `governs` are set; otherwise empty.

Band by driver:

| Driver | fresh | aging | stale |
|---|---|---|---|
| `drift` | no drift | never | any drift |
| `age` | age < aging | aging <= age < stale | age >= stale |

A `drift` type with no `governs`, or no `reviewed`, has nothing to diff. It falls back to `age` and reports `driver: "age"`. Nothing on `DocMeta`, nothing at store load. Only `show`, `validate` and pack call `compute`, so only they pay the git cost.

### Configuration

```toml
[staleness]
aging   = "90d"
stale   = "180d"
finding = "warning"   # warning | error; omit to disable (default warning)

[[types]]
name = "spec"
staleness = "drift"   # default age
```

Spec, convention, dictum: `drift`. RFC, ADR: `age`.

### Stamping

`update --status` sets `reviewed: <HEAD sha>` on every transition it performs. The engine hook is `Store::update_status`, which every local transition (CLI `update --status`, TUI status change, web status form, the /advance skill) already routes through. A local transition is a human looking at the document against current code; that is the review event. `pin <id>` (RFC-068) stamps without a transition.

Transitions that arrive from outside are not review events. A type with `status_authority` set takes its status from a project board; `fetch` syncs that status into the store without anyone opening the document. Those writes do not stamp. `update --status` on such a type moves the board card and also stamps, since a human ran it.

### Outputs

```json
"staleness": {
  "band": "stale",
  "driver": "drift",
  "anchor": "0123456",
  "age_days": 140,
  "drift": {"files": 12, "insertions": 310, "deletions": 85}
}
```

Human `show`: one line, `staleness: stale (drift, 12 files since 0123456, 140d)`.

`validate` emits `stale` for any document whose band is `stale`, at `[staleness].finding` severity. It is a `ValidationIssue::Stale { path, staleness }` variant and serialises through the object finding shape RFC-068 lands (`rule`, `message`, fields). CLI, TUI validation panel and web validation view carry it with no further change.

TUI and web: band badge on the selected document's detail surface. Computed on selection in a background worker, the same pattern as the search worker from BUG-011, so a cursor move never blocks the render loop on a git subprocess. The badge shows a placeholder until the result lands and is dropped if the selection moves on. Never computed for list rows. `why` (RFC-068) gains a `drifted: bool` per record here.

## Interfaces

```rust
@draft pub struct Staleness {
    pub band: Band,        // fresh | aging | stale
    pub driver: Driver,    // drift | age
    pub anchor: Anchor,    // Sha(String) | Date(NaiveDate)
    pub age_days: u64,
    pub drift: Drift,      // files, insertions, deletions
}

@draft pub struct StalenessConfig {
    pub aging: Duration,
    pub stale: Duration,
    pub finding: Option<Severity>,   // None = off
}

@draft pub enum ValidationIssue {
    // existing variants, plus RFC-068 additions
    Stale { path: PathBuf, staleness: Staleness },
}

@draft trait GitRefOps {
    // existing methods (read_commit_timestamp supplies anchor time), plus RFC-068 additions
    fn diff_stat(&self, root: &Path, from: &str, to: &str, paths: &[String]) -> Result<Drift>;
}
```

```text
lazyspec show SPEC-001 --json | jq .staleness
lazyspec validate --json | jq '.warnings[] | select(.rule=="stale") | .path'
lazyspec update SPEC-001 --status accepted --json   # stamps reviewed
```

## Decisions (ADRs to emit)

1. **Anchor is one commit sha in frontmatter.** Rejected: date only (day granularity, means created not reviewed, does not move on re-confirmation), per-file blob hashes (frontmatter explodes for module globs, rewrites on every change), tree hash of matched set (boolean only, no diff or rename, lazyspec-defined hash).
2. **Compute on demand, not at store load.** Rejected: compute at `Store::load` (a git subprocess per document on every command), on-demand plus a cache keyed on `(reviewed, HEAD)` (principle 6, no measured need).
3. **Driver is a type-level key naming behaviour.** `staleness = "drift" | "age"`. Rejected: per-doc frontmatter flag (author burden), derive from terminal lifecycle status (conflates done with evergreen), no distinction (wrong for RFCs or wrong for specs), the word "evergreen" (names the document, not the behaviour).
4. **Stamp in `Store::update_status`, on every local transition.** One hook covers CLI, TUI, web and skills. Rejected: stamp only into "accepted-class" statuses (lifecycles are per-type and configurable; nothing marks a status as accepted-class, so the engine would need a new list knob), stamp never (evergreen re-review becomes a separate chore), stamp in the /advance skill (skills are one caller of many; a TUI status change would not stamp), stamp on board-synced status (nobody looked at the document).
5. **Band, never score.** Three values a human can argue with. Rejected: weighted credibility number (false precision, invites gaming).
6. **Badge on detail only, computed off the render path.** Rejected: band on list rows cached at startup (git subprocess per row, layering regression into the TUI render path), synchronous compute on selection (blocks the event loop on every cursor move; BUG-011 already moved search off it for the same reason), findings only with no badge (the one place a human inspects a document would not show it).

## Stories

1. **Compute and expose staleness.** `[staleness]` and per-type driver config, `compute`, `GitRefOps::diff_stat`, `show` JSON and human line.
2. **Find stale documents and stamp review.** `stale` validation finding, `update_status` stamping, `drifted` on `why`.
3. **Show the band where documents are inspected.** TUI and web detail badges, computed in a background worker on selection.

## Risks and tradeoffs

- **Drift is binary for `drift` types.** One-line typo fix under `governs` makes a spec stale. Accepted: that is the correct signal; `pin` clears it in one command and the diff stat shows how much actually moved.
- **Stamping on every local transition widens what `reviewed` means.** A draft-to-review transition claims review against code. Accepted: the alternative is an undefined status class or a new config list; any local transition is a human act on the document.
- **Board-driven types stamp less often.** A type whose status lives on a project board only stamps through `pin` or a local `update --status`. Accepted: a synced status is not evidence anyone read the document; unowned findings and the `stale` finding push those documents toward `pin`.
- **Git cost per document.** Pack over a deep chain runs one diff per record. Accepted until measured; caching is a named non-goal with a known key.
- **Falls back to age for unpinned drift types.** A spec with no `governs` never reports drift. Accepted: RFC-068's unowned findings push pins onto exactly those documents.
- **RFC-064 lineage.** This RFC and RFC-070 together supersede RFC-064. Accepting both moves RFC-064 to `superseded` (`lazyspec update RFC-064 --status superseded`) in the same change that accepts them. Its citation-edge section has no successor.
