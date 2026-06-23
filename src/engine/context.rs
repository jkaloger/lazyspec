use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::{ResolveError, Store};
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

pub struct ContextNode<'a> {
    pub doc: &'a DocMeta,
    pub parents: Vec<PathBuf>,
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
            if !store
                .chain_relationships
                .iter()
                .any(|r| r.as_str() == rel.rel_type.as_str())
            {
                continue;
            }
            let Some(parent) = store.resolve_relation_target(&rel.target) else {
                continue;
            };
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

    // Forward context: docs that link to the target via any configured
    // parent-child relationship. Each is one hop out, reached through the
    // target.
    let target_path = doc.path.clone();
    let forward: Vec<RelatedRef> = store
        .reverse_links
        .get(&target_path)
        .map(|links| {
            links
                .iter()
                .filter(|(rel_type, _)| {
                    store
                        .chain_relationships
                        .iter()
                        .any(|r| r.as_str() == rel_type.as_str())
                })
                .filter_map(|(rel_type, source_path)| store.get(source_path).map(|d| (rel_type, d)))
                .map(|(rel_type, d)| RelatedRef {
                    doc: d,
                    relation: rel_type.clone(),
                    distance: 1,
                    via: target_path.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Related: BFS over `related-to` links (both directions), bounded by
    // `depth`. Hop 0's frontier is the chain/DAG nodes; each subsequent hop
    // expands one ring of `related-to` neighbours. First discovery wins, so a
    // doc's recorded `distance` is its shortest hop count. Frontiers and
    // neighbours are processed in path order, so when a doc is reachable from
    // two frontier docs at the same hop the lexicographically-smallest `via`
    // wins (deterministic, test-stable).
    let chain_paths: HashSet<PathBuf> = nodes.iter().map(|n| n.doc.path.clone()).collect();
    let mut related_seen: HashSet<PathBuf> = HashSet::new();
    let mut related: Vec<RelatedRef> = Vec::new();

    let mut frontier: Vec<PathBuf> = chain_paths.iter().cloned().collect();
    frontier.sort();

    for hop in 1..=depth {
        let mut next_frontier: Vec<PathBuf> = Vec::new();

        for from in &frontier {
            let mut neighbours: Vec<PathBuf> = Vec::new();
            if let Some(fwd) = store.forward_links.get(from) {
                for (rel_type, target) in fwd {
                    if rel_type.as_str() == "related-to" {
                        neighbours.push(target.clone());
                    }
                }
            }
            if let Some(rev) = store.reverse_links.get(from) {
                for (rel_type, source) in rev {
                    if rel_type.as_str() == "related-to" {
                        neighbours.push(source.clone());
                    }
                }
            }
            neighbours.sort();

            for neighbour in neighbours {
                if chain_paths.contains(&neighbour) || !related_seen.insert(neighbour.clone()) {
                    continue;
                }
                if let Some(resolved) = store.get(&neighbour) {
                    related.push(RelatedRef {
                        doc: resolved,
                        relation: RelationType::new("related-to"),
                        distance: hop,
                        via: from.clone(),
                    });
                    next_frontier.push(neighbour);
                }
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

/// Context forest, roots-first. When `anchor` is `None`, discovers every
/// document and, for each, its in-graph parents (resolved via
/// [`Store::resolve_relation_target`] over the configured parent-child
/// relationships), then orders the full DAG with [`topo_order`] so each
/// multi-parent node appears after all its parents and exactly once. Roots
/// (docs with no resolvable in-graph parent) come first.
///
/// When `anchor` is `Some(type)`, the forest is re-rooted on documents whose
/// `doc_type` matches `type`: roots become the anchor-type docs and only their
/// chain-descendant subtrees are emitted. Ancestors above an anchor (and any
/// doc not reachable downward from an anchor) are pruned. A descendant reachable
/// from two anchor-type docs is retained once with both anchor-side parents, so
/// it appears under each anchor in a tree render without looping.
///
/// Parents are sourced from each doc's own declared `related`, NOT from
/// `forward_links`: `propagate_parent_links` copies a parent's forward links
/// onto nested child docs, so reading `forward_links` would over-collect
/// inherited parent-child edges (the same trap `resolve_chain` avoids). Every
/// in-graph parent is retained on the node so multi-parent edges survive.
pub fn resolve_forest<'a>(store: &'a Store, anchor: Option<&str>) -> Vec<ContextNode<'a>> {
    // Each doc's in-graph parents over the configured chain relationships.
    let all_parents: HashMap<PathBuf, Vec<PathBuf>> = store
        .docs
        .values()
        .map(|doc| {
            let mut parents: Vec<PathBuf> = Vec::new();
            for rel in &doc.related {
                if !store
                    .chain_relationships
                    .iter()
                    .any(|r| r.as_str() == rel.rel_type.as_str())
                {
                    continue;
                }
                let Some(parent) = store.resolve_relation_target(&rel.target) else {
                    continue;
                };
                if !parents.contains(&parent.path) {
                    parents.push(parent.path.clone());
                }
            }
            (doc.path.clone(), parents)
        })
        .collect();

    let Some(anchor_type) = anchor else {
        let discovered: HashMap<PathBuf, &DocMeta> =
            store.docs.values().map(|d| (d.path.clone(), d)).collect();
        return topo_order(&discovered, &all_parents);
    };

    // Child adjacency (parent -> children) over the chain edges, so we can walk
    // downward from each anchor-type doc.
    let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for (child, parents) in &all_parents {
        for parent in parents {
            children
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
        }
    }

    // BFS down from every anchor-type doc, collecting the union of descendant
    // subtrees. The seen-set dedups diamonds and guards against cycles.
    let mut subtree: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = store
        .docs
        .values()
        .filter(|d| d.doc_type.as_str() == anchor_type)
        .map(|d| d.path.clone())
        .collect();
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

    let discovered: HashMap<PathBuf, &DocMeta> = store
        .docs
        .values()
        .filter(|d| subtree.contains(&d.path))
        .map(|d| (d.path.clone(), d))
        .collect();

    // Keep only parent edges that stay within the pruned subtree, so anchors
    // surface as roots and pruned ancestors do not reattach.
    let node_parents: HashMap<PathBuf, Vec<PathBuf>> = discovered
        .keys()
        .map(|path| {
            let parents = all_parents
                .get(path)
                .map(|ps| {
                    ps.iter()
                        .filter(|p| subtree.contains(*p))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            (path.clone(), parents)
        })
        .collect();

    topo_order(&discovered, &node_parents)
}

/// Deterministic topological ordering of the discovered DAG, root-first.
/// `node_parents` holds the parent-child edges (child -> parents). A node is
/// emitted only once all its parents have been emitted; ready nodes are
/// broken by path for determinism. For a single-parent chain this yields the
/// old `chain` order (root first, target last). Cyclic input has no valid
/// topological order, so any remaining nodes are appended path-ordered; the
/// node set is still complete (each node once).
fn topo_order<'a>(
    discovered: &HashMap<PathBuf, &'a DocMeta>,
    node_parents: &HashMap<PathBuf, Vec<PathBuf>>,
) -> Vec<ContextNode<'a>> {
    let mut remaining_parents: HashMap<PathBuf, usize> = discovered
        .keys()
        .map(|path| {
            let count = node_parents
                .get(path)
                .map(|parents| {
                    parents
                        .iter()
                        .filter(|p| discovered.contains_key(*p))
                        .count()
                })
                .unwrap_or(0);
            (path.clone(), count)
        })
        .collect();

    let mut ordered: Vec<PathBuf> = Vec::with_capacity(discovered.len());
    let mut emitted: HashSet<PathBuf> = HashSet::new();

    while ordered.len() < discovered.len() {
        let mut ready: Vec<&PathBuf> = remaining_parents
            .iter()
            .filter(|(path, count)| **count == 0 && !emitted.contains(*path))
            .map(|(path, _)| path)
            .collect();

        if ready.is_empty() {
            // Cycle: no node with all parents satisfied. Emit the remaining
            // nodes path-ordered to guarantee termination and completeness.
            let mut leftover: Vec<PathBuf> = discovered
                .keys()
                .filter(|p| !emitted.contains(*p))
                .cloned()
                .collect();
            leftover.sort();
            for path in leftover {
                emitted.insert(path.clone());
                ordered.push(path);
            }
            break;
        }

        ready.sort();
        let next = ready[0].clone();
        emitted.insert(next.clone());
        ordered.push(next.clone());

        for (child, parents) in node_parents {
            if parents.contains(&next) {
                if let Some(count) = remaining_parents.get_mut(child) {
                    *count = count.saturating_sub(1);
                }
            }
        }
    }

    ordered
        .into_iter()
        .map(|path| ContextNode {
            doc: discovered[&path],
            parents: node_parents.get(&path).cloned().unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    /// Build a markdown doc with the given type and `related` block. `related`
    /// is the YAML list body (e.g. `"- implements: RFC-001"`) or `"[]"` for
    /// none.
    fn doc_md(title: &str, doc_type: &str, related: &str) -> String {
        let related_block = if related == "[]" {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\n{related_block}\n---\n\n{title} body\n"
        )
    }

    /// Load a real `Store` from in-memory files written under a fresh TempDir.
    /// Goes through `Store::load` so link-building (including
    /// `propagate_parent_links`) runs exactly as in production.
    fn store_from(files: &[(&str, &str)]) -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        for (rel_path, contents) in files {
            let full = tmp.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        let store = Store::load(tmp.path(), &Config::default()).unwrap();
        (tmp, store)
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
        node.parents
            .iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
            .map(|stem| crate::engine::store::extract_id_from_name(&stem))
            .collect()
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

    // --- resolve_forest anchored -------------------------------------------

    #[test]
    fn forest_anchor_roots_on_type_and_prunes_ancestors() {
        // AC1: anchor=story -> roots are all stories with their iteration
        // descendants, no parent rfc above.
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
            ]),
            "anchored forest excludes the parent RFC and includes the iteration descendant"
        );
        // Stories are roots (no in-graph parents after pruning).
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
        assert!(
            !forest.iter().any(|n| n.doc.id == "RFC-001"),
            "ancestor RFC pruned"
        );
    }
}
