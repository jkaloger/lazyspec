---
title: 'Backlog queue: ownership, prioritization, and a computed work queue'
type: rfc
status: draft
author: Jack Kaloger
date: 2026-07-11
tags: []
related:
- related-to: RFC-056
- related-to: RFC-035
provenance:
- Feature brainstorm interview 2026-07-11
---

## Summary

Turn the document graph lazyspec already maintains into a prioritized, ownership-aware **work queue**. Three primitives, each expressed in lazyspec's existing grammar (a frontmatter field, a query, a validation rule, a view):

1. **Ownership** — an `assignee` (and `reviewer`) field on every document, mapped to a store's native assignee where one exists (GitHub, ClickUp) and stored in frontmatter otherwise.
2. **Prioritization** — a config-driven `[prioritization]` scheme that orders documents by declared attributes, a composite formula, and graph-derived urgency. Unopinionated: the team declares its scheme (MoSCoW, p1–p3, high/med/low, explicit rank, value/cost), lazyspec computes the order.
3. **Ranked queue** — a `next` command that topologically walks the chain/blocks graph, filters to the actionable and unblocked, optionally scopes to the current user, and orders by the declared prioritization. "What should I work on next" answered from the specs themselves.

This is the smallest slice that delivers the killer feature: (1) and (2) are the substrate the queue (3) is computed from; neither ships value alone.

## Motivation

lazyspec is a shared source of truth for teams, but it says nothing about *who does what next*. Today the only actor concept is the free-text `author` field and the agent-lease machinery (`refs/lazyspec/leases/*`); neither models human ownership, and nothing surfaces "what is assigned to me, unblocked, and worth doing first." A team member reasons about that by eye, re-deriving from the graph what the graph already knows.

The graph is the point. `implements`/`blocks` edges already encode dependency order; lifecycle states already encode what is actionable. The missing inputs are *who* owns a document and *how* the team weights competing work. Add those two declared fields and the queue is a pure read over data lazyspec already holds — a spec that blocks four stories is on the critical path whether or not a human noticed. This makes lazyspec's ordering better than gut ordering without adding a scheduler, a server, or any runtime state.

The primitives are also universal: `assignee` and a prioritization attribute are as useful on a filesystem RFC as on a ClickUp task, and both map onto the native fields the store backends already round-trip (ClickUp exposes native `priority`/`assignee`; GitHub exposes issue assignees).

## Goals

- A first-class `assignee` field (and `reviewer`) on `DocMeta`, resolvable by the current user (`@me`), filterable in `list`, and shown as a TUI column and a web group-by.
- Store-native mapping: for `github-issues` the field round-trips through issue assignees; for `clickup-tasks` through the task's native assignee; for filesystem it lives in frontmatter. One lazyspec concept, mapped per backend through the existing dispatch layer — no parallel source of truth.
- A config-driven `[prioritization]` scheme: an ordered list of sort keys over declared `[[types.attributes]]`, plus an optional composite formula key, plus a built-in graph-urgency key. Enum attributes order by their declared variant order (so MoSCoW / p1–p3 / high-med-low fall out for free).
- A `next` command (and `list --mine`) that computes the ranked queue: topological order over chain/blocks edges, filtered to unblocked ∩ actionable-status ∩ (optionally) assigned-to-me, ordered within that by the prioritization scheme.
- `--json` on everything, per convention principle 2. The queue is retrievable programmatically; agents consume the same ranking humans see.

## Non-goals

- **No review-as-handoff workflow, no team rollup dashboard, no git-activity feed.** Those are the remaining facets of the queue vision (facets ④ and ⑤ from the design brainstorm) and depend on ownership + prioritization landing first. Deferred to follow-up RFCs.
- **No user/team registry, no permissions, no accounts.** `assignee` is a string resolved against git identity (`git config user.name`/`user.email`) and the store's native user handles; `@me` resolves to the current identity. lazyspec does not own a user table.
- **No scheduler, no daemon, no runtime state.** The queue is a pure function of committed frontmatter and the graph, recomputed on demand. This keeps the feature inside the "structured-markdown doc tool" scope; the abandoned orchestration vision is not revived.
- **No capacity/velocity math (WIP-vs-throughput).** Estimates can feed a prioritization formula, but burndown/forecasting is out of scope.
- **No pluggable/scripted scoring hook.** Prioritization is declared data (sort keys + formula), not a shell-out — a scriptable scorer would drag runtime execution back in.

## Design

### Ownership

Add `assignee: Option<String>` and `reviewer: Option<String>` to `DocMeta` (`src/engine/document.rs`), serialized in frontmatter like `author`/`status`. A value is an opaque identity string; the resolver (extending `src/engine/agent.rs`'s `resolve_agent_id`) maps `@me` to the current git identity so `list --assignee @me` works without configuration.

Store mapping goes through the existing dispatch layer rather than a new concept:

- **filesystem** — the field is authoritative in frontmatter; no remote.
- **github-issues** — on `fetch`, the issue's first assignee populates `assignee`; on `update --assignee`, lazyspec sets the issue assignee via `gh`. GitHub issues carry multiple assignees; v1 reads/writes the first and preserves the rest on write (read-modify-write of the assignee list), leaving multi-assignee as a later concern.
- **clickup-tasks** — maps to the task's native assignee field, round-tripped by the `clickup-tasks` store the same way `priority`/`estimate`/`due` already are (native field, not a custom-field or body blob). Guarded by the store's existing optimistic lock on `task-map.json`.

`list` gains `--assignee <id>` (accepting `@me`) and `--unassigned`. `show --json` and `status --json` include `assignee`/`reviewer` in each document entry, always present (`null` when unset), consistent with the `attributes` field's always-present contract.

The TUI documents table gains an optional `assignee` column (configurable under `[tui.graph] columns`, which already accepts declared attribute names); the web view gains a group-by-assignee toggle alongside the existing status/tag filters. Per the cross-surface rule, the engine computes; CLI/TUI/web render.

### Prioritization

A new optional `[prioritization]` config block declares how the store is ordered. It is unopinionated: the block references declared attributes, so any scheme the team models with `[[types.attributes]]` (enum, int, float, date) is expressible.

```toml
[prioritization]
# Ordered sort keys, applied lexicographically (first key dominates).
# Each key is: a declared attribute name, the built-in `urgency`, or a
# declared formula key; with an optional direction.
order = ["moscow", "urgency desc", "date asc"]

# Optional composite keys, referenced by name in `order`.
# Arithmetic over numeric declared attributes; higher = higher priority.
[prioritization.formulas]
wsjf = "value / cost"
```

Semantics:

- **Enum attribute** — orders by the enum's declared variant order in `[[types.attributes]]`. A `moscow` enum declared `["must", "should", "could", "wont"]` sorts `must` first. This is how MoSCoW, p1–p3, and high/med/low are expressed — no special-casing in the engine, just an ordered enum.
- **Numeric / date attribute** — orders by value with the stated direction (`asc`/`desc`); default `desc` (higher priority first). Explicit manual ranking is a plain `int` attribute (`rank`) used as a sort key.
- **`urgency`** — a built-in key computed from the graph: how much a document unblocks. v1 defines it as the count of documents transitively reachable from this one along `blocks` (and chain) edges — i.e. how many things wait on it — reusing the traversal the context forest already walks (`src/engine/` graph/context code). A leaf blocks nothing (urgency 0); a foundational spec blocks the subtree beneath it. This is the "smarter than gut ordering" signal and costs no new stored state.
- **Formula key** — a named arithmetic expression over numeric declared attributes, evaluated per document, ordered `desc`. A small, closed expression evaluator (the four operators, parentheses, attribute references, numeric literals) — deliberately not a general scripting host. A formula referencing an attribute a document lacks yields no score and sorts last, mirroring the existing "missing attribute sorts last" rule in the graph view.

A missing `[prioritization]` block means documents order by their existing stable order (path/topological), so configs without the block are unaffected. Sort keys and formulas are validated at config load: an unknown attribute name, an enum key that isn't an enum, or a formula referencing an undeclared attribute is a config error surfaced up front (consistent with how lifecycle/relationship validation already rejects unknown references).

### Ranked queue

A new `next` command composes ownership, prioritization, and the graph into an ordered actionable list:

```sh
lazyspec next                 # whole-team queue, ranked
lazyspec next --mine          # scoped to @me
lazyspec next --type story    # one type
lazyspec next --json          # machine-readable, for agents
```

The computation, in order:

1. **Topological walk** over chain + `blocks` edges (the traversal the context forest already performs), establishing dependency order.
2. **Filter to actionable** — a document is actionable when its status is one from which lifecycle edges lead forward (i.e. not a terminal state like `complete`/`rejected`/`superseded`), and when it is **unblocked**: every document it is `blocked-by` (or that must precede it via a chain edge) has reached a terminal/complete status. A document waiting on unfinished upstream work is withheld from the queue, not ranked low.
3. **Scope** — with `--mine`, keep only documents whose `assignee` resolves to `@me`; otherwise keep all.
4. **Rank** — order the survivors by the `[prioritization]` scheme. With no scheme configured, `urgency` is the default sole key, so `next` is useful before any prioritization is declared.

`list --mine` is the flat, unranked convenience filter (assignee = `@me`, any status); `next` is the ranked, dependency-aware, actionable subset. The TUI surfaces `next` as a queue view for the individual contributor; the web view renders the same ranked list for a shareable read-only queue. `--json` emits each entry with its computed rank position, the prioritization key values that placed it, and its `urgency`, so an agent (or a human) can see *why* an item ranks where it does rather than trusting an opaque order.

## Interfaces

- `DocMeta.assignee: Option<String>` @draft — new frontmatter field (`src/engine/document.rs`)
- `DocMeta.reviewer: Option<String>` @draft — new frontmatter field (`src/engine/document.rs`)
- `resolve_assignee(&str) -> Identity` @draft — `@me` resolution extending `resolve_agent_id` (`src/engine/agent.rs`)
- `PrioritizationConfig` @draft — `[prioritization]` block: `order: Vec<SortKey>`, `formulas: HashMap<String, Formula>` (`src/engine/config.rs`)
- `SortKey` @draft — `{ attribute | urgency | formula, direction }`, validated against declared attributes
- `Formula` @draft — closed arithmetic expression over numeric attributes, with evaluator
- `urgency(doc, graph) -> usize` @draft — count of documents transitively unblocked by this one, over the existing graph walk
- `next` command @draft — ranked actionable queue (`src/cli/`), `--mine`/`--type`/`--json`; TUI queue view; web ranked list
- `list --assignee <id> | --unassigned` @draft — ownership filters, `@me` accepted (`src/cli/`)

