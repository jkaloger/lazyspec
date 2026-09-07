---
title: Packed task context
type: rfc
status: accepted
author: Jack Kaloger
date: 2026-09-04
tags: []
related:
- supersedes: RFC-064
---

## Summary

`context <id> --pack` returns one ordered, tiered context bundle for an anchor document. Same `{target, chain, forward, related}` shape as today; every record gains `tier`, `band`, `governs` and `body`. Tier follows graph position, stale records demote one tier. Built on existing chain resolution plus RFC-068 pins and RFC-069 staleness. Final RFC in the Pins -> Trust -> Pack chain. Supersedes RFC-064's packed assembly; RFC-064's globals, citation-edge and adoption designs get no successor.

## Motivation

1. Agents assemble task context by hand: `context`, then `show -e` per record, then a search. Every caller picks its own subset at its own detail. Inconsistent and token-expensive.
2. Lazyspec already knows chain membership, related-path distance, bodies, pins and staleness. It just never emits them in one call.
3. Chain context is task-level information. The highest-leverage lazyspec call an agent makes is "give me what I need for this story". One command, deterministic, pipeable.
4. RFC-064 §1 survives the split intact; its infra (`resolve_chain`, `merge_declared_related`, `get_body_expanded`) exists.

## Goals

- `context <id> --pack --json` emits the existing shape with `tier`, `band`, `governs`, `body` on every record. Existing `context --json` consumers are unaffected.
- Tiers: target and chain ancestors `full`, forward-chain descendants and one-hop related `summary`, two or more hops `title`.
- A `stale` record demotes one tier. Target never demotes. `title` is the floor.
- `summary` is the `## Summary` section, falling back to the first heading block.
- Human output is Markdown in assembly order, one heading per record with tier and band, then body at tier.
- TUI Relations tab and web context panel show the tier label per row.
- `DocMeta`, `Relation`, `EdgeDef`, `Store` and `.lazyspec.toml` untouched beyond RFC-068 and RFC-069.

## Non-goals

- Any second regime. No globals, no tag-selected documents, no convention selection. `convention --preamble` keeps injecting convention and dictums into sessions.
- `@ref` expansion inside the pack. Agents pipe record ids into `show -e`.
- Per-relation tier override. Edge-table attribute when a real non-chain strong relation appears.
- Token budget, cache, new TUI screen, MCP server, export file.
- Band badge. RFC-069's detail surface owns it; pack consumes the value.

## Design

### Assembly

```rust
@draft pub fn assemble(store: &Store, id: &str, depth: usize, git: &dyn GitRefOps) -> Pack;
```

`resolve_chain` then `merge_declared_related`, exactly as `context` does today. For each record: tier from position, band from `staleness::compute`, `governs` from `DocMeta`, body from tier. Nothing stored.

### Tiering

| Position | Tier |
|---|---|
| Target, chain ancestors | full |
| Forward-chain descendants, related at one hop | summary |
| Related at two or more hops | title |

`stale` demotes one tier: full to summary, summary to title, title stays title. The target is exempt; the caller asked for it. `summary` = `summary_section(body)`: the `## Summary` block, else the first heading block. `title` has `body: null`.

### Output

JSON: the current `context --json` object with the four fields added to every record. Human: Markdown in `target, chain, forward, related` order, one `## <id> <title> (tier, band)` heading per record, then the body at tier.

### Surfaces

TUI: tier label on each Relations-tab row. Web: same label in the document page's context panel. Both already walk `resolve_chain` at depth 1. Tier is structural, so the label needs no git call; the band stays on RFC-069's detail badge. No new screen.

## Interfaces

```rust
@draft pub struct Pack {
    pub target: PackRecord,
    pub chain: Vec<PackRecord>,
    pub forward: Vec<PackRecord>,
    pub related: Vec<PackRecord>,
}

@draft pub struct PackRecord {
    pub meta: DocMeta,
    pub tier: Tier,          // full | summary | title
    pub via: RelationPath,
    pub hops: usize,
    pub band: Band,
    pub governs: Vec<String>,
    pub body: Option<String>,
}

@draft pub fn summary_section(body: &str) -> Option<&str>;
```

```text
lazyspec context STORY-300 --pack --json
```

```json
{
  "target": {"id":"STORY-300","tier":"full","band":"fresh","governs":[],"body":"..."},
  "chain":   [{"id":"RFC-068","tier":"full","band":"aging","governs":["src/engine/governs/**"],"body":"..."}],
  "forward": [{"id":"ITER-410","tier":"summary","band":"fresh","governs":[],"body":"## Summary\n..."}],
  "related": [{"id":"ADR-012","tier":"title","band":"stale","governs":[],"body":null}]
}
```

## Decisions (ADRs to emit)

1. **Extend `context`, do not add a verb.** `--pack` on the existing command, existing shape preserved. The rule: the same question (what surrounds this document) with a richer answer is a flag; a new question gets a verb, which is why RFC-068's `why <path>` (code to documents) is one. Rejected: separate `pack <id>` command (a new verb for a flag of behaviour), flat ordered `records[]` with a section field (second shape for the same graph; TUI and web would map it back into sections).
2. **Chain-only, no globals regime.** Rejected: globals selected by tag intersection with the anchor (user does not want a global-doc concept), globals via `governs` overlap with the chain, globals as an implicit edge-table rule. The session hook is the mechanism for convention.
3. **Structural defaults, zero config.** Rejected: `[context]` table for summary section name and default depth (two knobs nobody asked for), per-relation tier override on the edge table (no project has a non-chain strong relation yet, principle 6).
4. **Band and tier live on `PackRecord`, not `ContextNode`.** Rejected: fold into `ContextNode`/`RelatedRef`. `resolve_chain` feeds the TUI Relations view on every render; a git subprocess there is a layering and performance regression.
5. **No `@ref` expansion in the bundle.** Rejected: inline expansion (bloats an unbudgeted bundle, muddies record shape). `show -e` per record already exists.
6. **Build order Pins -> Trust -> Pack.** Pack ships last with `band` and `governs` on every record from day one; its JSON shape is designed once with nothing reserved. Rejected: hub-and-spokes (pack first with empty sockets, shape before spokes), independent bets (pack shape reopened twice as spokes land).

## Stories

1. **Assemble structural tiers.** `engine::pack`, `summary_section`, `context --pack` JSON and human output. Band fixed at `fresh` until story 2.
2. **Carry pins and trust.** `governs` and `staleness::compute` per record, stale demotion with target exempt.
3. **Show tiers in existing views.** Relations-tab and web context-panel labels.

## Risks and tradeoffs

- **One git diff per record.** A deep chain with many pinned documents pays N subprocesses per pack. Accepted until measured; RFC-069 names caching as the upgrade with key `(reviewed, HEAD)`.
- **Summary heuristic.** Documents without a `## Summary` section get their first heading block, which may be Motivation. Accepted: template-driven documents have Summary; the fallback is visible in output and fixable in the document.
- **Fixed tiers can be wrong for a project.** A strong non-chain relation deserves `full`. Accepted: edge-table attribute is the named upgrade when the second use appears.
- **Pack ships last.** Highest-leverage piece waits on two RFCs. Accepted in the interview: a shape designed once beats a shape reopened twice.
- **RFC-064 adoption section dies.** Its density-loop plan assumed citation edges. Unowned findings in RFC-068 carry the adoption pressure instead. Accepting this RFC and RFC-069 moves RFC-064 to `superseded` in the same change.
