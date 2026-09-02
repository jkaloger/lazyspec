use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::{ResolveError, Store};
use crate::engine::traversal::{chain_children, related_neighbours};
use anyhow::Result;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;

pub struct ContextNode<'a> {
    pub doc: &'a DocMeta,
    pub parents: Vec<PathBuf>,
    /// True when anchoring INVERTED this node's parent edges: the node is a chain
    /// ancestor of every doc in `parents`, re-parented under it so an anchored
    /// pivot reads top-down in traversal order (STORY-247). Anchoring puts a doc on
    /// the descendant side of the anchors or the ancestor side, never both, so the
    /// flag covers ALL of `parents` or none of them — one bool, not a per-edge set.
    /// Always false for the unanchored forest and for `resolve_chain`, where every
    /// edge points the declared way.
    pub parents_inverted: bool,
}

/// A document surfaced alongside the chain, tagged with how it was reached:
/// the link type, the hop count from the chain (1 = directly adjacent), and the
/// path of the chain/frontier doc it was reached through.
pub struct RelatedRef<'a> {
    pub doc: &'a DocMeta,
    pub relation: RelationType,
    pub distance: usize,
    pub via: PathBuf,
}

pub struct ResolvedContext<'a> {
    pub target: &'a DocMeta,
    pub nodes: Vec<ContextNode<'a>>,
    pub forward: Vec<RelatedRef<'a>>,
    pub related: Vec<RelatedRef<'a>>,
}

pub fn resolve_chain<'a>(store: &'a Store, id: &str, depth: usize) -> Result<ResolvedContext<'a>> {
    let doc = store
        .resolve_shorthand(id)
        .map_err(|e| match e {
            ResolveError::NotFound(id) => anyhow::anyhow!("document not found: {}", id),
            ResolveError::Ambiguous { id, matches } => {
                let paths: Vec<String> = matches.iter().map(|m| m.to_string_lossy().to_string()).collect();
                anyhow::anyhow!("Ambiguous ID '{}' matches multiple documents:\n  {}\nSpecify the full path to show a specific document.", id, paths.join("\n  "))
            }
        })?;

    // BFS upward over the configured parent-child relationships. The
    // seen-set both dedups shared ancestors (diamonds) and guards against
    // cycles (re-entering a node).
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<&DocMeta> = VecDeque::new();
    let mut discovered: HashMap<PathBuf, &DocMeta> = HashMap::new();
    let mut node_parents: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    seen.insert(doc.path.clone());
    queue.push_back(doc);
    discovered.insert(doc.path.clone(), doc);

    while let Some(current) = queue.pop_front() {
        let mut parents: Vec<PathBuf> = Vec::new();
        for rel in &current.related {
            let Some(parent) = store.resolve_relation_target(&rel.target) else {
                continue;
            };
            if !store.traversal_walk.walks_chain(
                current.doc_type.as_str(),
                rel.rel_type.as_str(),
                parent.doc_type.as_str(),
            ) {
                continue;
            }
            if !parents.contains(&parent.path) {
                parents.push(parent.path.clone());
            }
            if seen.insert(parent.path.clone()) {
                discovered.insert(parent.path.clone(), parent);
                queue.push_back(parent);
            }
        }
        node_parents.insert(current.path.clone(), parents);
    }

    let nodes = topo_order(&discovered, &node_parents);

    // Forward context: the target's chain children, each one hop out and
    // reached through the target.
    let target_path = doc.path.clone();
    let forward: Vec<RelatedRef> = chain_children(store, &target_path)
        .into_iter()
        .map(|child| RelatedRef {
            doc: child.doc,
            relation: child.relation.clone(),
            distance: 1,
            via: target_path.clone(),
        })
        .collect();

    // Related: BFS over the configured related relationships (both directions),
    // bounded by `depth`. Hop 0's frontier is the chain/DAG nodes; each
    // subsequent hop expands one ring of related neighbours. First discovery
    // wins, so a doc's recorded `distance` is its shortest hop count. Frontiers
    // and neighbours are processed in path order, so when a doc is reachable
    // from two frontier docs at the same hop the lexicographically-smallest
    // `via` wins (deterministic, test-stable).
    let chain_paths: HashSet<PathBuf> = nodes.iter().map(|n| n.doc.path.clone()).collect();
    let mut related_seen: HashSet<PathBuf> = HashSet::new();
    let mut related: Vec<RelatedRef> = Vec::new();

    let mut frontier: Vec<PathBuf> = chain_paths.iter().cloned().collect();
    frontier.sort();

    for hop in 1..=depth {
        let mut next_frontier: Vec<PathBuf> = Vec::new();

        for from in &frontier {
            let mut neighbours = related_neighbours(store, from);
            neighbours.sort_by(|a, b| a.doc.path.cmp(&b.doc.path));

            for neighbour in neighbours {
                let path = &neighbour.doc.path;
                if chain_paths.contains(path) || !related_seen.insert(path.clone()) {
                    continue;
                }
                related.push(RelatedRef {
                    doc: neighbour.doc,
                    relation: neighbour.relation.clone(),
                    distance: hop,
                    via: from.clone(),
                });
                next_frontier.push(path.clone());
            }
        }

        frontier = next_frontier;
        if frontier.is_empty() {
            break;
        }
    }

    Ok(ResolvedContext {
        target: doc,
        nodes,
        forward,
        related,
    })
}

/// Merge the target's own declared relations into `resolved.related`, so
/// relations whose type carries no traversal marker still surface (BUG-013).
/// Chain-typed relations and targets already on the chain stay excluded --
/// they belong to the chain/forward sections -- and entries the related BFS
/// already found dedupe on (relation type, target path).
///
/// All five surfaces that render a neighbourhood call this immediately after
/// [`resolve_chain`] -- `cli::context`'s JSON and human renders, the TUI relations
/// tab, the web doc page, and the agent prompt's `context.related` -- and none of
/// them may skip it: a surface that also displays the raw `doc.related`
/// frontmatter row is not thereby covered, because that row is a verbatim list of
/// declared links (chain ones included) and not the neighbourhood the walk names.
pub fn merge_declared_related<'a>(store: &'a Store, resolved: &mut ResolvedContext<'a>) {
    let chain_paths: HashSet<&PathBuf> = resolved.nodes.iter().map(|n| &n.doc.path).collect();
    let mut seen: HashSet<(String, PathBuf)> = resolved
        .related
        .iter()
        .map(|r| (r.relation.to_string(), r.doc.path.clone()))
        .collect();

    let target = resolved.target;
    let declared: Vec<RelatedRef<'a>> = target
        .related
        .iter()
        .filter_map(|rel| store.resolve_relation_target(&rel.target).map(|d| (rel, d)))
        .filter(|(rel, d)| {
            !store.traversal_walk.walks_chain(
                target.doc_type.as_str(),
                rel.rel_type.as_str(),
                d.doc_type.as_str(),
            )
        })
        .filter(|(_, d)| !chain_paths.contains(&d.path))
        .filter(|(rel, d)| seen.insert((rel.rel_type.to_string(), d.path.clone())))
        .map(|(rel, d)| RelatedRef {
            doc: d,
            relation: rel.rel_type.clone(),
            distance: 1,
            via: target.path.clone(),
        })
        .collect();

    resolved.related.extend(declared);
}

/// Context forest, roots-first. When `anchor` is `None`, discovers every
/// document and, for each, its in-graph parents (resolved via
/// [`Store::resolve_relation_target`] over the configured parent-child
/// relationships), then orders the full DAG with [`topo_order`] so each
/// multi-parent node appears after all its parents and exactly once. Roots
/// (docs with no resolvable in-graph parent) come first.
///
/// When `anchor` is `Some(type)`, the forest is re-rooted on documents whose
/// `doc_type` matches `type`: roots become the anchor-type docs, with their
/// chain-descendant subtrees nested below them and their chain ancestors emitted
/// as INVERTED subtrees below them too (see [`resolve_forest_anchored`]). A doc
/// on neither side of an anchor's lineage is pruned. A descendant reachable from
/// two anchor-type docs is retained once with both anchor-side parents, so it
/// appears under each anchor in a tree render without looping; a shared ancestor
/// behaves the same way with inverted edges (the tree render then re-walks the
/// repeated ancestor, so each anchor shows the whole lineage above it — see
/// [`crate::engine::graph::flatten_forest`]).
///
/// Parents are sourced from each doc's own declared `related`, NOT from
/// `forward_links`: `propagate_parent_links` copies a parent's forward links
/// onto nested child docs, so reading `forward_links` would over-collect
/// inherited parent-child edges (the same trap `resolve_chain` avoids). Every
/// in-graph parent is retained on the node so multi-parent edges survive.
pub fn resolve_forest<'a>(store: &'a Store, anchor: Option<&str>) -> Vec<ContextNode<'a>> {
    let all_parents = chain_parents(store);

    let Some(anchor_type) = anchor else {
        let discovered: HashMap<PathBuf, &DocMeta> =
            store.docs.values().map(|d| (d.path.clone(), d)).collect();
        return topo_order(&discovered, &all_parents);
    };

    let anchor_roots: Vec<PathBuf> = store
        .docs
        .values()
        .filter(|d| d.doc_type.as_str() == anchor_type)
        .map(|d| d.path.clone())
        .collect();
    resolve_forest_anchored(store, &all_parents, anchor_roots)
}

/// Context forest re-rooted on documents carrying `tag`: roots become the
/// tag-bearing docs, with their chain descendants and inverted chain ancestors
/// below them — [`resolve_forest`]'s type anchor with a tag predicate, sharing the
/// same [`resolve_forest_anchored`] traversal. Used by the graph view's tag pivots.
pub fn resolve_forest_by_tag<'a>(store: &'a Store, tag: &str) -> Vec<ContextNode<'a>> {
    let all_parents = chain_parents(store);
    let anchor_roots: Vec<PathBuf> = store
        .docs
        .values()
        .filter(|d| d.tags.iter().any(|t| t == tag))
        .map(|d| d.path.clone())
        .collect();
    resolve_forest_anchored(store, &all_parents, anchor_roots)
}

/// Each doc's in-graph parents over the configured chain relationships. Shared
/// by the whole-store and anchored forest builders.
fn chain_parents(store: &Store) -> HashMap<PathBuf, Vec<PathBuf>> {
    store
        .docs
        .values()
        .map(|doc| {
            let mut parents: Vec<PathBuf> = Vec::new();
            for rel in &doc.related {
                let Some(parent) = store.resolve_relation_target(&rel.target) else {
                    continue;
                };
                if !store.traversal_walk.walks_chain(
                    doc.doc_type.as_str(),
                    rel.rel_type.as_str(),
                    parent.doc_type.as_str(),
                ) {
                    continue;
                }
                if !parents.contains(&parent.path) {
                    parents.push(parent.path.clone());
                }
            }
            (doc.path.clone(), parents)
        })
        .collect()
}

/// Re-root the forest on `anchor_roots`: BFS down the chain edges from each
/// anchor doc, keep the union of descendant subtrees, and retain only the parent
/// edges that stay inside that subtree so the anchors surface as roots. A
/// descendant reachable from two anchors is kept once with both anchor-side
/// parents.
///
/// The extent is then widened UPWARD (STORY-247): each anchor's chain ancestors
/// are emitted too, with their edges INVERTED — the ancestor becomes a child of
/// the anchor-side node it was reached from — so an anchored pivot reads top-down
/// in traversal order (`ITERATION-246 → STORY-184 → RFC-058`) with the anchor
/// still the root. Without this, pivoting on a leaf type renders a flat list,
/// since leaves have no chain descendants.
///
/// The upward walk starts only at the anchor docs and only re-parents; it never
/// descends from an ancestor, so an ancestor's OTHER children (an anchor's
/// siblings/cousins) stay out of the forest. An ancestor already in the downward
/// subtree keeps its forward edge and is not re-parented, so one doc never carries
/// both directions of the same edge — and the ascent HALTS there rather than
/// stepping over it, so chain parents ABOVE an in-subtree ancestor are left out of
/// the forest entirely. That is not a loss against the old descendants-only extent
/// (which pruned every ancestor), and it only bites where the upward path has
/// re-entered the anchors' own downward subtree — the ancestor it stops at is
/// already on screen as an anchor-side node.
fn resolve_forest_anchored<'a>(
    store: &'a Store,
    all_parents: &HashMap<PathBuf, Vec<PathBuf>>,
    anchor_roots: Vec<PathBuf>,
) -> Vec<ContextNode<'a>> {
    // Child adjacency (parent -> children) over the chain edges, so we can walk
    // downward from each anchor doc.
    let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for (child, parents) in all_parents {
        for parent in parents {
            children
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
        }
    }

    // BFS down from every anchor doc, collecting the union of descendant
    // subtrees. The seen-set dedups diamonds and guards against cycles.
    let mut subtree: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = anchor_roots.iter().cloned().collect();
    for path in &queue {
        subtree.insert(path.clone());
    }
    while let Some(current) = queue.pop_front() {
        if let Some(kids) = children.get(&current) {
            for kid in kids {
                if subtree.insert(kid.clone()) {
                    queue.push_back(kid.clone());
                }
            }
        }
    }

    // BFS up the chain parents from every anchor doc, inverting each edge:
    // `inverted[ancestor]` collects the anchor-side nodes the ancestor was reached
    // from, which become its parents in the emitted forest. Ancestors already in
    // the downward subtree are left alone (their forward edge stands). Every
    // anchor-side node that reaches an ancestor records an edge, so the ancestor
    // renders under each — mirroring a shared descendant keeping both anchor-side
    // parents — but the walk continues upward only on first discovery, so a
    // diamond or a cycle above the anchor is traversed once.
    let mut inverted: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    let mut walked_up: HashSet<PathBuf> = HashSet::new();
    let mut up: VecDeque<PathBuf> = anchor_roots.into_iter().collect();
    while let Some(current) = up.pop_front() {
        let Some(parents) = all_parents.get(&current) else {
            continue;
        };
        for parent in parents {
            if subtree.contains(parent) {
                continue;
            }
            let edges = inverted.entry(parent.clone()).or_default();
            if !edges.contains(&current) {
                edges.push(current.clone());
            }
            if walked_up.insert(parent.clone()) {
                up.push_back(parent.clone());
            }
        }
    }
    // Which anchor reaches an ancestor first depends on `store.docs` iteration
    // order (a HashMap), so path-sort each inverted edge list to keep the emitted
    // parent lists stable across runs. The edge SET is already order-independent:
    // every node is processed exactly once and records an edge to each of its
    // out-of-subtree chain parents.
    for edges in inverted.values_mut() {
        edges.sort();
    }

    let discovered: HashMap<PathBuf, &DocMeta> = store
        .docs
        .values()
        .filter(|d| subtree.contains(&d.path) || inverted.contains_key(&d.path))
        .map(|d| (d.path.clone(), d))
        .collect();

    // A subtree node keeps only the parent edges that stay within the subtree, so
    // the anchors surface as roots; an inverted node's parents are exactly the
    // anchor-side nodes it was re-parented under (its own chain parents are its
    // children here, one hop further up).
    let node_parents: HashMap<PathBuf, Vec<PathBuf>> = discovered
        .keys()
        .map(|path| {
            let parents = match inverted.get(path) {
                Some(edges) => edges.clone(),
                None => all_parents
                    .get(path)
                    .map(|ps| {
                        ps.iter()
                            .filter(|p| subtree.contains(*p))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            (path.clone(), parents)
        })
        .collect();

    let mut ordered = topo_order(&discovered, &node_parents);
    for node in &mut ordered {
        node.parents_inverted = inverted.contains_key(&node.doc.path);
    }
    ordered
}

/// Deterministic topological ordering of the discovered DAG, root-first.
/// `node_parents` holds the parent-child edges (child -> parents). A node is
/// emitted only once all its parents have been emitted; ready nodes are
/// broken by path for determinism. For a single-parent chain this yields the
/// old `chain` order (root first, target last). Cyclic input has no valid
/// topological order, so any remaining nodes are appended path-ordered; the
/// node set is still complete (each node once).
///
/// Kahn's algorithm over a path-ordered ready frontier, O(V + E log V). The
/// frontier is the whole point: an earlier version re-derived the ready set from
/// scratch each step — rescan every indegree, sort the entire ready set to take
/// its minimum, then linear-search every node's parent list to decrement — three
/// O(V) passes per emitted node. That is O(V^2 log V), and since the TUI
/// re-resolves the forest on the UI thread whenever the graph view opens or its
/// pivot/sort changes (`tui::state::app::rebuild_graph`), it showed up as a
/// visible stutter (145ms on a 751-doc store). See
/// `forest_topo_order_is_near_linear_in_forest_size`.
fn topo_order<'a>(
    discovered: &HashMap<PathBuf, &'a DocMeta>,
    node_parents: &HashMap<PathBuf, Vec<PathBuf>>,
) -> Vec<ContextNode<'a>> {
    // Indegree counts each node's DISTINCT in-graph parents and `children` is the
    // matching forward edge list, both built from the same deduped parent set so
    // every counted edge has exactly one decrement. (Callers already dedupe, but
    // deriving both from one set means a repeated parent cannot strand a node
    // above zero and silently divert it to the cycle branch.)
    let mut indegree: HashMap<&PathBuf, usize> = HashMap::with_capacity(discovered.len());
    let mut children: HashMap<&PathBuf, Vec<&PathBuf>> = HashMap::new();
    for path in discovered.keys() {
        let mut parents: Vec<&PathBuf> = Vec::new();
        if let Some(declared) = node_parents.get(path) {
            for parent in declared {
                // Re-borrow the parent path out of `discovered` so the adjacency
                // keys share its lifetime rather than `node_parents`'.
                if let Some((key, _)) = discovered.get_key_value(parent) {
                    if !parents.contains(&key) {
                        parents.push(key);
                    }
                }
            }
        }
        indegree.insert(path, parents.len());
        for parent in parents {
            children.entry(parent).or_default().push(path);
        }
    }

    // A `BTreeSet` frontier makes "smallest ready path" a pop rather than a sort,
    // preserving the exact tiebreak the callers' order assertions pin.
    let mut frontier: BTreeSet<&PathBuf> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(path, _)| *path)
        .collect();

    let mut ordered: Vec<&PathBuf> = Vec::with_capacity(discovered.len());
    while let Some(next) = frontier.pop_first() {
        ordered.push(next);
        for child in children.get(next).into_iter().flatten() {
            let degree = indegree
                .get_mut(*child)
                .expect("children are keyed from discovered");
            *degree -= 1;
            if *degree == 0 {
                frontier.insert(*child);
            }
        }
    }

    // Cyclic input has no valid topological order. A node is emitted exactly when
    // its indegree reaches zero, so the nodes still holding a positive indegree
    // are precisely the ones the drain never reached: append them path-ordered so
    // the node set stays complete.
    if ordered.len() < discovered.len() {
        let mut leftover: Vec<&PathBuf> = indegree
            .iter()
            .filter(|(_, degree)| **degree > 0)
            .map(|(path, _)| *path)
            .collect();
        leftover.sort();
        ordered.extend(leftover);
    }

    ordered
        .into_iter()
        .map(|path| ContextNode {
            doc: discovered[path],
            parents: node_parents.get(path).cloned().unwrap_or_default(),
            parents_inverted: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        Config, EdgeDef, RelSelector, RelationshipDef, Traversal, TypeSelector,
    };
    use crate::engine::store::test_support::{
        doc_md, store_from_with_config, stories_mention_rfcs, write_docs,
    };
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    /// [`store_from_with_config`] under the starter config, for the tests that
    /// do not pin traversal markers of their own.
    fn store_from(files: &[(&str, &str)]) -> (TempDir, Store) {
        store_from_with_config(files, &Config::default())
    }

    /// Set of doc ids in a node slice, for order-insensitive membership asserts.
    fn node_ids(nodes: &[ContextNode]) -> BTreeSet<String> {
        nodes.iter().map(|n| n.doc.id.clone()).collect()
    }

    /// The parents of the node with the given id, as a set of doc ids.
    fn parents_of(nodes: &[ContextNode], id: &str) -> BTreeSet<String> {
        let node = nodes
            .iter()
            .find(|n| n.doc.id == id)
            .unwrap_or_else(|| panic!("node {id} not in chain"));
        ids_of_paths(&node.parents)
    }

    /// The parents of the node with the given id, as doc ids IN EMITTED ORDER, for
    /// asserting the determinism of a parent list.
    fn ordered_parents_of(nodes: &[ContextNode], id: &str) -> Vec<String> {
        let node = nodes
            .iter()
            .find(|n| n.doc.id == id)
            .unwrap_or_else(|| panic!("node {id} not in chain"));
        ids_in_order(&node.parents)
    }

    /// Whether anchoring inverted the parent edges of the node with the given id.
    fn parents_inverted_of(nodes: &[ContextNode], id: &str) -> bool {
        nodes
            .iter()
            .find(|n| n.doc.id == id)
            .unwrap_or_else(|| panic!("node {id} not in chain"))
            .parents_inverted
    }

    fn ids_of_paths(paths: &[PathBuf]) -> BTreeSet<String> {
        ids_in_order(paths).into_iter().collect()
    }

    fn ids_in_order(paths: &[PathBuf]) -> Vec<String> {
        paths.iter().map(|p| id_of_path(p)).collect()
    }

    fn id_of_path(path: &std::path::Path) -> String {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        crate::engine::store::extract_id_from_name(&stem)
    }

    // --- resolve_chain -----------------------------------------------------

    #[test]
    fn chain_linear_orders_root_first_target_last() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();
        let ids: Vec<&str> = resolved.nodes.iter().map(|n| n.doc.id.as_str()).collect();

        assert_eq!(ids, vec!["RFC-001", "STORY-001", "ITERATION-001"]);
        assert_eq!(resolved.target.id, "ITERATION-001");
    }

    #[test]
    fn chain_diamond_dedups_shared_ancestor() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-left.md",
                &doc_md("Left", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-002-right.md",
                &doc_md("Right", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: STORY-001\n- implements: STORY-002",
                ),
            ),
        ]);

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();

        assert_eq!(
            node_ids(&resolved.nodes),
            BTreeSet::from([
                "RFC-001".to_string(),
                "STORY-001".to_string(),
                "STORY-002".to_string(),
                "ITERATION-001".to_string(),
            ]),
            "shared ancestor RFC-001 should appear exactly once"
        );
        // Each node appears once (no duplicates).
        assert_eq!(resolved.nodes.len(), 4);
    }

    #[test]
    fn chain_retains_all_parents_of_multiparent_node() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: RFC-001\n- implements: RFC-002",
                ),
            ),
        ]);

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();

        assert_eq!(
            parents_of(&resolved.nodes, "ITERATION-001"),
            BTreeSet::from(["RFC-001".to_string(), "RFC-002".to_string()]),
            "multi-parent node should list both in-graph parents"
        );
    }

    #[test]
    fn chain_terminates_on_cycle_each_node_once() {
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-a.md",
                &doc_md("A", "rfc", "- implements: RFC-002"),
            ),
            (
                "docs/rfcs/RFC-002-b.md",
                &doc_md("B", "rfc", "- implements: RFC-001"),
            ),
        ]);

        let resolved = resolve_chain(&store, "RFC-001", 1).unwrap();

        assert_eq!(
            node_ids(&resolved.nodes),
            BTreeSet::from(["RFC-001".to_string(), "RFC-002".to_string()]),
            "cycle should terminate with each node present exactly once"
        );
        assert_eq!(resolved.nodes.len(), 2);
    }

    #[test]
    fn chain_related_bfs_records_shortest_hop_distance_and_depth_bounds() {
        // Chain anchor RFC-001 -related-to-> RFC-002 -related-to-> RFC-003.
        // RFC-003 sits 2 hops out; RFC-004 sits 3 hops out.
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-anchor.md",
                &doc_md("Anchor", "rfc", "- related-to: RFC-002"),
            ),
            (
                "docs/rfcs/RFC-002-near.md",
                &doc_md("Near", "rfc", "- related-to: RFC-003"),
            ),
            (
                "docs/rfcs/RFC-003-far.md",
                &doc_md("Far", "rfc", "- related-to: RFC-004"),
            ),
            (
                "docs/rfcs/RFC-004-furthest.md",
                &doc_md("Furthest", "rfc", "[]"),
            ),
        ]);

        let depth2 = resolve_chain(&store, "RFC-001", 2).unwrap();
        let by_id = |refs: &[RelatedRef], id: &str| -> Option<usize> {
            refs.iter().find(|r| r.doc.id == id).map(|r| r.distance)
        };

        assert_eq!(by_id(&depth2.related, "RFC-002"), Some(1));
        assert_eq!(
            by_id(&depth2.related, "RFC-003"),
            Some(2),
            "RFC-003 is reachable at shortest 2 hops"
        );
        assert_eq!(
            by_id(&depth2.related, "RFC-004"),
            None,
            "depth=2 must exclude the 3-hop doc"
        );

        let depth3 = resolve_chain(&store, "RFC-001", 3).unwrap();
        assert_eq!(by_id(&depth3.related, "RFC-004"), Some(3));
    }

    #[test]
    fn chain_resolves_shorthand_implements_target() {
        // ITERATION-001 implements RFC-001 by shorthand id (not a path); the
        // RFC must surface in the chain (task-2 resolve_relation_target fix).
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: RFC-001"),
            ),
        ]);

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();

        assert!(
            resolved.nodes.iter().any(|n| n.doc.id == "RFC-001"),
            "shorthand implements target should resolve into the chain"
        );
    }

    #[test]
    fn chain_resolves_path_implements_target() {
        // Same as above but the implements target is a full path, not an id.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: docs/rfcs/RFC-001-base.md",
                ),
            ),
        ]);

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();

        assert!(
            resolved.nodes.iter().any(|n| n.doc.id == "RFC-001"),
            "path implements target should resolve into the chain"
        );
    }

    // Verify item 2: a Chain relationship forms the chain purely from its
    // `traversal` marker, with NO ParentChild rule declaring the link.
    #[test]
    fn chain_forms_from_traversal_marker_without_validation_rule() {
        let mut config = Config::default();
        config.rules.clear();
        assert!(
            config.relationship_by_name("implements").unwrap().traversal == Some(Traversal::Chain),
            "implements is Chain by marker"
        );

        let (_tmp, store) = store_from_with_config(
            &[
                ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
                (
                    "docs/iterations/ITERATION-001-leaf.md",
                    &doc_md("Leaf", "iteration", "- implements: RFC-001"),
                ),
            ],
            &config,
        );

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();
        assert_eq!(
            node_ids(&resolved.nodes),
            BTreeSet::from(["RFC-001".to_string(), "ITERATION-001".to_string()]),
            "chain forms from the traversal marker even with no validation rule"
        );
    }

    // Verify item 3: a config with NO traversal markers yields a target-only
    // context (empty chain + empty related), no panic.
    #[test]
    fn no_traversal_markers_yields_target_only_context() {
        let mut config = Config::default();
        for rel in &mut config.relationships {
            rel.traversal = None;
        }

        let (_tmp, store) = store_from_with_config(
            &[
                ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
                (
                    "docs/iterations/ITERATION-001-leaf.md",
                    &doc_md("Leaf", "iteration", "- implements: RFC-001"),
                ),
            ],
            &config,
        );

        let resolved = resolve_chain(&store, "ITERATION-001", 3).unwrap();
        assert_eq!(
            node_ids(&resolved.nodes),
            BTreeSet::from(["ITERATION-001".to_string()]),
            "no chain markers => the target stands alone"
        );
        assert!(
            resolved.forward.is_empty(),
            "no chain markers => no forward"
        );
        assert!(
            resolved.related.is_empty(),
            "no related markers => no related"
        );
    }

    /// A config in the arrangement RFC-067 §Problem.3 names: `targets` carries a
    /// global `traversal = "chain"` marker AND one `[[edges]]` row declares it
    /// chain for iteration -> milestone only. The global marker being present is
    /// the point -- an edge row stating a traversal suppresses it, so the row is
    /// the whole story for `targets`.
    fn targets_walks_only_to_milestones() -> Config {
        let mut config = Config::default();

        let mut milestone = config
            .documents
            .types
            .iter()
            .find(|t| t.name == "rfc")
            .expect("starter types declare rfc")
            .clone();
        milestone.name = "milestone".to_string();
        milestone.plural = "milestones".to_string();
        milestone.dir = "docs/milestones".to_string();
        milestone.prefix = "MILESTONE".to_string();
        config.documents.types.push(milestone);

        config.relationships.push(RelationshipDef {
            name: "targets".to_string(),
            inverse: Some("targeted-by".to_string()),
            github_native: None,
            traversal: Some(Traversal::Chain),
        });
        config.edges.push(EdgeDef {
            name: "iterations-target-milestones".to_string(),
            from: TypeSelector::Types(vec!["iteration".to_string()]),
            to: TypeSelector::Types(vec!["milestone".to_string()]),
            via: RelSelector::Named(vec!["targets".to_string()]),
            required: None,
            traversal: Some(Traversal::Chain),
        });
        config
    }

    /// One iteration targets a story and a milestone. STORY-257 AC1: the story's
    /// context does not reach the iteration, because no row declares
    /// iteration --targets--> story. AC2: the milestone's row does walk.
    fn iteration_targeting_story_and_milestone() -> (TempDir, Store) {
        store_from_with_config(
            &[
                (
                    "docs/milestones/MILESTONE-001-launch.md",
                    &doc_md("Launch", "milestone", "[]"),
                ),
                (
                    "docs/stories/STORY-001-mid.md",
                    &doc_md("Mid", "story", "[]"),
                ),
                (
                    "docs/iterations/ITERATION-001-leaf.md",
                    &doc_md(
                        "Leaf",
                        "iteration",
                        "  - targets: STORY-001\n  - targets: MILESTONE-001",
                    ),
                ),
            ],
            &targets_walks_only_to_milestones(),
        )
    }

    // STORY-257 AC1.
    #[test]
    fn chain_excludes_a_targets_link_no_edge_row_declares() {
        let (_tmp, store) = iteration_targeting_story_and_milestone();

        let resolved = resolve_chain(&store, "STORY-001", 1).unwrap();
        let reached: BTreeSet<String> = resolved
            .nodes
            .iter()
            .map(|n| n.doc.id.clone())
            .chain(resolved.forward.iter().map(|r| r.doc.id.clone()))
            .collect();

        assert_eq!(
            reached,
            BTreeSet::from(["STORY-001".to_string()]),
            "no row declares iteration --targets--> story, so the story stands alone"
        );
    }

    // STORY-257 AC2.
    #[test]
    fn chain_includes_the_targets_link_an_edge_row_declares() {
        let (_tmp, store) = iteration_targeting_story_and_milestone();

        let resolved = resolve_chain(&store, "ITERATION-001", 1).unwrap();

        assert_eq!(
            node_ids(&resolved.nodes),
            BTreeSet::from(["ITERATION-001".to_string(), "MILESTONE-001".to_string()]),
            "the declared iteration --targets--> milestone row walks the chain"
        );
    }

    // --- resolve_chain: links a child inherits from its parent -------------

    const CHAIN_PARENT: &str = "docs/stories/STORY-001-parent/index.md";
    const CHAIN_INHERITOR: &str = "docs/stories/STORY-001-parent/ITERATION-001.md";

    /// One chain row with `to` concrete and `from` the caller's, over a parent
    /// that declares `implements: RFC-001` and a nested child of a DIFFERENT
    /// type that declares nothing and inherits the link through
    /// [`Store::propagate_parent_links`]. A concrete `from` is the only shape
    /// that can tell the declaring document from the inheriting one.
    fn chain_parent_declares_child_inherits(from: TypeSelector) -> (TempDir, Store) {
        let config = Config {
            relationships: Vec::new(),
            edges: vec![EdgeDef {
                name: "implements-rfcs".to_string(),
                from,
                to: TypeSelector::Types(vec!["rfc".to_string()]),
                via: RelSelector::Named(vec!["implements".to_string()]),
                required: None,
                traversal: Some(Traversal::Chain),
            }],
            ..Config::default()
        };
        store_from_with_config(
            &[
                (
                    CHAIN_PARENT,
                    &doc_md("Parent", "story", "- implements: RFC-001"),
                ),
                (CHAIN_INHERITOR, &doc_md("Child", "iteration", "[]")),
                ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            ],
            &config,
        )
    }

    /// The ids in a resolved context's `forward` section -- the target's chain
    /// children, the one hop that reads the link maps rather than a doc's own
    /// `related`.
    fn forward_ids(store: &Store, id: &str) -> BTreeSet<String> {
        resolve_chain(store, id, 1)
            .unwrap()
            .forward
            .iter()
            .map(|r| r.doc.id.clone())
            .collect()
    }

    #[test]
    fn forward_asks_an_inherited_chain_link_with_the_declaring_parents_type_as_from() {
        let (_tmp, store) =
            chain_parent_declares_child_inherits(TypeSelector::Types(vec!["story".to_string()]));

        assert_eq!(
            forward_ids(&store, "RFC-001"),
            BTreeSet::from(["STORY-001".to_string(), "ITERATION-001".to_string()]),
            "the parent stated the relation, so the parent's row admits the copy its child inherited"
        );
    }

    #[test]
    fn forward_does_not_ask_an_inherited_chain_link_with_the_inheritors_type_as_from() {
        let (_tmp, store) =
            chain_parent_declares_child_inherits(TypeSelector::Types(
                vec!["iteration".to_string()],
            ));

        assert!(
            forward_ids(&store, "RFC-001").is_empty(),
            "iteration --implements--> rfc is an edge no document declared, from either end"
        );
    }

    /// `related-to` links spanning three types, outbound and inbound, with a
    /// two-hop reach: enough that a wildcard row's `from` and `to` positions
    /// and the BFS's `distance` and `via` are all really being claimed.
    fn related_neighbourhood_files() -> Vec<(&'static str, String)> {
        vec![
            (
                "docs/rfcs/RFC-001-anchor.md",
                doc_md("Anchor", "rfc", "- related-to: STORY-001"),
            ),
            (
                "docs/stories/STORY-001-near.md",
                doc_md("Near", "story", "- related-to: ITERATION-001"),
            ),
            (
                "docs/iterations/ITERATION-001-far.md",
                doc_md("Far", "iteration", "[]"),
            ),
            (
                "docs/adrs/ADR-001-inbound.md",
                doc_md("Inbound", "adr", "- related-to: RFC-001"),
            ),
        ]
    }

    /// Every related entry as (id, distance, relation, id of the doc it was
    /// reached through) in emitted order -- the whole resolved claim, so an
    /// equivalence assertion cannot pass by comparing less than the output.
    fn related_neighbourhood(store: &Store) -> Vec<(String, usize, String, String)> {
        resolve_chain(store, "RFC-001", 2)
            .unwrap()
            .related
            .iter()
            .map(|r| {
                (
                    r.doc.id.clone(),
                    r.distance,
                    r.relation.to_string(),
                    id_of_path(&r.via),
                )
            })
            .collect()
    }

    // STORY-257 AC4.
    #[test]
    fn a_wildcard_related_row_reproduces_the_global_markers_neighbourhood() {
        let owned = related_neighbourhood_files();
        let files: Vec<(&str, &str)> = owned.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let tmp = write_docs(&files);

        let by_marker = Store::load(tmp.path(), &Config::default()).unwrap();

        // The row is the whole story: `related-to`'s global marker is gone, so
        // a neighbourhood can only come from the table.
        let mut row_config = Config::default();
        for rel in &mut row_config.relationships {
            if rel.name == "related-to" {
                rel.traversal = None;
            }
        }
        row_config.edges.push(EdgeDef {
            name: "anything-relates-to-anything".to_string(),
            from: TypeSelector::Any,
            to: TypeSelector::Any,
            via: RelSelector::Named(vec!["related-to".to_string()]),
            required: None,
            traversal: Some(Traversal::Related),
        });
        let by_row = Store::load(tmp.path(), &row_config).unwrap();

        let expected = related_neighbourhood(&by_marker);
        assert_eq!(
            expected.len(),
            3,
            "the fixture must have a neighbourhood to compare"
        );
        assert_eq!(related_neighbourhood(&by_row), expected);
    }

    #[test]
    fn related_bfs_asks_a_reverse_link_in_the_direction_it_was_declared() {
        let (_tmp, store) = store_from_with_config(
            &[
                (
                    "docs/stories/STORY-001-a.md",
                    &doc_md("A", "story", "- mentions: RFC-001"),
                ),
                ("docs/rfcs/RFC-001-b.md", &doc_md("B", "rfc", "[]")),
                (
                    "docs/rfcs/RFC-002-c.md",
                    &doc_md("C", "rfc", "- mentions: STORY-002"),
                ),
                ("docs/stories/STORY-002-d.md", &doc_md("D", "story", "[]")),
            ],
            &stories_mention_rfcs(),
        );

        let neighbours = |id: &str| -> BTreeSet<String> {
            resolve_chain(&store, id, 1)
                .unwrap()
                .related
                .iter()
                .map(|r| r.doc.id.clone())
                .collect()
        };

        assert_eq!(
            neighbours("RFC-001"),
            BTreeSet::from(["STORY-001".to_string()]),
            "reading the link backwards still asks story -mentions-> rfc"
        );
        assert_eq!(
            neighbours("STORY-001"),
            BTreeSet::from(["RFC-001".to_string()])
        );
        assert!(
            neighbours("RFC-002").is_empty(),
            "rfc -mentions-> story is not the declared triple"
        );
        assert!(neighbours("STORY-002").is_empty());
    }

    #[test]
    fn related_bfs_drops_a_neighbour_with_no_document_in_the_store() {
        let (_tmp, store) = store_from(&[(
            "docs/rfcs/RFC-001-a.md",
            &doc_md("A", "rfc", "- related-to: NOPE-001"),
        )]);

        let resolved = resolve_chain(&store, "RFC-001", 1).unwrap();

        assert!(
            resolved.related.is_empty(),
            "a dangling target has no type to ask the triple about, so it is not a neighbour"
        );
    }

    // --- merge_declared_related ---------------------------------------------

    #[test]
    fn merge_declared_related_surfaces_relation_without_traversal_marker() {
        // `blocks` carries no traversal marker in the starter config, so the
        // related BFS drops it; the merge must surface it, rel type intact.
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-anchor.md",
                &doc_md("Anchor", "rfc", "- blocks: RFC-002"),
            ),
            ("docs/rfcs/RFC-002-near.md", &doc_md("Near", "rfc", "[]")),
        ]);

        let mut resolved = resolve_chain(&store, "RFC-001", 1).unwrap();
        assert!(resolved.related.is_empty(), "BFS drops unmarked relations");

        merge_declared_related(&store, &mut resolved);

        assert_eq!(resolved.related.len(), 1);
        assert_eq!(resolved.related[0].doc.id, "RFC-002");
        assert_eq!(resolved.related[0].relation.as_str(), "blocks");
        assert_eq!(resolved.related[0].distance, 1);
    }

    #[test]
    fn merge_declared_related_dedupes_on_relation_type_and_target() {
        // RFC-002 is declared under both `related-to` (already found by the
        // BFS) and `blocks` (unmarked): the merge must not re-add the BFS
        // entry, but the second rel type is its own entry.
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-anchor.md",
                &doc_md("Anchor", "rfc", "- related-to: RFC-002\n- blocks: RFC-002"),
            ),
            ("docs/rfcs/RFC-002-near.md", &doc_md("Near", "rfc", "[]")),
        ]);

        let mut resolved = resolve_chain(&store, "RFC-001", 1).unwrap();
        merge_declared_related(&store, &mut resolved);

        let pairs: Vec<(String, String)> = resolved
            .related
            .iter()
            .map(|r| (r.relation.to_string(), r.doc.id.clone()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("related-to".to_string(), "RFC-002".to_string()),
                ("blocks".to_string(), "RFC-002".to_string()),
            ]
        );
    }

    // BUG-013 under an edge-table config: the roles come from `[[edges]]`, and
    // `blocks` appears in no row, so no triple gives it a role at all. It must
    // still surface at one hop.
    #[test]
    fn merge_declared_related_surfaces_a_relation_no_row_gives_a_role() {
        let mut config = Config::default();
        for rel in &mut config.relationships {
            rel.traversal = None;
        }
        config.edges.push(EdgeDef {
            name: "anything-relates-to-anything".to_string(),
            from: TypeSelector::Any,
            to: TypeSelector::Any,
            via: RelSelector::Named(vec!["related-to".to_string()]),
            required: None,
            traversal: Some(Traversal::Related),
        });

        let (_tmp, store) = store_from_with_config(
            &[
                (
                    "docs/rfcs/RFC-001-anchor.md",
                    &doc_md("Anchor", "rfc", "- blocks: RFC-002"),
                ),
                ("docs/rfcs/RFC-002-near.md", &doc_md("Near", "rfc", "[]")),
            ],
            &config,
        );

        let mut resolved = resolve_chain(&store, "RFC-001", 1).unwrap();
        assert!(
            resolved.related.is_empty(),
            "no row gives `blocks` a role, so the BFS drops it"
        );

        merge_declared_related(&store, &mut resolved);

        assert_eq!(resolved.related.len(), 1);
        assert_eq!(resolved.related[0].doc.id, "RFC-002");
        assert_eq!(resolved.related[0].relation.as_str(), "blocks");
        assert_eq!(resolved.related[0].distance, 1);
    }

    #[test]
    fn merge_declared_related_skips_chain_relations_and_chain_members() {
        // `implements: RFC-001` is a chain relation (already in nodes) and a
        // declared `related-to` pointing at a chain member is skipped too, so
        // the related section stays disjoint from the chain.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md(
                    "Mid",
                    "story",
                    "- implements: RFC-001\n- related-to: RFC-001",
                ),
            ),
        ]);

        let mut resolved = resolve_chain(&store, "STORY-001", 1).unwrap();
        merge_declared_related(&store, &mut resolved);

        assert!(
            resolved.related.is_empty(),
            "chain relations and chain members must not leak into related"
        );
    }

    // --- resolve_forest ----------------------------------------------------

    #[test]
    fn forest_single_root_chain_orders_root_first() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let forest = resolve_forest(&store, None);
        let ids: Vec<&str> = forest.iter().map(|n| n.doc.id.as_str()).collect();

        assert_eq!(ids, vec!["RFC-001", "STORY-001", "ITERATION-001"]);
    }

    #[test]
    fn forest_multi_root_includes_both_trees_roots_first() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/stories/STORY-001-childa.md",
                &doc_md("ChildA", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-002-childb.md",
                &doc_md("ChildB", "story", "- implements: RFC-002"),
            ),
        ]);

        let forest = resolve_forest(&store, None);
        let ids: Vec<&str> = forest.iter().map(|n| n.doc.id.as_str()).collect();

        // All four docs present.
        assert_eq!(
            node_ids(&forest),
            BTreeSet::from([
                "RFC-001".to_string(),
                "RFC-002".to_string(),
                "STORY-001".to_string(),
                "STORY-002".to_string(),
            ])
        );
        // Both roots (parent-count 0, path-sorted) precede both children.
        assert_eq!(&ids[..2], &["RFC-001", "RFC-002"]);
        assert!(
            forest.iter().position(|n| n.doc.id == "RFC-001").unwrap()
                < forest.iter().position(|n| n.doc.id == "STORY-001").unwrap()
        );
    }

    #[test]
    fn forest_diamond_keeps_ancestor_once_and_retains_both_parents() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-left.md",
                &doc_md("Left", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-002-right.md",
                &doc_md("Right", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: STORY-001\n- implements: STORY-002",
                ),
            ),
        ]);

        let forest = resolve_forest(&store, None);

        // Shared ancestor present exactly once.
        let base_count = forest.iter().filter(|n| n.doc.id == "RFC-001").count();
        assert_eq!(base_count, 1, "shared ancestor must appear once");
        assert_eq!(forest.len(), 4);

        // Leaf retains both parents.
        assert_eq!(
            parents_of(&forest, "ITERATION-001"),
            BTreeSet::from(["STORY-001".to_string(), "STORY-002".to_string()])
        );
    }

    #[test]
    fn forest_terminates_on_cycle_each_node_present_once() {
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-a.md",
                &doc_md("A", "rfc", "- implements: RFC-002"),
            ),
            (
                "docs/rfcs/RFC-002-b.md",
                &doc_md("B", "rfc", "- implements: RFC-001"),
            ),
        ]);

        let forest = resolve_forest(&store, None);

        assert_eq!(
            node_ids(&forest),
            BTreeSet::from(["RFC-001".to_string(), "RFC-002".to_string()])
        );
        assert_eq!(forest.len(), 2, "cycle must not duplicate nodes");
    }

    /// `topo_order` must stay near-linear in the forest size. It once re-derived
    /// the ready set from scratch on every step — rescan every indegree, sort the
    /// whole ready set to take its minimum, then linear-search every parent list
    /// to decrement — three O(N) passes per emitted node, so O(N^2 log N). That
    /// cost 145ms on the 751-doc lazyspec repo, and since the TUI re-resolves the
    /// forest synchronously on the UI thread when the graph view opens and on
    /// every pivot/sort keystroke (`tui::state::app::rebuild_graph`), it showed up
    /// as a visible stutter rather than merely a slow function.
    ///
    /// 800 docs over 400 chain edges, with a 400-wide initial ready frontier to
    /// load the ready-set sort. This asserts a complexity class, not a latency
    /// budget: measured in a debug build, the quadratic order took 1.08s here and
    /// the near-linear one takes single-digit ms, so the 250ms bound sits ~4x
    /// under the regression and ~25x over the fix — wide enough on either side
    /// that a slow CI box cannot flip the verdict.
    #[test]
    fn forest_topo_order_is_near_linear_in_forest_size() {
        const ROOTS: usize = 400;
        let mut files: Vec<(String, String)> = Vec::with_capacity(ROOTS * 2);
        for i in 1..=ROOTS {
            files.push((
                format!("docs/rfcs/RFC-{i:03}-root.md"),
                doc_md("Root", "rfc", "[]"),
            ));
            files.push((
                format!("docs/stories/STORY-{i:03}-leaf.md"),
                doc_md("Leaf", "story", &format!("- implements: RFC-{i:03}")),
            ));
        }
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();
        let (_tmp, store) = store_from(&refs);

        let start = std::time::Instant::now();
        let forest = resolve_forest(&store, None);
        let elapsed = start.elapsed();

        assert_eq!(forest.len(), ROOTS * 2, "every doc is emitted exactly once");
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "resolve_forest over {} docs took {elapsed:?}; topo_order has regressed \
             to a quadratic scan-per-emitted-node",
            ROOTS * 2
        );
    }

    /// A forest reduced to comparable values: the emitted order, each node's
    /// parents in emitted order, and whether anchoring inverted them.
    /// `ContextNode` borrows a `DocMeta` and carries no equality, so two forests
    /// are compared through this projection. Both loads below read one fixture
    /// directory, so the paths themselves compare.
    fn forest_shape(forest: &[ContextNode]) -> Vec<(PathBuf, Vec<PathBuf>, bool)> {
        forest
            .iter()
            .map(|node| {
                (
                    node.doc.path.clone(),
                    node.parents.clone(),
                    node.parents_inverted,
                )
            })
            .collect()
    }

    /// RFC-067 §"The traversal cost, stated plainly" made executable: a wildcard
    /// row buys no precision and exists to keep the config short, so it must
    /// reproduce what the global marker did rather than approximate it. Two
    /// loads of one fixture, differing only in where `implements` is declared to
    /// walk, must yield the same whole-store forest -- same nodes, same order,
    /// same parent edges.
    #[test]
    fn a_blanket_edge_row_rebuilds_the_forest_the_global_marker_built() {
        let tmp = write_docs(&[
            ("docs/rfcs/RFC-001-left.md", &doc_md("Left", "rfc", "[]")),
            ("docs/rfcs/RFC-002-right.md", &doc_md("Right", "rfc", "[]")),
            (
                "docs/stories/STORY-001-first.md",
                &doc_md("First", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-002-second.md",
                &doc_md("Second", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-003-third.md",
                &doc_md("Third", "story", "- implements: RFC-002"),
            ),
            (
                "docs/iterations/ITERATION-001-shared.md",
                &doc_md(
                    "Shared",
                    "iteration",
                    "- implements: STORY-001\n- implements: STORY-002",
                ),
            ),
            (
                "docs/iterations/ITERATION-002-loose.md",
                &doc_md("Loose", "iteration", "[]"),
            ),
        ]);

        let by_global_marker = Config::default();
        assert_eq!(
            by_global_marker
                .relationship_by_name("implements")
                .unwrap()
                .traversal,
            Some(Traversal::Chain),
            "the starter config marks implements chain; that marker is what the row must replace"
        );

        let mut by_blanket_row = by_global_marker.clone();
        for rel in &mut by_blanket_row.relationships {
            if rel.name == "implements" {
                rel.traversal = None;
            }
        }
        by_blanket_row.edges.push(EdgeDef {
            name: "anything-implements-anything".to_string(),
            from: TypeSelector::Any,
            to: TypeSelector::Any,
            via: RelSelector::Named(vec!["implements".to_string()]),
            required: None,
            traversal: Some(Traversal::Chain),
        });

        let marker_store = Store::load(tmp.path(), &by_global_marker).unwrap();
        let row_store = Store::load(tmp.path(), &by_blanket_row).unwrap();
        let by_marker = resolve_forest(&marker_store, None);
        let by_row = resolve_forest(&row_store, None);

        // The forest under comparison has real shape -- three roots, three
        // levels, a diamond -- so equality below cannot pass by both sides being
        // flat or empty.
        assert_eq!(by_marker.len(), 7);
        assert_eq!(
            parents_of(&by_marker, "ITERATION-001"),
            BTreeSet::from(["STORY-001".to_string(), "STORY-002".to_string()])
        );
        assert_eq!(
            parents_of(&by_marker, "STORY-003"),
            BTreeSet::from(["RFC-002".to_string()])
        );
        assert!(parents_of(&by_marker, "ITERATION-002").is_empty());

        assert_eq!(
            forest_shape(&by_row),
            forest_shape(&by_marker),
            "the blanket row must reproduce the global marker's forest exactly"
        );
    }

    // --- resolve_forest anchored -------------------------------------------

    #[test]
    fn forest_anchor_roots_on_type_with_descendants_nested() {
        // anchor=story -> roots are all stories with their iteration descendants.
        // The shared parent RFC is no longer pruned (STORY-247): it hangs under
        // each story as an inverted edge, asserted in the reverse-chain tests
        // below. What matters here is that the stories are the ROOTS.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-a.md",
                &doc_md("A", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-002-b.md",
                &doc_md("B", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let forest = resolve_forest(&store, Some("story"));

        assert_eq!(
            node_ids(&forest),
            BTreeSet::from([
                "STORY-001".to_string(),
                "STORY-002".to_string(),
                "ITERATION-001".to_string(),
                "RFC-001".to_string(),
            ])
        );
        // Stories are roots: their own chain parents were inverted, not kept.
        assert!(forest
            .iter()
            .find(|n| n.doc.id == "STORY-001")
            .unwrap()
            .parents
            .is_empty());
        // The iteration retains its story parent within the subtree.
        assert_eq!(
            parents_of(&forest, "ITERATION-001"),
            BTreeSet::from(["STORY-001".to_string()])
        );
    }

    #[test]
    fn forest_none_matches_whole_store() {
        // AC2: resolve_forest(store, None) == today's whole-store output.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/stories/STORY-001-childa.md",
                &doc_md("ChildA", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let forest = resolve_forest(&store, None);
        let ids: Vec<&str> = forest.iter().map(|n| n.doc.id.as_str()).collect();

        // Whole store, every doc once, root-first topological order.
        assert_eq!(
            node_ids(&forest),
            BTreeSet::from([
                "RFC-001".to_string(),
                "RFC-002".to_string(),
                "STORY-001".to_string(),
                "ITERATION-001".to_string(),
            ])
        );
        assert!(
            ids.iter().position(|i| *i == "RFC-001").unwrap()
                < ids.iter().position(|i| *i == "STORY-001").unwrap()
        );
        assert!(
            ids.iter().position(|i| *i == "STORY-001").unwrap()
                < ids.iter().position(|i| *i == "ITERATION-001").unwrap()
        );
    }

    #[test]
    fn forest_anchor_diamond_descendant_under_each_anchor_no_loop() {
        // AC4: a doc with two anchor-type ancestors appears once, retaining both
        // anchor parents, no infinite loop.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-left.md",
                &doc_md("Left", "story", "- implements: RFC-001"),
            ),
            (
                "docs/stories/STORY-002-right.md",
                &doc_md("Right", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: STORY-001\n- implements: STORY-002",
                ),
            ),
        ]);

        let forest = resolve_forest(&store, Some("story"));

        let leaf_count = forest
            .iter()
            .filter(|n| n.doc.id == "ITERATION-001")
            .count();
        assert_eq!(leaf_count, 1, "diamond descendant appears exactly once");
        assert_eq!(
            parents_of(&forest, "ITERATION-001"),
            BTreeSet::from(["STORY-001".to_string(), "STORY-002".to_string()]),
            "descendant retains both anchor-type parents"
        );
    }

    #[test]
    fn forest_by_tag_reroots_on_tagged_docs() {
        // A tagged story becomes a root and pulls in its descendant subtree plus
        // its inverted ancestor chain. STORY-002 stays pruned even though it
        // implements the same RFC: the upward walk re-parents ancestors, it does
        // not descend from them.
        let tagged_story = "---\ntitle: \"Tagged\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags:\n- alpha\nrelated:\n- implements: RFC-001\n---\n\nbody\n";
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            ("docs/stories/STORY-001-tagged.md", tagged_story),
            (
                "docs/stories/STORY-002-other.md",
                &doc_md("Other", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let forest = resolve_forest_by_tag(&store, "alpha");

        assert_eq!(
            node_ids(&forest),
            BTreeSet::from([
                "STORY-001".to_string(),
                "ITERATION-001".to_string(),
                "RFC-001".to_string(),
            ]),
            "tag anchor keeps the tagged story, its descendant and its ancestor; \
             the sibling story is pruned"
        );
        assert!(
            forest
                .iter()
                .find(|n| n.doc.id == "STORY-001")
                .unwrap()
                .parents
                .is_empty(),
            "the tagged story is a root (its RFC parent edge was inverted)"
        );
    }

    #[test]
    fn forest_by_tag_empty_when_no_doc_carries_tag() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            (
                "docs/stories/STORY-001-b.md",
                &doc_md("B", "story", "- implements: RFC-001"),
            ),
        ]);

        let forest = resolve_forest_by_tag(&store, "nonexistent");
        assert!(forest.is_empty(), "no roots -> empty forest");
    }

    // --- anchored reverse chain (STORY-247) --------------------------------

    /// `ITERATION-001 implements STORY-001 implements RFC-001` — the 3-deep chain
    /// the reverse-chain tests pivot on.
    fn linear_chain_store() -> (TempDir, Store) {
        store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ])
    }

    #[test]
    fn forest_anchor_leaf_type_inverts_the_whole_ancestor_chain() {
        // AC1: pivoting on the leaf type used to yield a flat list. Now the
        // iteration is the root and each ancestor hangs below the node it was
        // reached from, its edge recorded as inverted.
        let (_tmp, store) = linear_chain_store();

        let forest = resolve_forest(&store, Some("iteration"));

        assert_eq!(
            node_ids(&forest),
            BTreeSet::from([
                "ITERATION-001".to_string(),
                "STORY-001".to_string(),
                "RFC-001".to_string(),
            ])
        );
        let anchor = forest.iter().find(|n| n.doc.id == "ITERATION-001").unwrap();
        assert!(anchor.parents.is_empty(), "the anchor is the root");
        assert!(!anchor.parents_inverted);

        assert_eq!(
            parents_of(&forest, "STORY-001"),
            BTreeSet::from(["ITERATION-001".to_string()]),
            "the anchor's chain parent is re-parented under the anchor"
        );
        assert!(parents_inverted_of(&forest, "STORY-001"));
        assert_eq!(
            parents_of(&forest, "RFC-001"),
            BTreeSet::from(["STORY-001".to_string()]),
            "the walk continues up: the grandparent hangs under the parent"
        );
        assert!(parents_inverted_of(&forest, "RFC-001"));
    }

    #[test]
    fn forest_anchor_mid_chain_emits_descendants_forward_and_ancestors_inverted() {
        // AC3: a story anchor has both directions. Its iteration keeps a forward
        // edge (no reverse marker), its RFC gets an inverted one, and neither is
        // emitted twice.
        let (_tmp, store) = linear_chain_store();

        let forest = resolve_forest(&store, Some("story"));

        assert_eq!(forest.len(), 3, "no node emitted twice");
        assert!(forest
            .iter()
            .find(|n| n.doc.id == "STORY-001")
            .unwrap()
            .parents
            .is_empty());
        assert_eq!(
            parents_of(&forest, "ITERATION-001"),
            BTreeSet::from(["STORY-001".to_string()])
        );
        assert!(
            !parents_inverted_of(&forest, "ITERATION-001"),
            "a descendant edge is not inverted"
        );
        assert_eq!(
            parents_of(&forest, "RFC-001"),
            BTreeSet::from(["STORY-001".to_string()])
        );
        assert!(
            parents_inverted_of(&forest, "RFC-001"),
            "an ancestor edge is"
        );
    }

    #[test]
    fn forest_anchor_forked_lineage_keeps_both_upward_branches() {
        // AC4: a story implementing two RFCs renders both, each an inverted child
        // of the anchor.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/stories/STORY-001-fork.md",
                &doc_md(
                    "Fork",
                    "story",
                    "- implements: RFC-001\n- implements: RFC-002",
                ),
            ),
        ]);

        let forest = resolve_forest(&store, Some("story"));

        for rfc in ["RFC-001", "RFC-002"] {
            assert_eq!(
                parents_of(&forest, rfc),
                BTreeSet::from(["STORY-001".to_string()]),
                "{rfc} hangs under the anchor"
            );
            assert!(parents_inverted_of(&forest, rfc));
        }
    }

    #[test]
    fn forest_anchor_shared_ancestor_kept_once_with_its_own_lineage_above_it() {
        // Three anchors fanning into one story that is itself under an RFC. The
        // shared ancestor is kept once carrying an inverted edge to each anchor
        // (mirroring how a shared DESCENDANT keeps both anchor-side parents), and
        // the walk still climbs past it, so no anchor's lineage is cut short.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-top.md", &doc_md("Top", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-a.md",
                &doc_md("A", "iteration", "- implements: STORY-001"),
            ),
            (
                "docs/iterations/ITERATION-002-b.md",
                &doc_md("B", "iteration", "- implements: STORY-001"),
            ),
            (
                "docs/iterations/ITERATION-003-c.md",
                &doc_md("C", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let forest = resolve_forest(&store, Some("iteration"));

        assert_eq!(
            forest.iter().filter(|n| n.doc.id == "STORY-001").count(),
            1,
            "the shared ancestor is kept once"
        );
        // Which anchor reaches the ancestor first depends on `store.docs` iteration
        // order (a HashMap), so the inverted edge list is path-sorted: these parents
        // must come out in exactly this order on every run.
        assert_eq!(
            ordered_parents_of(&forest, "STORY-001"),
            vec![
                "ITERATION-001".to_string(),
                "ITERATION-002".to_string(),
                "ITERATION-003".to_string(),
            ],
            "one inverted edge per anchor, path-sorted"
        );
        assert!(parents_inverted_of(&forest, "STORY-001"));
        assert_eq!(
            ordered_parents_of(&forest, "RFC-001"),
            vec!["STORY-001".to_string()],
            "the shared ancestor's own ancestor is reached too"
        );
        assert!(parents_inverted_of(&forest, "RFC-001"));
    }

    #[test]
    fn forest_by_tag_inverts_ancestor_chain_like_the_type_anchor() {
        // AC5: the tag pivot shares resolve_forest_anchored, so the reverse chain
        // is identical to the type pivot over the same anchor doc.
        let tagged_iteration = "---\ntitle: \"Leaf\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags:\n- alpha\nrelated:\n- implements: STORY-001\n---\n\nbody\n";
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            ("docs/iterations/ITERATION-001-leaf.md", tagged_iteration),
        ]);

        let by_tag = resolve_forest_by_tag(&store, "alpha");
        let by_type = resolve_forest(&store, Some("iteration"));

        assert_eq!(node_ids(&by_tag), node_ids(&by_type));
        assert_eq!(
            parents_of(&by_tag, "STORY-001"),
            BTreeSet::from(["ITERATION-001".to_string()])
        );
        assert!(parents_inverted_of(&by_tag, "STORY-001"));
        assert_eq!(
            parents_of(&by_tag, "RFC-001"),
            BTreeSet::from(["STORY-001".to_string()])
        );
        assert!(parents_inverted_of(&by_tag, "RFC-001"));
    }

    #[test]
    fn forest_by_tag_anchor_inside_a_higher_anchors_subtree_inverts_nothing() {
        // AC5, the case a type pivot cannot produce: one tag on an RFC and on an
        // iteration three hops below it. Mixed-level tag anchors on DIVERGENT
        // branches do still invert; what inverts nothing is a lower anchor sitting
        // INSIDE a higher anchor's downward subtree, as here. The higher anchor's BFS
        // covers the lower one, so every ancestor the upward walk sees is inside the
        // subtree and NOTHING is inverted — the pivot is the plain forward chain
        // rooted on the RFC. The story between them is pulled in as a descendant of
        // the RFC even though it carries no tag.
        let tagged_rfc = "---\ntitle: \"Top\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags:\n- alpha\nrelated: []\n---\n\nbody\n";
        let tagged_iteration = "---\ntitle: \"Leaf\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags:\n- alpha\nrelated:\n- implements: STORY-001\n---\n\nbody\n";
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-top.md", tagged_rfc),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            ("docs/iterations/ITERATION-001-leaf.md", tagged_iteration),
        ]);

        let forest = resolve_forest_by_tag(&store, "alpha");

        assert_eq!(
            forest.iter().map(|n| n.doc.id.as_str()).collect::<Vec<_>>(),
            vec!["RFC-001", "STORY-001", "ITERATION-001"],
            "the forward chain, root-first"
        );
        assert!(
            forest.iter().all(|n| !n.parents_inverted),
            "an ancestor inside the anchors' own subtree is never re-parented"
        );
        assert_eq!(
            ordered_parents_of(&forest, "ITERATION-001"),
            vec!["STORY-001".to_string()],
            "the lower anchor keeps its forward parent edge"
        );
    }

    #[test]
    fn forest_anchor_upward_cycle_terminates_each_node_once() {
        // AC7: STORY-001 and STORY-002 implement each other above the anchor. The
        // upward walk's seen-set stops the loop; every node is still present once.
        let (_tmp, store) = store_from(&[
            (
                "docs/stories/STORY-001-a.md",
                &doc_md("A", "story", "- implements: STORY-002"),
            ),
            (
                "docs/stories/STORY-002-b.md",
                &doc_md("B", "story", "- implements: STORY-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]);

        let forest = resolve_forest(&store, Some("iteration"));

        assert_eq!(
            node_ids(&forest),
            BTreeSet::from([
                "ITERATION-001".to_string(),
                "STORY-001".to_string(),
                "STORY-002".to_string(),
            ])
        );
        assert_eq!(forest.len(), 3, "cycle must not duplicate nodes");
    }

    #[test]
    fn forest_unanchored_never_inverts_an_edge() {
        // AC6: the All forest is unchanged -- no reverse edges anywhere.
        let (_tmp, store) = linear_chain_store();

        let forest = resolve_forest(&store, None);

        assert!(forest.iter().all(|n| !n.parents_inverted));
        assert_eq!(
            forest.iter().map(|n| n.doc.id.as_str()).collect::<Vec<_>>(),
            vec!["RFC-001", "STORY-001", "ITERATION-001"]
        );
    }
}
