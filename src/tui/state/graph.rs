use crate::engine::context::ContextNode;
use crate::engine::store::{extract_id_from_name, Store};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::GraphNode;

/// Flatten the engine's whole-store context forest into the flat
/// `Vec<GraphNode>` the graph view renders. Walks the forest roots-first,
/// depth-first, mirroring the CLI `render_tree` traversal: roots (nodes with no
/// in-graph `implements` parent) sorted by path, children sorted by path, with
/// `depth` assigned by tree level.
///
/// A node reachable by more than one parent (a diamond) is drawn in full on
/// first encounter and emitted as a one-line back-reference (`reference: true`)
/// on subsequent encounters without recursing, so every reachable doc appears
/// exactly once as a full node. Cyclic SCCs with no root are emitted as depth-0
/// subtrees after the root pass, so the render is complete and terminates.
///
/// Each full node also carries its depth-1 `related-to` neighbours as a
/// display-only annotation set (RFC-006 Graph mode Phase 1), sourced from the
/// store's `forward_links`/`reverse_links` filtered to the `related-to` relation.
/// A neighbour that lies on the node's own `implements` lineage — a transitive
/// ancestor (parent, grandparent, …) or a transitive descendant (child,
/// grandchild, …) — is excluded, because such a link is already drawn as a tree
/// edge through this node and is not cross-cutting. Everything else IS surfaced,
/// including siblings/cousins reachable only through a SHARED ANCESTOR (e.g. two
/// docs that both `implements` the same root and are `related-to` each other):
/// there is no `implements` path between them, so the link is genuinely
/// cross-cutting and the Story AC ("connected only by a related-to link, no
/// implements path between them") requires it to surface.
///
/// The exclusion is the node's OWN ancestors and descendants only — see
/// [`implements_lineage_of`] for why the up/down walks must stay independent so a
/// shared ancestor's other children are not swept in. The ancestor direction
/// matches the upward BFS `chain_paths` exclusion in [`resolve_chain`]
/// (`engine/context.rs`). Note this annotation set is therefore NOT equal in
/// general to `resolve_chain(id, 1).related`: `resolve_chain` also surfaces the
/// related-to links of the node's `implements` ancestors, whereas this annotation
/// is strictly the node's OWN depth-1 cross-cutting set.
///
/// Back-reference nodes carry no annotation: the set belongs on the full node line.
pub fn flatten_forest(forest: &[ContextNode], store: &Store) -> Vec<GraphNode> {
    // child adjacency: parent path -> child paths, sorted by path.
    let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for node in forest {
        for parent in &node.parents {
            children
                .entry(parent.clone())
                .or_default()
                .push(node.doc.path.clone());
        }
    }
    for kids in children.values_mut() {
        kids.sort();
    }

    let by_path: HashMap<&PathBuf, &ContextNode> =
        forest.iter().map(|n| (&n.doc.path, n)).collect();

    // Per-node `implements` lineage: the node's own transitive ancestors AND
    // descendants. A related-to neighbour on this set is already drawn as a tree
    // edge through the node, so it is not re-surfaced as a cross-cutting
    // annotation. Siblings and cousins reached only via a shared ancestor are NOT
    // on the lineage and ARE surfaced (see flatten_forest / implements_lineage_of
    // doc-comments).
    let parents_of: HashMap<&PathBuf, &Vec<PathBuf>> =
        forest.iter().map(|n| (&n.doc.path, &n.parents)).collect();
    let lineage: HashMap<&PathBuf, HashSet<PathBuf>> = forest
        .iter()
        .map(|n| {
            (
                &n.doc.path,
                implements_lineage_of(&n.doc.path, &parents_of, &children),
            )
        })
        .collect();

    let mut roots: Vec<&ContextNode> = forest.iter().filter(|n| n.parents.is_empty()).collect();
    roots.sort_by(|a, b| a.doc.path.cmp(&b.doc.path));

    let mut out: Vec<GraphNode> = Vec::with_capacity(forest.len());
    let mut drawn: HashSet<PathBuf> = HashSet::new();

    for root in roots {
        walk(
            root, 0, &children, &by_path, &lineage, store, &mut drawn, &mut out,
        );
    }

    // Cyclic input can leave a strongly-connected component with no root, so
    // the root pass never reaches it. Emit any still-undrawn node as a depth-0
    // subtree (forest order is topological/path-broken) so the render is
    // complete; the drawn-set still guarantees each node is drawn once in full.
    for node in forest {
        if !drawn.contains(&node.doc.path) {
            walk(
                node, 0, &children, &by_path, &lineage, store, &mut drawn, &mut out,
            );
        }
    }

    out
}

/// The node's own `implements` lineage: its transitive ANCESTORS (parents,
/// grandparents, …) and its transitive DESCENDANTS (children, grandchildren, …),
/// with the node's own path excluded. Used to exclude already-edge-connected
/// related-to neighbours (links drawn as a tree edge through this node) from the
/// cross-cutting annotation set.
///
/// Crucially this is two INDEPENDENT walks seeded at `path` — an upward pass over
/// parents and a downward pass over children — that never cross-pollinate. The
/// up-walk never descends and the down-walk never ascends, so an ANCESTOR's other
/// children (the node's siblings/cousins) are never collected. That is the whole
/// fix: a sibling reached only through a shared ancestor has no `implements` path
/// to the node, so its related-to link is genuinely cross-cutting and must
/// surface (the Story AC's "connected only by a related-to link"). The earlier
/// single-stack walk pushed both parents and children of every popped node, so
/// from a shared ancestor it descended into siblings and wrongly excluded them.
///
/// The ancestor direction mirrors the upward BFS that builds `chain_paths` in
/// [`resolve_chain`] (`engine/context.rs`). The descendant direction is also
/// excluded because a direct/transitive child connected by a drawn `implements`
/// edge is on the tree, not cross-cutting (e.g. a root that both parents and is
/// related-to its child must not annotate that child). Bounded by the forest
/// size; the seen sets guard cycles.
fn implements_lineage_of(
    path: &Path,
    parents_of: &HashMap<&PathBuf, &Vec<PathBuf>>,
    children: &HashMap<PathBuf, Vec<PathBuf>>,
) -> HashSet<PathBuf> {
    let mut lineage: HashSet<PathBuf> = HashSet::new();

    let mut up: Vec<PathBuf> = vec![path.to_path_buf()];
    let mut seen_up: HashSet<PathBuf> = HashSet::from([path.to_path_buf()]);
    while let Some(current) = up.pop() {
        if let Some(parents) = parents_of.get(&current) {
            for parent in parents.iter() {
                if seen_up.insert(parent.clone()) {
                    lineage.insert(parent.clone());
                    up.push(parent.clone());
                }
            }
        }
    }

    let mut down: Vec<PathBuf> = vec![path.to_path_buf()];
    let mut seen_down: HashSet<PathBuf> = HashSet::from([path.to_path_buf()]);
    while let Some(current) = down.pop() {
        if let Some(kids) = children.get(&current) {
            for kid in kids {
                if seen_down.insert(kid.clone()) {
                    lineage.insert(kid.clone());
                    down.push(kid.clone());
                }
            }
        }
    }

    lineage
}

/// The node's OWN depth-1 cross-cutting `related-to` neighbours as doc ids,
/// sorted, excluding any neighbour on the node's `implements` lineage (a
/// transitive ancestor or descendant, already drawn as a tree edge through the
/// node). Reads the same propagated `forward_links`/`reverse_links` the `context`
/// command's related set reads, but is NOT equal to `context <id> --json`'s
/// `related` in general: that set also surfaces the related-to links of the
/// node's ancestors, whereas this is the node's own depth-1 set only (see
/// `flatten_forest` doc-comment).
fn related_annotations(path: &Path, lineage: &HashSet<PathBuf>, store: &Store) -> Vec<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for (rel, neighbour) in store.related_to(path) {
        if rel.as_str() != "related-to" || lineage.contains(neighbour) {
            continue;
        }
        let id = match store.get(neighbour) {
            Some(doc) => doc.id.clone(),
            None => neighbour
                .file_stem()
                .map(|s| extract_id_from_name(&s.to_string_lossy()))
                .unwrap_or_default(),
        };
        ids.insert(id);
    }
    ids.into_iter().collect()
}

#[allow(clippy::too_many_arguments)]
fn walk(
    node: &ContextNode,
    depth: usize,
    children: &HashMap<PathBuf, Vec<PathBuf>>,
    by_path: &HashMap<&PathBuf, &ContextNode>,
    lineage: &HashMap<&PathBuf, HashSet<PathBuf>>,
    store: &Store,
    drawn: &mut HashSet<PathBuf>,
    out: &mut Vec<GraphNode>,
) {
    let doc = node.doc;

    if drawn.contains(&doc.path) {
        out.push(GraphNode {
            path: doc.path.clone(),
            title: doc.title.clone(),
            doc_type: doc.doc_type.clone(),
            status: doc.status.clone(),
            depth,
            reference: true,
            related: Vec::new(),
        });
        return;
    }
    drawn.insert(doc.path.clone());

    let node_lineage = lineage.get(&doc.path);
    let empty = HashSet::new();
    out.push(GraphNode {
        path: doc.path.clone(),
        title: doc.title.clone(),
        doc_type: doc.doc_type.clone(),
        status: doc.status.clone(),
        depth,
        reference: false,
        related: related_annotations(&doc.path, node_lineage.unwrap_or(&empty), store),
    });

    if let Some(kids) = children.get(&doc.path) {
        for child_path in kids {
            if let Some(child) = by_path.get(child_path) {
                walk(
                    child,
                    depth + 1,
                    children,
                    by_path,
                    lineage,
                    store,
                    drawn,
                    out,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use crate::engine::context::resolve_forest;
    use crate::engine::store::{extract_id_from_name, Store};
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

    /// Load a real `Store` from in-memory files written under a fresh TempDir,
    /// going through `Store::load` so link-building runs as in production.
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

    /// The doc id of a `GraphNode` (derived from its path file stem).
    fn id_of(node: &GraphNode) -> String {
        let stem = node.path.file_stem().unwrap().to_string_lossy();
        extract_id_from_name(&stem)
    }

    /// Collapse a flattened node list to `(id, depth, reference)` triples for
    /// exact-sequence assertions.
    fn triples(nodes: &[GraphNode]) -> Vec<(String, usize, bool)> {
        nodes
            .iter()
            .map(|n| (id_of(n), n.depth, n.reference))
            .collect()
    }

    #[test]
    fn flatten_single_chain_increments_depth_no_references() {
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

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, false),
                ("ITERATION-001".to_string(), 2, false),
            ]
        );
    }

    #[test]
    fn flatten_diamond_draws_shared_node_once_then_back_reference() {
        // RFC-001 is the root; STORY-001 and STORY-002 both implement it;
        // ITERATION-001 implements both stories (the diamond). On the second
        // story's subtree the leaf is a back-reference.
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

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, false),
                ("ITERATION-001".to_string(), 2, false),
                ("STORY-002".to_string(), 1, false),
                ("ITERATION-001".to_string(), 2, true),
            ],
            "shared leaf is full under STORY-001, a back-reference under STORY-002"
        );
    }

    #[test]
    fn flatten_multi_root_orders_roots_by_path() {
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

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, false),
                ("RFC-002".to_string(), 0, false),
                ("STORY-002".to_string(), 1, false),
            ],
            "roots are emitted path-sorted, each followed by its subtree"
        );
    }

    #[test]
    fn flatten_multi_parent_node_retains_both_edges() {
        // ITERATION-001 implements two roots; both edges must surface, so the
        // leaf appears under each parent (full once, then a back-reference).
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

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("ITERATION-001".to_string(), 1, false),
                ("RFC-002".to_string(), 0, false),
                ("ITERATION-001".to_string(), 1, true),
            ],
            "both implements edges render: full under RFC-001, reference under RFC-002"
        );
    }

    #[test]
    fn flatten_cycle_terminates_each_node_once_in_full() {
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

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        // Cycle has no root, so the leftover pass draws RFC-001 full at depth 0,
        // recurses into child RFC-002 full at depth 1, then re-encounters RFC-001
        // as a depth-2 back-reference. No infinite recursion.
        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("RFC-002".to_string(), 1, false),
                ("RFC-001".to_string(), 2, true),
            ],
            "cycle terminates: each node full once, back-edge as a reference"
        );
    }

    // --- related-to annotations -------------------------------------------

    /// The `related` annotation set of the first full node with the given id.
    fn related_of(nodes: &[GraphNode], id: &str) -> Vec<String> {
        nodes
            .iter()
            .find(|n| !n.reference && id_of(n) == id)
            .unwrap_or_else(|| panic!("full node {id} not in flattened forest"))
            .related
            .clone()
    }

    #[test]
    fn annotation_surfaces_cross_cutting_related_to_neighbour() {
        // ITERATION-001 implements two RFCs (multi-parent) and is related-to a
        // STORY that sits on no implements path to it. The related-to link must
        // surface as an annotation on the iteration node.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/stories/STORY-009-side.md",
                &doc_md("Side", "story", "[]"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: RFC-001\n- implements: RFC-002\n- related-to: STORY-009",
                ),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            related_of(&nodes, "ITERATION-001"),
            vec!["STORY-009".to_string()],
            "cross-cutting related-to neighbour is annotated on the node"
        );
    }

    #[test]
    fn annotation_excludes_related_target_on_implements_lineage() {
        // ITERATION-001 implements RFC-001 (a direct implements edge, drawn as a
        // tree edge) AND declares related-to RFC-001. The related-to link must
        // NOT be annotated: RFC-001 is on the node's implements lineage, so it is
        // already visually edge-connected, not a cross-cutting link.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: RFC-001\n- related-to: RFC-001",
                ),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert!(
            related_of(&nodes, "ITERATION-001").is_empty(),
            "a related target on the node's implements lineage is not annotated"
        );
        assert!(
            related_of(&nodes, "RFC-001").is_empty(),
            "the parent end is also on the lineage, so also not annotated"
        );
    }

    #[test]
    fn annotation_excludes_transitive_implements_ancestor() {
        // ITERATION-001 implements STORY-001 implements RFC-001, and is also
        // related-to its grandparent RFC-001. RFC-001 is a transitive ancestor on
        // the implements lineage, so the related-to is not a cross-cutting link.
        // This mirrors resolve_chain excluding chain_paths (full ancestor set).
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: STORY-001\n- related-to: RFC-001",
                ),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert!(
            related_of(&nodes, "ITERATION-001").is_empty(),
            "a transitive implements ancestor is not a cross-cutting annotation"
        );
    }

    #[test]
    fn annotation_surfaces_sibling_root_cross_cutting_link() {
        // Two roots in separate implements trees, linked only by related-to. No
        // implements path between them, so the link IS cross-cutting and must be
        // surfaced on both ends (the Story AC's "connected only by related-to").
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-a.md",
                &doc_md("A", "rfc", "- related-to: RFC-002"),
            ),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            related_of(&nodes, "RFC-001"),
            vec!["RFC-002".to_string()],
            "cross-cutting link between unrelated roots is surfaced"
        );
        assert_eq!(
            related_of(&nodes, "RFC-002"),
            vec!["RFC-001".to_string()],
            "and on the reverse end too"
        );
    }

    #[test]
    fn annotation_surfaces_sibling_under_shared_root_cross_cutting_link() {
        // B and C both implement A (siblings under a shared root) and B is
        // related-to C. There is NO implements path between B and C — they reach
        // each other only via the shared ancestor A — so the related-to link is
        // cross-cutting and MUST surface on both ends. A is an implements parent
        // of both, so it must not appear in either annotation set. This is the
        // Story AC's "connected only by a related-to link (no implements path
        // between them)" case, and the defect the descendant descent in the old
        // lineage walk introduced (now ancestors-only in ancestors_of).
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            (
                "docs/stories/STORY-001-b.md",
                &doc_md(
                    "B",
                    "story",
                    "- implements: RFC-001\n- related-to: STORY-002",
                ),
            ),
            (
                "docs/stories/STORY-002-c.md",
                &doc_md("C", "story", "- implements: RFC-001"),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            related_of(&nodes, "STORY-001"),
            vec!["STORY-002".to_string()],
            "sibling C surfaces on B; the shared ancestor A does not"
        );
        assert_eq!(
            related_of(&nodes, "STORY-002"),
            vec!["STORY-001".to_string()],
            "and the reverse end surfaces B; A is excluded as an implements parent"
        );
    }

    #[test]
    fn annotation_empty_when_no_related_to_links() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert!(related_of(&nodes, "RFC-001").is_empty());
        assert!(related_of(&nodes, "STORY-001").is_empty());
    }

    #[test]
    fn annotation_surfaces_on_reverse_related_to_end() {
        // The link is declared on STORY-009 (related-to RFC-001). RFC-001's
        // annotation comes via the reverse link, so it surfaces STORY-009; the
        // forward end STORY-009 surfaces RFC-001. Both ends are full nodes here
        // (separate trees) — neither is on the other's implements path.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            (
                "docs/stories/STORY-009-side.md",
                &doc_md("Side", "story", "- related-to: RFC-001"),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            related_of(&nodes, "RFC-001"),
            vec!["STORY-009".to_string()],
            "reverse related-to end surfaces the neighbour"
        );
        assert_eq!(related_of(&nodes, "STORY-009"), vec!["RFC-001".to_string()],);
    }

    #[test]
    fn back_reference_node_carries_no_annotation() {
        // ITERATION-001 implements two roots (rendered full then as a
        // back-reference) and is related-to a side STORY. The annotation lives
        // on the full node; the back-reference re-encounter carries none.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/stories/STORY-009-side.md",
                &doc_md("Side", "story", "[]"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: RFC-001\n- implements: RFC-002\n- related-to: STORY-009",
                ),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        let back_ref = nodes
            .iter()
            .find(|n| n.reference && id_of(n) == "ITERATION-001")
            .expect("iteration back-reference present");
        assert!(
            back_ref.related.is_empty(),
            "back-reference re-encounters carry no annotation"
        );
        assert_eq!(
            related_of(&nodes, "ITERATION-001"),
            vec!["STORY-009".to_string()],
            "full node still carries the annotation"
        );
    }

    #[test]
    fn annotation_matches_context_related_set_when_ancestors_have_no_related_links() {
        // Parity with the `context` command holds ONLY for the narrow case where
        // the node's `implements` ancestors declare no related-to links. Here
        // ITERATION-001 implements RFC-001 (which has `related: []`) and is
        // related-to STORY-009. With no ancestor related-to links to add, the
        // node's own depth-1 cross-cutting set coincides with
        // `resolve_chain(id, 1).related`. The graph annotation is the node's OWN
        // depth-1 set in general — NOT blanket `resolve_chain` equality (see
        // annotation_differs_from_context_when_ancestor_has_related_links).
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-a.md", &doc_md("A", "rfc", "[]")),
            ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            (
                "docs/stories/STORY-009-side.md",
                &doc_md("Side", "story", "[]"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: RFC-001\n- related-to: STORY-009",
                ),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        // resolve_chain(ITERATION-001, 1) excludes its chain_paths
        // ({ITERATION-001, RFC-001}) from `related`, leaving the cross-cutting
        // STORY-009 — the same set the graph annotation carries, because RFC-001
        // (the ancestor) declares no related-to links of its own.
        let chain = crate::engine::context::resolve_chain(&store, "ITERATION-001", 1).unwrap();
        let context_related: Vec<String> = chain
            .related
            .iter()
            .map(|r| r.doc.id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        assert_eq!(
            related_of(&nodes, "ITERATION-001"),
            context_related,
            "with no ancestor related-to links, the annotation coincides with the \
             context command's depth-1 related set"
        );
        assert_eq!(context_related, vec!["STORY-009".to_string()]);
    }

    #[test]
    fn annotation_is_node_own_depth_one_set_not_ancestor_related_links() {
        // The honest contract: a node's annotation is ITS OWN depth-1 cross-cutting
        // related-to set, NOT the broader set `resolve_chain` reports (which also
        // pulls in the related-to links of the node's `implements` ancestors).
        // RFC-001 (root) is related-to RFC-009; STORY-001 implements RFC-001 and is
        // related-to STORY-009. resolve_chain(STORY-001, 1).related surfaces BOTH
        // STORY-009 (own link) AND RFC-009 (reached via ancestor RFC-001), but the
        // graph annotation on STORY-001 must be only STORY-009.
        let (_tmp, store) = store_from(&[
            (
                "docs/rfcs/RFC-001-root.md",
                &doc_md("Root", "rfc", "- related-to: RFC-009"),
            ),
            ("docs/rfcs/RFC-009-aside.md", &doc_md("Aside", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md(
                    "Mid",
                    "story",
                    "- implements: RFC-001\n- related-to: STORY-009",
                ),
            ),
            (
                "docs/stories/STORY-009-side.md",
                &doc_md("Side", "story", "[]"),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store), &store);

        assert_eq!(
            related_of(&nodes, "STORY-001"),
            vec!["STORY-009".to_string()],
            "annotation is the node's own depth-1 link, not its ancestor's RFC-009"
        );

        // Prove the divergence is real: resolve_chain pulls in the ancestor's link.
        let chain = crate::engine::context::resolve_chain(&store, "STORY-001", 1).unwrap();
        let context_related: BTreeSet<String> =
            chain.related.iter().map(|r| r.doc.id.clone()).collect();
        assert!(
            context_related.contains("RFC-009"),
            "resolve_chain surfaces the ancestor's related-to link, so the sets differ"
        );
    }
}
