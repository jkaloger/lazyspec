---
title: "Context engine: packed assembly, citation comments, and trust signals"
type: rfc
status: review
author: "jack"
date: 2026-07-20
tags: []
related: []
---## Summary

Lift lazyspec from a typed relationship store to a context engine. Three parts, in dependency order:

1. **Packed assembly** — `lazyspec context <id> --pack` emits one assembled, tiered context bundle for a work anchor: target body, chain ancestors, conventions, expanded `@ref`s, related-doc summaries. One call replaces the compose-it-yourself dance every agent currently reimplements.
2. **Citation edges** — doc↔code links stored as RFC-060 comments (`kind: citation`) carrying path, blob hash, and commit hash. Append-only observations, not frontmatter mutations.
3. **Trust signals** — per-citation validity (`fresh | drifted | moved | gone`) computed from git plumbing, and per-doc staleness (raw signals plus a derived `fresh | aging | stale` band). Retrieved context arrives labelled with how much to trust it.

The CLI with `--json` is the API. No MCP server, no daemon, no exported snapshot files. Blocked by RFC-060: the comment layer is the substrate for citations and everything downstream of them.

## Motivation

**Assembly is manual and lossy.** `context` today returns chain metadata; agents must then call `show -e`, `convention`, and `search` themselves and decide what matters. Every skill reimplements that policy, each differently. The engine knows relation types, distance, and status — the caller assembling by hand does not. Packing belongs in the engine.

**Docs rot silently.** An RFC that is six months and 500 commits old reads as authoritative as one written yesterday. Age alone is a weak signal (an accepted ADR is supposed to be old), but "cited code 60% drifted, doc untouched, status draft" is a real one. Nothing computes or surfaces it today.

**The graph stops at the doc boundary.** Relations link docs to docs. The question agents actually ask mid-task — "which docs govern the file I am editing?" — has no index. `@ref` directives exist but density depends on authors hand-maintaining prose, so a reverse index built only on them starves.

**Priority.** Assembly first, trust second, discovery (query-driven search, embeddings) deferred: lazyspec's structural edge is typed relations, which assembly exploits and search does not — agents already grep well.

## Goals

- One command yields ready-to-use, priority-tiered context for a work anchor. Agents and humans consume the same interface (`--json`).
- Doc↔code edges are first-class, cheap for agents to write, and never require rewriting the document.
- Every citation is verifiable against git; every doc carries computable staleness signals plus a coarse band.
- Tiering needs zero configuration to be sensible, and one optional key to tune.
- TUI, web, `list`, and `status` surface the same trust signals the CLI emits.

## Non-goals

- **No MCP server, no exported context files.** The CLI is the API; snapshots rot, which fights the trust layer. MCP can be a thin adapter later.
- **No token budget in v1.** Tier degradation is driven by relation semantics, not size. A `--budget` flag is future work if real bundles bloat (convention principle 6).
- **No credibility score.** The engine emits facts and a coarse band, never an invented 0–1 trust number.
- **No semantic search / embeddings.** Discovery tier is out of scope.
- **No new TUI screens.** The existing graph view is the human context view; parity is badges and annotations on existing surfaces.
- **No structured provenance rework.** `provenance` stays legacy free-text strings; citations supersede it for doc↔code purposes.

## Design

### Packed assembly: `context <id> --pack`

`--pack` upgrades the existing chain resolution from metadata to content. Bundle assembly order:

1. Target doc — full body.
2. Chain ancestors — per-tier body (see below).
3. Convention + dictums in scope for the target's type.
4. Expanded `@ref` blocks (existing `show -e` machinery, pinned hashes verified).
5. Related docs — summaries or title lines per tier.

**Tier per doc from graph position, not size:**

| Position | Tier |
|---|---|
| Target | full |
| Chain relation (ancestors, `implements`) | full |
| Non-chain, 1 hop (`related-to`) | summary |
| 2+ hops | title + frontmatter line |
| Staleness band `stale` | demote one tier, flag |

`summary` tier = the doc's own Summary section; fallback is the first heading block. Structured markdown gives graceful degradation without an LLM.

Relation vocabulary is project-defined, so the engine cannot hardcode which custom relation is "strong". Default is structural: `chain_relationships` membership partitions full vs summary. A relation declaration may carry an optional `context-tier: full | summary | title` override for projects whose strong relations are non-chain (`supersedes`, `constrains`). Unset means structural default; nobody must configure anything.

`--json` emits the bundle as structured records, each annotated with tier, relation path, staleness band, and citation states. Human output renders the same order as markdown.

### Citation edges (blocked by RFC-060)

A doc↔code edge is a comment with `kind: citation` and attributes:

```yaml
attributes:
  kind: citation
  path: src/engine/graph.rs
  blob: <blob hash at cite time>
  commit: <HEAD commit at cite time>
```

Comments, not structured frontmatter, deliberately:

- **Frontmatter mutation poisons the staleness signal.** Doc staleness reads `last_edited` from the doc file's git history. If every cite or re-pin rewrites frontmatter, the trust engine corrupts its own input. Citations must not touch the doc.
- **Citations are observations, not authored structure.** Doc↔doc relations are intentional design edges — frontmatter is their home. A citation is a dated claim: "at time T this doc described code X@hash". It decays and gets superseded; ledger semantics are native to the append-only comment stream.
- **Concurrency.** Two agents citing the same doc write distinct maildir files — no contention. Frontmatter would be a write conflict on one file, exactly when agents maintain citations at the density we want.
- **Attribution free.** The envelope carries author and timestamp.

The current edge set is derived by folding the stream — latest citation per (doc, path). Aging projects accumulate a citation ledger: a temporal history of what code each doc described when, queryable, with validity computed rather than assumed.

`@ref` directives stay as prose-level illustration and remain indexed and validity-checked via their existing pin hashes; citations are the structured layer, not a replacement.

**Reverse lookup:** `lazyspec context --for-file <path>` folds citation streams plus an `@ref` scan and returns docs citing the path, annotated with validity and staleness, composable with `--pack`. This answers "which docs govern this file" for maintenance and bugfix flows where no work item exists yet.

### Trust signals

**Citation validity** — blob hash answers "did the content change", commit hash anchors history walks. All local git plumbing:

| Check | Mechanism | Verdict |
|---|---|---|
| Content unchanged | `HEAD:<path>` blob equals cited blob | `fresh` |
| Path missing, rename-chase (`git diff -M <commit>..HEAD`) finds it | follow rename | `moved(new-path)` |
| Path missing, no rename found | — | `gone` |
| Blob differs | `git rev-list --count <commit>..HEAD -- <path>` + changed-lines ratio vs cited blob | `drifted(score)` |

The same checks apply to `@ref` pin hashes. `lazyspec validate --citations` sweeps the store and reports non-fresh edges; `moved` is auto-healable by appending a corrected citation.

**Doc staleness** — raw signals per doc, no invented score:

- `last_edited` (git log of the doc file — kept honest because citations never touch it)
- commits on cited paths since `last_edited`
- citation state histogram (`fresh: 1, drifted: 3, gone: 1`)
- age, lifecycle status

Plus a derived three-band verdict `fresh | aging | stale` from default thresholds. Bands are type- and status-aware: an `accepted` ADR is supposed to be old; a `draft` RFC at six months is rotting. Per-type threshold overrides are future config, added when a project actually needs to tune them — v1 ships defaults only.

Signals and band appear in `--pack` output, `status`, and `list --json`. Agents reason from the facts; humans glance at the band.

### Surfaces and parity

- **CLI**: everything above, all `--json`.
- **TUI / web**: staleness band as a colour badge in list and detail views; doc detail shows folded citations with validity states. The graph view is the context view — no new screen.
- **`status`**: store-wide rot summary (counts per band, worst offenders).

Engine owns fold, validity, tiering; CLI formats; TUI/web render. Dependencies flow inward per convention principle 3.

### Adoption: closing the density loop

A reverse index is only as good as citation density. Two levers:

1. **Skills mandate citations.** `execute` posts a citation when work touches code a doc describes; `review` checks citations exist for delivery docs; `writing-iterations` cites the code an iteration targets. Agents author most docs, so the discipline cost lands on agents.
2. **The flywheel.** `--pack` and `--for-file` surface cited docs prominently; uncited docs are invisible in agent flows. Citing pays off visibly, so density grows without enforcement. Validation stays a warning (delivery doc with zero citations), not a gate.

### Sequencing

1. **RFC-060 comment layer** — substrate; everything trust-related consumes it.
2. **`context --pack`** — bundle, tier degradation, `context-tier` config key, `@ref` pin validity, staleness-lite (age + status signals available without citations).
3. **Citations** — `kind: citation` vocabulary, fold/derived index, `--for-file`, full staleness (cited-path churn), `validate --citations`.
4. **Adoption + parity** — skill updates, TUI/web/`status` badges.

Each slice is independently shippable; slice 2 does not depend on RFC-060 but sequencing it after keeps the trust layer arriving as one coherent story.