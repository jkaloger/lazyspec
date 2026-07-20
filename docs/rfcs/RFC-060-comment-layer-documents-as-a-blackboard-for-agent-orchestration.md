---
title: 'Comment layer: attributed append-only annotations on documents'
type: rfc
status: review
author: jack
date: 2026-07-12
tags: []
related:
- blocks: RFC-064
---

## Summary

Add a comment layer to lazyspec documents: attributed, append-only, immutable annotations posted against a document or a section of it. This is the same primitive as a review comment on a spec, a margin note on an RFC, or a discussion thread on a story — a fine-grained contribution that lives beside the document without rewriting it. Each comment carries a small engine-owned envelope (id, author, timestamp, reply-to) plus a **project-configurable attribute map** — the vocabulary (`kind`, `confidence`, `anchor`, …) is declared in config, exactly as document types already are, not hard-coded in the engine. Thread structure and resolution state are *derived* by folding the append-only stream, never stored as mutable fields. Storage is a filesystem comment-per-file (maildir) layout: still just markdown on disk, PR-reviewable, `cat`-able. Every operation has `--json`, so both humans and agents consume the same interface.

## Motivation

**The missing primitive: annotate without rewriting.** A lazyspec document is a whole artifact — you advance it, review it, edit its body. But there is no way to attach a fine-grained, attributed note *to* a document or a section of it without rewriting the document. Reviewing an RFC, you want to leave a comment on section 3. Reasoning about a spec, you want to record an observation, challenge a claim, or note a decision — dated and attributed — beside the doc, not folded into its prose. Today that has nowhere to live except the document body itself, which loses attribution, loses ordering, and forces a full rewrite for every note. A comment layer is the primitive that fills this gap, and it is squarely a documentation feature: attributed, append-only annotations on structured markdown.

**Append-only, so notes are safe and concurrent.** A comment is never edited. A correction is a new comment; a resolution is a new comment; agreement is a new comment. This is event sourcing: thread structure and resolution status are *folded* from the stream, never written as mutable fields. Immutability is what makes the reasoning trail survive — a claim is challenged, not erased — and it is also what makes concurrent posting conflict-free: each comment is a distinct entry with a unique id, so two writers never contend for the same target. On a maildir filesystem layout this falls out for free: distinct ULID-named files, no shared mutable state, clean git merges.

**Configurable attributes, mirroring document types.** Comments are not a fixed-schema social feature. Each comment carries a project-declared attribute map — `kind`, `confidence`, `anchor`, or whatever a project needs — validated against a config schema exactly the way document `[[types]]` are. The engine ships a sensible default vocabulary and hard-codes none of it. This keeps the engine domain-agnostic and consistent with how lazyspec already treats configurability.

**One consumer, not the reason.** These same properties — attributed, timestamped, append-only contributions with a `--since` view — happen to be what an external agent orchestrator would want if it used the doc store as shared state. That is a legitimate downstream *consumer* of the primitive, not its justification. This RFC scopes the comment layer as a documentation feature that stands on its own; orchestration is deliberately out of scope (see Non-goals) and is not designed for here.

## Goals

- A comment is an immutable, attributed, timestamped entry against a document or one of its sections. Never edited; corrections are new comments.
- Concurrent writers post without write conflicts (distinct-file, distinct-id).
- Comments carry semantics as **configurable attributes**, not fixed fields: a project declares its attribute schema (e.g. `kind`, `confidence`, `anchor`) the same way it declares document types. The engine ships sensible defaults but hard-codes none of the vocabulary or its enum values.
- Thread and resolution state are *derived* by folding the append-only stream, never stored as mutable fields.
- Storage is a filesystem maildir layout — still markdown on disk.
- Every operation has `--json`. Agents consume the same interface humans do.

## Non-goals

- **No orchestration.** No scheduler, no agent spawn, no priority policy, no control loop, no daemon. lazyspec is not a runner and does not model one. An external orchestrator *may* consume the comment layer, but this RFC does not design for it — no tier table, no control-loop section, no blackboard machinery. If orchestration is ever wanted, it is a separate RFC that builds on this primitive.
- **Local filesystem store only in scope.** Comments are markdown-on-disk (maildir). No git-ref store, no `--since` revision cursor, no github-issues/clickup adapters in this RFC — those served the polling orchestrator and pull in multi-store cursor semantics, remote-mirror immutability questions, and dynamic nested attributes the schema doesn't model. If a second store becomes real, it justifies the `CommentStore` trait then (convention principle 6); until then the fs impl stays inline.
- **No mutable edit/delete of posted comments.** Append-only is the invariant that makes the trail durable and concurrency safe.
- **No garbage collection of resolved threads in v1.** Deferred.

## Design

### The comment as a blackboard posting

Every comment is one contribution from one knowledge source. It has a small fixed **envelope** — the fields the engine must understand to store, order, thread, and attribute — plus an open **attribute map** that carries the blackboard vocabulary. The envelope is hard-coded because the storage and fold logic depend on it; the attributes are project configuration.

```yaml
# --- envelope: fixed, engine-owned ---
id: 01JC...            # ULID — time-sortable, globally unique, zero coordination
author: agent-planner  # KS identity → provenance
created_at: <iso8601>
in_reply_to: 01JB...   # parent comment id → threading tree (null = root)
refs: [DOC-12, 01JA...] # links to other docs / comments
# --- attributes: project-configurable, engine-opaque ---
attributes:
  kind: observation    # blackboard vocabulary — values defined in config
  confidence: 0.7      # hypothesis weighting — type/range defined in config
  anchor: "#section-slug"  # section targeted; slug not line (lines drift on edit)
---
markdown body
```

Only the envelope is interpreted by the engine. Everything under `attributes` is data. The schema governs *authored* attributes — the ones a human/agent supplies via `--attr` — declaring names, types, allowed values, defaults, whether required. The values shown (`kind`, `confidence`, `anchor`) are the **shipped default schema**, not a fixed one — a project can rename them, drop them, constrain them, or add its own (`severity`, `tags`, `resolves`, …).

**Attributes are an open map, not a closed one.** The schema constrains what may be *authored*; it does not cap what may *appear*. Remote adapters (github-issues, clickup) ingest native fields — reactions, labels, assignees, emoji tallies — as attributes **dynamically, with no config required**. A GitHub 👍 reaction becomes `attributes.reactions.+1: 3`; a clickup tag becomes `attributes.tags: [...]`. These flow through verbatim without a schema entry, are read-only (materialized from the source of truth on the remote), and surface identically to authored attributes in `--json` and the TUI. Unknown authored attributes are rejected; unknown adapter-sourced attributes are preserved. This keeps the local vocabulary disciplined while letting the blackboard absorb whatever a remote source offers.

`kind` illustrates the blackboard use: an `observation` posts new evidence; a `challenge` disputes a hypothesis without overwriting it; a `decision` records a control outcome. `confidence` lets the control component weigh competing hypotheses. These are conventions expressed *in the schema*, not compiled-in enums — a different domain configures a different vocabulary.

### Immutability is the discipline

Comments are never mutated. Every act is an append:

- Correction → new comment `in_reply_to` the old.
- Resolve a thread → append a comment carrying the configured resolution attribute (default `kind: decision`) referencing the root.
- Agreement / reaction → append.

This is event sourcing. Thread structure and resolution status are computed by folding the stream — they are not fields anyone writes. Thread structure folds from the envelope (`in_reply_to`) and is engine-owned; resolution status folds from a **configured predicate over attributes** (default: an entry with `kind == decision`), so what counts as "resolved" is project policy, not compiled-in. This is exactly why a blackboard posting never overwrites: a hypothesis is challenged, not erased, so the reasoning trail survives.

### Configurable store — the trait seam

Per convention principle 4, I/O lives behind a trait. Comment storage is one axis below the existing per-type `store` setting (documents already choose `filesystem | github-issues | git-ref | clickup`).

```rust
trait CommentStore {
    fn append(&self, doc: &DocId, c: Comment) -> Result<CommentId>;
    fn thread(&self, doc: &DocId) -> Result<Vec<Comment>>;   // ordered, ready to fold
    fn since(&self, doc: &DocId, cursor: Rev) -> Result<Vec<Comment>>;
}
```

`Comment` is the envelope plus an `attributes: Map<String, Value>` — the store persists the map verbatim and never interprets it. The engine folds threads store-agnostic. Schema validation applies only on the **authored** path (`append`/`comment add`); attributes an adapter reads back from a remote (reactions, labels) bypass validation and are surfaced as-is. Each store does one job (Unix philosophy); the store is configurable per project or per type.

**Filesystem impl — comment-per-file (maildir).**
```
docs/rfcs/RFC-0007/comments/01JC....md
docs/rfcs/RFC-0007/comments/01JD....md
```
ULID filename gives ordering for free. Distinct files → concurrent writers never touch the same path → conflict-free, and git merges distinct new paths cleanly. Human-readable, PR-reviewable, `cat`-able. This preserves lazyspec's core value: it is still just markdown on disk. Coarse-grained sharing via ordinary commit+push.

**Git-ref impl — blob + namespaced ref.**
```
body  → git hash-object -w         (immutable blob, local, cheap; no commit, no worktree churn)
point → refs/lazyspec/comments/<doc>/<ulid>   (own namespace; no branch pollution)
```
This is the *live* lane. Writes are local and fast. `git update-ref` with an old-value check is an atomic compare-and-swap — the optimistic-concurrency primitive comes free from git, lock-free. This reuses the machinery already behind reservation refs and `claim`/`leases`. Trade-off: opaque (not `cat`-able; lazyspec renders via `git cat-file`), and hosts like GitHub reject client pushes to non-standard ref namespaces — so git-ref comments are local / self-hosted / bare-remote, not cross-GitHub.

**Remote adapters — github-issues, clickup.** A comment maps 1:1 to a native issue/task comment. Thin adapters over the existing store backends. The adapter projects native side-channel data onto the attribute map dynamically: GitHub reactions → `attributes.reactions.{+1,heart,...}`, labels → `attributes.labels`; clickup reactions/tags likewise. No per-project config declares these — the adapter knows its source's shape and emits attributes on read. They are read-only mirrors; lazyspec does not write reactions back.

### Tiers, not either/or

Two axes were being conflated: *write mechanism* and *sharing transport*. Separate them.

| Tier | Store | Role |
|---|---|---|
| Live | git-ref (local) | agent chatter, CAS, conflict-free, ephemeral |
| Durable | filesystem markdown, coarse commit | resolved threads, PR-reviewable, human-facing |
| Shared | commit+push, or remote adapter | opt-in; refs do not cross GitHub, so share = materialized markdown or native issue comment |

`lazyspec comment materialize <doc>` folds a ref-backed stream into committed markdown when a thread resolves or on demand — cheap live posting, durable reviewable artifact. Which store is live vs durable is configuration, not a hard-coded policy.

### Control loop this enables (external orchestrator)

```
poll frontier (ready docs) → pick (priority) → dispatch KS agent
  → agent claims doc (lease) → reads state + thread → posts observations/findings
  → advances status or appends decision → releases
→ repeat until frontier empty (quiescence)
```

lazyspec supplies state + frontier + safe writes + the comment channel. The orchestrator supplies scheduling and agent lifecycle.

## Interfaces

Proposed CLI (all `@draft`):

```
lazyspec comment add <doc> --attr kind=observation --attr confidence=0.7 \
        [--reply-to <id>] [--body <..> | --body-file -]
lazyspec comments <doc> [--json]              # folded thread tree
lazyspec comments <doc> --since <rev> [--json] # change detection for control loop
lazyspec comment resolve <thread-root>         # appends the configured resolution attribute
lazyspec comment materialize <doc>             # fold live ref stream → committed markdown
```

`--attr key=value` is repeatable and generic; the engine validates each pair against the project's attribute schema (unknown key, bad type, or out-of-range value → error with `--json` diagnostics). No per-attribute flag is compiled in, so adding an attribute to config needs no CLI change.

**Output modes.** `comments <doc>` renders two ways from the same folded tree: `--json` (machine-readable — envelope + full attribute map per comment, including adapter-sourced reactions/labels) and the default **pretty** print (indented thread tree, author + relative time + attribute chips per node, reply nesting shown by indentation). Per convention principle 2, the pretty and JSON views are the same data, one formatted for terminals and one for agents.

Config (`.lazyspec.toml`):

- `comment_store` — project-default plus optional per-type override, values `filesystem | git-ref | github-issues | clickup`.
- A `[comments.attributes]` schema declaring each attribute: `type` (`enum` / `number` / `string` / `list`), allowed values or range, `default`, `required`. Ships with a default schema (`kind`, `confidence`, `anchor`) that a project can override wholesale — mirroring how document `[[types]]` are configured, not hard-coded.
- `resolution` — the predicate that marks a thread resolved (default `kind == decision`), so the fold stays config-driven.

Engine: `CommentStore` trait + `Comment` type (envelope + opaque attribute map); attribute-schema validation (authored path only) and thread-fold logic in engine, store-agnostic. TUI: a **thread/chat-like pane** per document — nested reply bubbles, author + timestamp headers, attribute chips (kind/confidence and any adapter-sourced reactions), collapse-resolved, jump-to-anchor; posting a reply inline. Web view: same tree rendered. All surfaces are attribute-driven, with no hard-coded slot for `kind` or `confidence` — they render whatever attributes each comment carries, authored or remote. All three move together (per CLAUDE.md).

## Decisions (ADRs to emit)

- **ADR: comment attributes are project-configurable, not hard-coded** — envelope (id/author/created_at/in_reply_to/refs) is engine-owned; blackboard vocabulary (`kind`, `confidence`, `anchor`, resolution predicate) lives in a config schema with shipped defaults. Rationale: mirrors document `[[types]]`; the engine stays domain-agnostic; new attributes need no code or CLI change.
- **ADR: comments are immutable append-only** — event-sourced; derived thread/resolution state. Rationale: conflict-free concurrency + preserved reasoning trail.
- **ADR: ULID identifiers for comments** — time-sortable, coordination-free ordering across agents; accept wall-clock drift (causal order via `in_reply_to` is authoritative).
- **ADR: comment storage behind a configurable trait** — filesystem and git-ref first; remote adapters thin. Rationale: one axis below existing per-type store; Unix single-responsibility.
- **ADR: git-ref store for live tier, materialize to markdown for durable/shared** — names the GitHub custom-ref-push limitation explicitly.

## Stories

1. `Comment` type (envelope + opaque attribute map) + `CommentStore` trait + filesystem (maildir) impl + fold logic. Engine + `comment add --attr` / `comments` CLI with both `--json` and pretty thread-tree output.
2. Attribute-schema config (`[comments.attributes]` + `resolution`) with shipped defaults; validation at the `append` boundary; config-driven resolution fold.
3. Git-ref `CommentStore` impl (blob + namespaced ref, CAS on `update-ref`). Reuse reservation-ref machinery.
4. `comment_store` config wiring; project default + per-type override.
5. `--since` change-detection cursor for the orchestrator control loop.
6. `comment resolve` + `comment materialize` (ref → committed markdown).
7. github-issues + clickup adapters. Native comment ↔ `Comment`; reactions/labels/tags projected onto the attribute map dynamically (read-only, no config).
8. TUI thread/chat-like pane + web-view thread tree, both attribute-driven (render authored + adapter-sourced attributes; no hard-coded columns).

## Risks and tradeoffs

- **Git-ref opacity.** Live comments are not files on disk; readable only through lazyspec. Mitigated by the durable filesystem tier + `materialize`. Accept: live lane trades human-readability for speed and CAS.
- **Cross-host sharing of refs.** GitHub rejects pushes to `refs/lazyspec/*`. Accept: cross-GitHub sharing goes through materialized markdown or native issue-comment adapters, not raw refs.
- **Clock drift across agents.** ULID absolute order is approximate; causal order (`in_reply_to`) is authoritative. Accept — sufficient for blackboard reasoning.
- **Unbounded growth.** Append-only streams grow without bound. GC/archival of resolved threads deferred past v1.
- **Anchor stability.** Line anchors break on edit; section-slug anchors survive better. Accept slug granularity; whole-doc anchor allowed.
- **Configurable-attribute cost.** Generic attributes mean the engine can't rely on `kind`/`confidence` existing, and remote adapters must map native fields onto whatever the schema declares. Mitigated by shipping a sensible default schema and validating at `append`; the flexibility matches how document `[[types]]` already work, so it is consistent, not novel surface area.
- **Scope creep.** This must stay a doc primitive, not become an orchestration engine. The non-goals fence this: no scheduler, no runner, no push daemon.

