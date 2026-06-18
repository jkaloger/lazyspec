---
title: Document Context Chain
type: spec
status: draft
author: jkaloger
date: 2026-03-25
tags:
- cli
- context
- relationships
related:
- related-to: STORY-019
- related-to: STORY-054
---


## Summary

The `lazyspec context` command resolves a document's full lineage by walking `implements` relationships upward and collecting forward implementors and related documents. The result is a `ResolvedContext` describing the backward ancestor *set* as a directed graph, the forward implementors, the related records, and an explicit reference to the requested `target` document.

@ref src/cli/context.rs#ResolvedContext

The backward ancestor set is a list of `ContextNode` entries, each carrying its in-graph `implements` edges (the resolved paths of the parents it points at). This makes the lineage an addressable graph rather than a positional chain: there is no `chain` vector and no `target_index`, only the node set, its edges, and the explicit `target`. Two output modes render this structure for human and machine consumers.

@ref src/cli/context.rs#ContextNode

## Chain Resolution

@ref src/cli/context.rs#resolve_chain

`resolve_chain` accepts a `Store` and a shorthand ID string. It first calls `resolve_shorthand` to locate the target document, mapping `ResolveError` variants into `anyhow` errors. For `NotFound`, the message is `"document not found: {id}"`. For `Ambiguous`, the message lists all matching paths and prompts the user to specify the full path.

@ref src/engine/store.rs#resolve_shorthand

From the resolved document, the function performs a breadth-first traversal upward over *all* `implements` relations, not just a single parent. Each node's `related` entries are inspected for `Implements` relations, and every relation whose `target` path resolves through `Store::get` contributes a DAG edge to that parent; the parent is then enqueued for its own ancestor walk. A parent listed more than once on a single node contributes a single edge. This allows a document to implement multiple parents, so the lineage is a directed acyclic graph rather than a linear chain. Parents are followed only when `Store::get` resolves the relation target; unresolvable targets are skipped.

A `HashSet<PathBuf>` seen-set governs the traversal. It serves two purposes: it deduplicates shared ancestors so a diamond (two paths converging on the same ancestor) visits that ancestor only once, and it guards against cycles — a node that has already been seen is not re-enqueued, so cyclic `implements` input terminates instead of looping forever.

The discovered nodes are emitted in a deterministic root-first topological order. A node appears only after all of its in-graph parents, with ready nodes broken by path for stability. For a single-parent graph this reduces to the old chain order (root first, requested document last). Cyclic input has no valid topological order, so once no node has all its parents satisfied the remaining nodes are appended path-ordered; the node set is always complete, with each node present exactly once.

@ref src/cli/context.rs#topo_order

The `target` field is an explicit reference to the originally-requested document, replacing the old positional `target_index`. Renderers and consumers identify the requested document by matching `doc.path` against `target.path` rather than relying on a position in a chain.

## Forward Context

After building the backward node set, `resolve_chain` collects forward context: documents that implement the target. It queries `Store.reverse_links` for the target's path, filters to entries where the relation type is `Implements`, and resolves each source path through the store. Only direct implementors of the target are included; the traversal does not recurse transitively.

@ref src/engine/store.rs#Store

## Related Records

Related records are gathered by a breadth-first traversal over `related-to` links, bounded by the `--depth N` flag (default `1`). Frontier 0 is the set of context documents in `nodes`. Each hop examines the current frontier's `related-to` links in *both* directions — `Store.forward_links` and `Store.reverse_links`, selecting entries with `RelationType::RelatedTo` — and follows them to undiscovered documents. A document discovered at hop `h` is recorded with `distance = h` and `via = <path of the document it was reached through>`, and joins the next frontier. The traversal stops after `depth` hops, so nothing more than `depth` `related-to` links away from the chain appears.

Documents already present among the context nodes are excluded from related records entirely; they are never recorded regardless of distance. Dedup is by path with first-discovery-wins semantics: a document reachable by several routes is recorded exactly once, at the *shortest* hop count, so its `distance` is always the minimum number of `related-to` links from the chain (a document reachable at both 1 and 2 hops is recorded once at distance 1). The traversal is deterministic: the frontier and each node's neighbours are path-sorted before expansion, so on a same-hop tie the lexicographically-smallest `via` wins.

With the default `depth == 1` this reduces exactly to the prior behaviour — the direct `related-to` neighbours of the context documents, each at distance 1 — so the default is backward compatible. Every related entry carries three tags alongside its frontmatter: `relation` (always `"related-to"`), `distance` (the shortest hop count), and `via` (the path of the document it was reached through). See [JSON Output](#json-output) for the serialized shape.

@ref src/engine/document.rs#RelationType

## Human Output

@ref src/cli/context.rs#run_human

`run_human` chooses one of two render modes based on the graph shape. When every node has at most one in-graph parent the lineage is linear and `render_stack` draws the existing vertical stack of mini-cards, backward compatible with the previous output. When some node has more than one parent the lineage is a DAG and `render_tree` draws an indented tree instead.

@ref src/cli/context.rs#render_stack

@ref src/cli/context.rs#render_tree

Each node is rendered by `mini_card`, which draws a bordered box containing the document title and a line with the uppercased shorthand ID, lowercase doc type, and status in brackets. When `colors_enabled()` returns true, the box uses Unicode box-drawing characters and the status receives colour styling via `styled_status`. When colours are disabled, the box falls back to ASCII (`+`, `-`, `|`).

@ref src/cli/context.rs#mini_card

The target document's mini-card receives a single `"<- you are here"` marker appended to its title line. The marker is driven by comparing each node's `doc.path` against `resolved.target.path`, not by a positional index, so exactly one card is marked regardless of render mode.

In the linear stack, `chain_connector` separates cards with a vertical pipe character, and below each card the forward children are listed via `push_card_children` — indented lines with tree connectors (`├─` / `└─`) showing each child's shorthand, title, and status.

@ref src/cli/context.rs#chain_connector

@ref src/cli/context.rs#push_card_children

In the DAG tree, the graph roots (nodes with no in-graph parents) are drawn first, and children descend along the `implements` edges with increasing indentation. Roots and each node's children are path-sorted for determinism. Each node is drawn exactly once: a node reachable by multiple paths (a diamond's shared node) is drawn in full on first encounter, and on every subsequent encounter it renders as a one-line shorthand reference (`↳ <ID> (see above)`) without recursing into its subtree. The `<- you are here` marker is never placed on a shorthand reference. Cyclic input can leave a strongly-connected component with no root; after the root traversal any still-undrawn node is drawn as a depth-0 subtree in node order, so every node appears even when no root reaches it.

After the nodes, forward implementors are rendered in the same tree-connector style in both modes, preceded by a chain connector. If the forward list is empty, this section is omitted entirely.

Related records appear after a blank line and a `"--- related ---"` separator (or its Unicode equivalent when colours are enabled). Each related document is printed as `SHORTHAND  Title [status]`. Entries at distance 1 render exactly as before, unannotated. Entries discovered beyond depth 1 receive a trailing ` (via SHORTHAND, dN)`, where `SHORTHAND` is the uppercased shorthand ID of the `via` document (its raw path when the shorthand cannot be resolved) and `N` is the distance — for example `STORY-054  Forward and Backward Context with Related Records [accepted] (via SPEC-012, d2)`. The forward section is rendered unchanged and carries no distance annotation. The section is omitted when there are no related records.

## JSON Output

@ref src/cli/context.rs#run_json

`run_json` serializes the resolved context into a JSON object with four top-level keys: `chain`, `forward`, `related`, and `target`.

- `chain` is an array of node objects in `nodes` (topological) order. Each is produced by `doc_to_json_with_family` — full frontmatter fields plus any children and parent information — with an added `implements_in_context` array carrying that node's in-graph `implements` edge targets as path strings. A root or ancestor with no in-graph parents carries an empty `[]`. This lets a consumer reconstruct the DAG directly from `chain` and its edges without re-walking the store.
- `forward` is an array of direct forward implementors. It is always present and is an empty array `[]` when nothing implements the target, giving a stable schema.
- `related` is an array of the related documents discovered by the depth-bounded BFS.
- `target` is the requested document's path as a string (a relative path such as `docs/...md`). Consumers locate the requested document within `chain` by matching `target` against each element's `path`, replacing the old positional `target_index`.

Entries in `forward` and `related` share one schema: the full frontmatter object produced by `doc_to_json_with_family`, plus three tag keys describing how the document was reached:

- `relation` — string, the link type: `"implements"` for `forward` entries, `"related-to"` for `related` entries.
- `distance` — number, the hop count from the chain. For `forward` this is always `1`. For `related` it is the shortest number of `related-to` links from the chain (see [Related Records](#related-records)).
- `via` — string, the path of the document the entry was reached through. For `related` this is the neighbouring document that linked to it. For `forward` it is the *target document's own path* — the document `context` was run on — since forward implementors are reached "through" the target they implement; this is expected, not a bug.

A `related` entry therefore looks like:

```json
{
  "title": "Forward and Backward Context with Related Records",
  "type": "story",
  "status": "accepted",
  "path": "docs/stories/STORY-054-forward-and-backward-context-with-related-records.md",
  "relation": "related-to",
  "distance": 2,
  "via": "docs/specs/SPEC-012-document-context-chain.md"
}
```

and a `forward` entry carries the same three keys with `"relation": "implements"`, `"distance": 1`, and `via` set to the target's path. (Frontmatter fields above are abbreviated; the real object also carries `author`, `date`, `tags`, `related`, and family information.)

@ref src/cli/json.rs#doc_to_json_with_family

The order of entries within `forward` and within each `implements_in_context` array is not a guaranteed contract — only their membership is. The DAG is fully reconstructable from `chain` plus the per-node `implements_in_context` edges, and `target` identifies which node was requested. Forward implementors are now surfaced in JSON via the `forward` key, no longer only in the human render.
