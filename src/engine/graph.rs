//! Graph forest flattening and sibling ordering, shared by the TUI graph view
//! and the web `/graph` tree render (RFC-052 / STORY-179). Lifted out of the TUI
//! so both consumers share one ordering implementation; this module has NO
//! dependency on the `tui` module.
//!
//! [`resolve_forest`]/`topo_order` (in [`crate::engine::context`]) build the
//! stable topological DAG order; [`flatten_forest`] walks that forest into the
//! flat `Vec<GraphNode>` rendered as a tree, and [`compare_siblings`] sorts each
//! parent's children under the active [`GraphSort`].

use crate::engine::context::ContextNode;
use crate::engine::document::{AttrValue, DocMeta, DocType, Status};
use crate::engine::store::Store;
use crate::engine::traversal::related_neighbours;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One flattened graph row: a document positioned at a tree `depth`, carrying the
/// fields the renderer shows plus its cross-cutting related-role annotations.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub path: PathBuf,
    pub title: String,
    pub doc_type: DocType,
    pub status: Status,
    pub depth: usize,
    /// Doc ids of this node's OWN depth-1 related-role neighbours, minus those on
    /// its chain lineage (its transitive ancestors and descendants, already
    /// drawn as tree edges through the node). Siblings/cousins reachable only
    /// through a shared ancestor ARE included — they have no chain path to
    /// the node, so the link is genuinely cross-cutting. Display-only (RFC-006
    /// Graph mode Phase 1, rendered `┄▷ <id>` by the renderer), sorted for
    /// determinism. This is the node's own depth-1 set, NOT the `context`
    /// command's related set (which also surfaces the related-to links of the
    /// node's ancestors).
    pub related: Vec<String>,
    /// The doc's custom frontmatter attributes (ITERATION-209), copied so the
    /// nested-table renderer and the sibling-sort comparator can read attribute
    /// cells without re-fetching from the store.
    pub attributes: std::collections::BTreeMap<String, AttrValue>,
    /// True when the EDGE this row was reached by is an anchoring-inverted chain
    /// edge (STORY-247): the row is a chain ancestor of its rendered parent, not a
    /// descendant. Anchoring puts a doc on the descendant side of the anchors or
    /// the ancestor side, never both, and inverts either ALL of its parent edges or
    /// none (`ContextNode::parents_inverted`) — so every row emitted for one doc BY
    /// AN EDGE carries the same value. Depth-0 rows were reached by no edge at all
    /// and are never reverse, so one doc CAN carry an unmarked depth-0 row and
    /// marked deeper rows (see
    /// `flatten_anchored_rootless_cycle_re_roots_an_ancestor_unmarked`). The
    /// unanchored forest never sets it.
    pub reverse: bool,
}

/// Budget for an anchored forest's reverse RE-EXPANSION: the rows
/// [`flatten_forest`] spends re-walking an ancestor it has already emitted, past
/// which a reverse re-encounter stops recursing. It counts ONLY those rows — a row
/// emitted on a first encounter never consumes it, forward or reverse, because the
/// `drawn` set already caps first encounters at one per node and forward repeats at
/// one per edge. So store size alone cannot exhaust the budget; only re-expansion
/// can.
///
/// Re-expansion is the one part of the walk with no edge-count bound: a reverse
/// re-encounter recurses (that is what gives every anchor its whole lineage), so L
/// stacked chain diamonds above an anchor have 2^L distinct upward paths and every
/// one is re-drawn — 41 docs in 20 levels emit 2,097,151 rows. Real backlogs are
/// nowhere near that: on this 751-doc repo the largest pivot (`iteration`) emits 973
/// rows, of which 360 are re-expansion rows — ~2.8 rows per anchor over lineages at
/// most 4 rows deep. But the TUI re-flattens on every pivot keystroke, so an
/// unbounded walk is a UI hang waiting for a pathological store.
///
/// 10_000 is ~28x this repo's re-expansion count: far above any hand-authored
/// backlog, far below the point where re-flattening is perceptible. Crossing it is
/// NOT signalled to the viewer — a truncated reverse re-encounter renders as a
/// childless row, indistinguishable from a genuine chain root, with no marker and no
/// message. That silence is exactly why the budget ignores forest size: a store has
/// to be pathologically SHAPED, not merely large, to reach it.
pub const MAX_REVERSE_EXPANSION_ROWS: usize = 10_000;

/// The active sibling sort: the column id (`path`, `status`, or an attribute
/// name) and whether it is reversed. Presentation-only (ITERATION-209): the
/// engine emits a stable topo order; this reorders SIBLINGS within each subtree,
/// never across parent groups. `path` is the identity sort that matches the
/// pre-sort topo tiebreak.
#[derive(Debug, Clone)]
pub struct GraphSort {
    pub col: String,
    pub rev: bool,
}

impl Default for GraphSort {
    fn default() -> Self {
        GraphSort {
            col: "path".to_string(),
            rev: false,
        }
    }
}

/// A comparable sort key for one doc under the active column. Missing/absent
/// values sort LAST regardless of direction, so the comparator is a total order
/// with a deterministic place for blanks. Variants order before `Missing`; the
/// `path` tiebreak (applied separately) keeps the order total even for equal
/// keys.
#[derive(Debug, Clone, PartialEq)]
enum SortKey {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    Missing,
}

impl SortKey {
    /// The key for `doc` under sort column `col`. `path` keys on the file path so
    /// the comparator's tiebreak alone orders it; `status` on the status string;
    /// any other id reads the matching attribute, coercing to a typed key, and is
    /// `Missing` when the attribute is absent (or unrepresentable).
    fn extract(doc: &DocMeta, col: &str) -> SortKey {
        match col {
            "path" => SortKey::Text(doc.path.to_string_lossy().to_string()),
            "status" => SortKey::Text(doc.status.to_string()),
            attr => match doc.attributes.get(attr) {
                Some(AttrValue::Int(i)) => SortKey::Int(*i),
                Some(AttrValue::Float(f)) => SortKey::Float(*f),
                Some(AttrValue::Str(s)) => SortKey::Text(s.clone()),
                Some(AttrValue::Bool(b)) => SortKey::Bool(*b),
                Some(AttrValue::Date(d)) => SortKey::Text(d.format("%Y-%m-%d").to_string()),
                Some(AttrValue::Raw(_)) | None => SortKey::Missing,
            },
        }
    }

    /// Rank of the value-carrying variants for cross-variant ordering. Within a
    /// rank the held values compare; `Missing` ranks highest so blanks sort last.
    fn rank(&self) -> u8 {
        match self {
            SortKey::Int(_) | SortKey::Float(_) => 0,
            SortKey::Text(_) => 1,
            SortKey::Bool(_) => 2,
            SortKey::Missing => 3,
        }
    }

    /// Total order over keys. Mixed numeric variants (Int/Float) compare as
    /// floats; like variants compare naturally; `Missing` is greatest. NaN floats
    /// are treated as equal to keep the order total (they cannot arise from a
    /// parsed config but the comparator must still be total).
    fn cmp_key(&self, other: &SortKey) -> Ordering {
        use SortKey::*;
        match (self, other) {
            (Int(a), Int(b)) => a.cmp(b),
            (Float(a), Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Int(a), Float(b)) => (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal),
            (Float(a), Int(b)) => a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal),
            (Text(a), Text(b)) => a.cmp(b),
            (Bool(a), Bool(b)) => a.cmp(b),
            (Missing, Missing) => Ordering::Equal,
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

/// Compare two siblings under the active sort: by the column key, then `rev`
/// flips that comparison, then `path` is the stable tiebreak (NOT reversed, so
/// the order stays total and deterministic even when `rev` is set). A `Missing`
/// value sorts LAST in both directions because the `rev` flip is applied only to
/// the value comparison while `Missing` keys compare equal among themselves and
/// greatest against any present value — reversing equal-or-greatest still lands
/// them after present values once the path tiebreak settles ties. To guarantee
/// "missing last" under `rev`, missing-vs-present is decided before the flip.
fn compare_siblings(a: &DocMeta, b: &DocMeta, sort: &GraphSort) -> Ordering {
    let ka = SortKey::extract(a, &sort.col);
    let kb = SortKey::extract(b, &sort.col);

    let a_missing = matches!(ka, SortKey::Missing);
    let b_missing = matches!(kb, SortKey::Missing);

    let primary = match (a_missing, b_missing) {
        (true, true) => Ordering::Equal,
        // Missing always sorts last, regardless of `rev`.
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let base = ka.cmp_key(&kb);
            if sort.rev {
                base.reverse()
            } else {
                base
            }
        }
    };

    primary.then_with(|| a.path.cmp(&b.path))
}

/// Flatten the engine's whole-store context forest into the flat
/// `Vec<GraphNode>` the graph view renders. Walks the forest roots-first,
/// depth-first, mirroring the CLI `render_tree` traversal: roots (nodes with no
/// in-graph chain parent) sorted by path, children sorted by path, with
/// `depth` assigned by tree level.
///
/// Each row's `reverse` flag is decided AT THE EDGE it was reached by, from the
/// child's `parents_inverted` flag (an anchored forest inverts ancestor edges —
/// STORY-247). Depth-0 rows carry no marker because they were reached by no edge:
/// that covers the roots and the leftover cycle pass below, which can only
/// re-root a node when every parent that would mark it is itself unreachable from
/// a root, and which draws that node's marked row anyway once the walk reaches it
/// through such a parent.
///
/// A node reachable by more than one parent (a diamond) is drawn in full on first
/// encounter. On a later FORWARD encounter it is re-emitted as the plain doc row
/// (full title/status/attrs, never a "see above" back-reference) WITHOUT
/// recursing — its descendant subtree was already drawn under the first parent.
/// On a later REVERSE encounter it DOES recurse, so every anchor shows its whole
/// upward lineage instead of a childless ancestor row; repeating an ancestor under
/// each anchor is the point of an upward pivot (a leaf pivot is otherwise a flat
/// list). `drawn` therefore means only "emitted at least once" — it no longer
/// implies the subtree below the node has been drawn.
///
/// The output contract is therefore: every forest node is emitted at least once,
/// and a node reached by a REVERSE edge is drawn in full — its row plus the lineage
/// below it — once per rendered PARENT ROW. A 3-cycle above four anchors draws each
/// of its nodes four times, once under each anchor. Uniqueness is likewise per
/// parent ROW, not per parent DOC: the same (parent doc, child doc) pair recurs
/// whenever the parent doc itself recurs under a different anchor (61 pairs do on
/// this repo's `iteration` pivot, one of them 7 times), and what never happens is
/// one emitted parent row listing the same child twice.
///
/// Termination rests on `on_stack` alone: a re-encounter of a node still on the
/// current DFS path is dropped entirely (cycles never render a back-edge row), so
/// no DFS path can hold a node twice and every path is bounded by the node count.
/// Reverse recursion re-walks ancestor chains only, never descendant subtrees (an
/// inverted node's forest children are themselves inverted) — but that bounds the
/// re-walked REGION, not the work inside it: stacked chain diamonds in that region
/// multiply upward paths as 2^L, so unlike the `drawn`-capped forward walk the row
/// count has no edge-count bound. [`MAX_REVERSE_EXPANSION_ROWS`] caps it, counting
/// only the rows a reverse RE-encounter emits: past that many, a reverse
/// re-encounter truncates like a forward one, so a pathological store degrades to
/// short lineages instead of hanging the caller. A truncated row is not flagged — it
/// is a childless row that reads exactly like a genuine chain root — so the budget
/// deliberately excludes first-encounter rows, leaving forest size unable to trigger
/// that silent degradation on its own. No store under the budget renders
/// differently, and which rows lose their lineage is deterministic — the walk order
/// is the sorted root and sibling order, not a HashMap's. Cyclic SCCs with no root
/// are emitted as depth-0 subtrees after the root pass, so the render is complete
/// and terminates.
///
/// Each full node also carries its depth-1 related neighbours as a display-only
/// annotation set (RFC-006 Graph mode Phase 1), sourced from the store's
/// `forward_links`/`reverse_links` filtered to the triples the config gives the
/// related traversal role — see [`related_neighbours`].
/// A neighbour that lies on the node's own chain lineage — a transitive
/// ancestor (parent, grandparent, …) or a transitive descendant (child,
/// grandchild, …) — is excluded, because such a link is already drawn as a tree
/// edge through this node and is not cross-cutting. Everything else IS surfaced,
/// including siblings/cousins reachable only through a SHARED ANCESTOR (e.g. two
/// docs that both take the same root as a chain parent and are `related-to` each
/// other): there is no chain path between them, so the link is genuinely
/// cross-cutting and the Story AC ("connected only by a related-to link, no
/// implements path between them") requires it to surface.
///
/// The exclusion is the node's OWN ancestors and descendants only — see
/// [`chain_lineage_of`] for why the up/down walks must stay independent so a
/// shared ancestor's other children are not swept in. The ancestor direction
/// matches the upward BFS `chain_paths` exclusion in `resolve_chain`
/// (`engine/context.rs`). Note this annotation set is therefore NOT equal in
/// general to `resolve_chain(id, 1).related`: `resolve_chain` also surfaces the
/// related-role links of the node's chain ancestors, whereas this annotation
/// is strictly the node's OWN depth-1 cross-cutting set.
pub fn flatten_forest(forest: &[ContextNode], store: &Store, sort: &GraphSort) -> Vec<GraphNode> {
    // child adjacency: parent path -> child paths.
    let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for node in forest {
        for parent in &node.parents {
            children
                .entry(parent.clone())
                .or_default()
                .push(node.doc.path.clone());
        }
    }

    let by_path: HashMap<&PathBuf, &ContextNode> =
        forest.iter().map(|n| (&n.doc.path, n)).collect();

    // Sibling sort (ITERATION-209): reorder each parent's children by the active
    // column with a `path` tiebreak. Sibling-scoped — parent grouping and the
    // engine's topo order are preserved; only the order WITHIN a sibling set
    // changes. A path with no node in `by_path` falls back to a path compare.
    let sort_siblings = |paths: &mut [PathBuf]| {
        paths.sort_by(|a, b| match (by_path.get(a), by_path.get(b)) {
            (Some(na), Some(nb)) => compare_siblings(na.doc, nb.doc, sort),
            _ => a.cmp(b),
        });
    };
    for kids in children.values_mut() {
        sort_siblings(kids);
    }

    // Per-node chain lineage: the node's own transitive ancestors AND
    // descendants. A related-role neighbour on this set is already drawn as a tree
    // edge through the node, so it is not re-surfaced as a cross-cutting
    // annotation. Siblings and cousins reached only via a shared ancestor are NOT
    // on the lineage and ARE surfaced (see flatten_forest / chain_lineage_of
    // doc-comments).
    let parents_of: HashMap<&PathBuf, &Vec<PathBuf>> =
        forest.iter().map(|n| (&n.doc.path, &n.parents)).collect();
    let lineage: HashMap<&PathBuf, HashSet<PathBuf>> = forest
        .iter()
        .map(|n| {
            (
                &n.doc.path,
                chain_lineage_of(&n.doc.path, &parents_of, &children),
            )
        })
        .collect();

    let mut roots: Vec<&ContextNode> = forest.iter().filter(|n| n.parents.is_empty()).collect();
    roots.sort_by(|a, b| compare_siblings(a.doc, b.doc, sort));

    let mut out: Vec<GraphNode> = Vec::with_capacity(forest.len());
    let mut drawn: HashSet<PathBuf> = HashSet::new();
    let mut on_stack: HashSet<PathBuf> = HashSet::new();
    let mut reverse_rows: usize = 0;

    for root in roots {
        walk(
            root,
            0,
            false,
            &children,
            &by_path,
            &lineage,
            store,
            &mut drawn,
            &mut on_stack,
            &mut reverse_rows,
            &mut out,
        );
    }

    // Cyclic input can leave a strongly-connected component with no root, so
    // the root pass never reaches it. Emit any still-undrawn node as a depth-0
    // subtree (forest order is topological/path-broken) so the render is complete.
    // `drawn` now means only "emitted at least once", so this pass covers exactly
    // the nodes no row mentions yet — not "not yet drawn in full".
    // `reverse: false` like a root: a node re-rooted here was reached by no edge.
    // Anchoring can make that node an inverted ancestor (only when the anchor that
    // owns it sits in a rootless cycle), and it then gets its marked row too, once
    // the walk descends to it from that anchor.
    for node in forest {
        if !drawn.contains(&node.doc.path) {
            walk(
                node,
                0,
                false,
                &children,
                &by_path,
                &lineage,
                store,
                &mut drawn,
                &mut on_stack,
                &mut reverse_rows,
                &mut out,
            );
        }
    }

    out
}

/// The node's own chain lineage: its transitive ANCESTORS (parents,
/// grandparents, …) and its transitive DESCENDANTS (children, grandchildren, …),
/// with the node's own path excluded. Used to exclude already-edge-connected
/// related-to neighbours (links drawn as a tree edge through this node) from the
/// cross-cutting annotation set.
///
/// Crucially this is two INDEPENDENT walks seeded at `path` — an upward pass over
/// parents and a downward pass over children — that never cross-pollinate. The
/// up-walk never descends and the down-walk never ascends, so an ANCESTOR's other
/// children (the node's siblings/cousins) are never collected. That is the whole
/// fix: a sibling reached only through a shared ancestor has no chain path
/// to the node, so its related-to link is genuinely cross-cutting and must
/// surface (the Story AC's "connected only by a related-to link"). The earlier
/// single-stack walk pushed both parents and children of every popped node, so
/// from a shared ancestor it descended into siblings and wrongly excluded them.
///
/// The ancestor direction mirrors the upward BFS that builds `chain_paths` in
/// `resolve_chain` (`engine/context.rs`). The descendant direction is also
/// excluded because a direct/transitive child connected by a drawn chain
/// edge is on the tree, not cross-cutting (e.g. a root that both parents and is
/// related-to its child must not annotate that child). Bounded by the forest
/// size; the seen sets guard cycles.
fn chain_lineage_of(
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

/// The node's OWN depth-1 cross-cutting related neighbours as doc ids, sorted,
/// excluding any neighbour on the node's chain lineage (a transitive
/// ancestor or descendant, already drawn as a tree edge through the node). Takes
/// its neighbours from [`related_neighbours`], the same engine walk
/// `resolve_chain`'s related BFS steps, but is NOT equal to `context <id>
/// --json`'s `related` in general: that set also surfaces the related links of
/// the node's ancestors, whereas this is the node's own depth-1 set only (see
/// `flatten_forest` doc-comment).
fn related_annotations(path: &Path, lineage: &HashSet<PathBuf>, store: &Store) -> Vec<String> {
    let ids: BTreeSet<String> = related_neighbours(store, path)
        .into_iter()
        .filter(|neighbour| !lineage.contains(&neighbour.doc.path))
        .map(|neighbour| neighbour.doc.id.clone())
        .collect();
    ids.into_iter().collect()
}

#[allow(clippy::too_many_arguments)]
fn walk(
    node: &ContextNode,
    depth: usize,
    reverse: bool,
    children: &HashMap<PathBuf, Vec<PathBuf>>,
    by_path: &HashMap<&PathBuf, &ContextNode>,
    lineage: &HashMap<&PathBuf, HashSet<PathBuf>>,
    store: &Store,
    drawn: &mut HashSet<PathBuf>,
    on_stack: &mut HashSet<PathBuf>,
    // Rows emitted so far by a reverse RE-encounter: the only rows
    // MAX_REVERSE_EXPANSION_ROWS budgets.
    reverse_rows: &mut usize,
    out: &mut Vec<GraphNode>,
) {
    let doc = node.doc;
    let node_lineage = lineage.get(&doc.path);
    let empty = HashSet::new();

    // A node still on the current DFS path closes a cycle: drop it entirely, no
    // back-edge row. This guard is what makes the walk terminate.
    if on_stack.contains(&doc.path) {
        return;
    }
    let repeat = !drawn.insert(doc.path.clone());

    out.push(GraphNode {
        path: doc.path.clone(),
        title: doc.title.clone(),
        doc_type: doc.doc_type.clone(),
        status: doc.status.clone(),
        depth,
        related: related_annotations(&doc.path, node_lineage.unwrap_or(&empty), store),
        attributes: doc.attributes.clone(),
        reverse,
    });

    // A FORWARD re-encounter (a diamond) stops here: the row stands alone because
    // the node's descendant subtree was already drawn under the first parent. A
    // REVERSE re-encounter recurses again so this anchor shows the ancestor's own
    // ancestors too, rather than a truncated lineage — until the re-expansion budget
    // is spent, past which it truncates like a forward one so a pathological store
    // cannot run away. The row just pushed is itself re-expansion work, so it counts
    // against the budget whether or not it goes on to recurse; rows the walk would
    // emit anyway (first encounters, forward repeats) never do.
    if repeat && reverse {
        *reverse_rows += 1;
    }
    if repeat && (!reverse || *reverse_rows >= MAX_REVERSE_EXPANSION_ROWS) {
        return;
    }
    on_stack.insert(doc.path.clone());

    if let Some(kids) = children.get(&doc.path) {
        for child_path in kids {
            if let Some(child) = by_path.get(child_path) {
                walk(
                    child,
                    depth + 1,
                    child.parents_inverted,
                    children,
                    by_path,
                    lineage,
                    store,
                    drawn,
                    on_stack,
                    reverse_rows,
                    out,
                );
            }
        }
    }

    on_stack.remove(&doc.path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        Config, EdgeDef, RelSelector, RelationshipDef, Traversal, TypeSelector,
    };
    use crate::engine::context::resolve_forest;
    use crate::engine::store::test_support::{
        doc_md, store_from_with_config, stories_mention_rfcs,
    };
    use crate::engine::store::{extract_id_from_name, Store};
    use tempfile::TempDir;

    /// [`store_from_with`] under the starter config, for the tests that do not
    /// declare traversal roles of their own.
    fn store_from(files: &[(&str, &str)]) -> (TempDir, Store) {
        store_from_with(files, Config::default())
    }

    fn store_from_with(files: &[(&str, &str)], config: Config) -> (TempDir, Store) {
        store_from_with_config(files, &config)
    }

    /// The doc id of a `GraphNode` (derived from its path file stem).
    fn id_of(node: &GraphNode) -> String {
        let stem = node.path.file_stem().unwrap().to_string_lossy();
        extract_id_from_name(&stem)
    }

    /// Collapse a flattened node list to `(id, depth)` pairs for exact-sequence
    /// assertions.
    fn triples(nodes: &[GraphNode]) -> Vec<(String, usize)> {
        nodes.iter().map(|n| (id_of(n), n.depth)).collect()
    }

    /// Like [`triples`] plus the reverse-edge marker, for anchored forests.
    fn marked_rows(nodes: &[GraphNode]) -> Vec<(String, usize, bool)> {
        nodes
            .iter()
            .map(|n| (id_of(n), n.depth, n.reverse))
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0),
                ("STORY-001".to_string(), 1),
                ("ITERATION-001".to_string(), 2),
            ]
        );
    }

    #[test]
    fn flatten_diamond_draws_shared_node_then_repeats_as_plain_doc() {
        // RFC-001 is the root; STORY-001 and STORY-002 both implement it;
        // ITERATION-001 implements both stories (the diamond). Under the second
        // story the leaf re-appears as the plain doc (full row, no back-ref).
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0),
                ("STORY-001".to_string(), 1),
                ("ITERATION-001".to_string(), 2),
                ("STORY-002".to_string(), 1),
                ("ITERATION-001".to_string(), 2),
            ],
            "shared leaf is full under STORY-001, repeated as a plain doc under STORY-002"
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0),
                ("STORY-001".to_string(), 1),
                ("RFC-002".to_string(), 0),
                ("STORY-002".to_string(), 1),
            ],
            "roots are emitted path-sorted, each followed by its subtree"
        );
    }

    #[test]
    fn flatten_multi_parent_node_retains_both_edges() {
        // ITERATION-001 implements two roots; both edges must surface, so the
        // leaf appears under each parent (full once, then again as a plain doc).
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            triples(&nodes),
            vec![
                ("RFC-001".to_string(), 0),
                ("ITERATION-001".to_string(), 1),
                ("RFC-002".to_string(), 0),
                ("ITERATION-001".to_string(), 1),
            ],
            "both implements edges render: full under RFC-001, plain doc under RFC-002"
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        // Cycle has no root, so the leftover pass draws RFC-001 full at depth 0,
        // recurses into child RFC-002 full at depth 1, then re-encounters RFC-001
        // which is on the current DFS path — a cycle back-edge, dropped entirely.
        assert_eq!(
            triples(&nodes),
            vec![("RFC-001".to_string(), 0), ("RFC-002".to_string(), 1),],
            "cycle terminates: each node full once, the back-edge is hidden"
        );
    }

    // --- related-to annotations -------------------------------------------

    /// The `related` annotation set of the first full node with the given id.
    fn related_of(nodes: &[GraphNode], id: &str) -> Vec<String> {
        nodes
            .iter()
            .find(|n| id_of(n) == id)
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            related_of(&nodes, "RFC-001"),
            vec!["STORY-009".to_string()],
            "reverse related-to end surfaces the neighbour"
        );
        assert_eq!(related_of(&nodes, "STORY-009"), vec!["RFC-001".to_string()],);
    }

    #[test]
    fn diamond_repeat_renders_as_plain_doc_never_a_back_reference() {
        // ITERATION-001 implements two roots, so it appears twice. Neither
        // occurrence is a back-reference ("see above" is never shown); both are
        // plain doc rows. It is related-to a side STORY, so both carry that
        // annotation.
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        let occurrences = nodes.iter().filter(|n| id_of(n) == "ITERATION-001").count();
        assert_eq!(
            occurrences, 2,
            "the multi-parent doc appears under each parent"
        );
        assert_eq!(
            related_of(&nodes, "ITERATION-001"),
            vec!["STORY-009".to_string()],
            "the doc carries its annotation"
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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

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

    // --- the related role is declared, not hardcoded (RFC-067) ------------

    /// A config whose ONLY related-role relationship is `mentions`, and which
    /// declares no `related-to` at all: the legacy `[[relationships]]` marker,
    /// under a name the graph used to filter out.
    fn mentions_marked_related() -> Config {
        Config {
            relationships: vec![RelationshipDef {
                name: "mentions".to_string(),
                inverse: None,
                github_native: None,
                traversal: Some(Traversal::Related),
            }],
            ..Config::default()
        }
    }

    #[test]
    fn annotation_follows_the_configured_related_relationship() {
        let (_tmp, store) = store_from_with(
            &[
                (
                    "docs/rfcs/RFC-001-a.md",
                    &doc_md("A", "rfc", "- mentions: RFC-002"),
                ),
                ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            ],
            mentions_marked_related(),
        );

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            related_of(&nodes, "RFC-001"),
            vec!["RFC-002".to_string()],
            "a project whose related relationship is `mentions` gets annotations"
        );
        assert_eq!(related_of(&nodes, "RFC-002"), vec!["RFC-001".to_string()]);
    }

    #[test]
    fn annotation_ignores_a_relationship_no_declaration_gives_the_related_role() {
        // The same store's links renamed to `related-to`, which this config says
        // nothing about: the annotation set is what the config declares, so the
        // name that used to be hardcoded carries no special weight either.
        let (_tmp, store) = store_from_with(
            &[
                (
                    "docs/rfcs/RFC-001-a.md",
                    &doc_md("A", "rfc", "- related-to: RFC-002"),
                ),
                ("docs/rfcs/RFC-002-b.md", &doc_md("B", "rfc", "[]")),
            ],
            mentions_marked_related(),
        );

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert!(related_of(&nodes, "RFC-001").is_empty());
        assert!(related_of(&nodes, "RFC-002").is_empty());
    }

    #[test]
    fn annotation_asks_the_triple_in_the_direction_the_link_was_declared() {
        // STORY-001 -mentions-> RFC-001 is the declared triple; RFC-002
        // -mentions-> STORY-002 is the same relationship the other way round and
        // no row covers it. Both ends of a declared link are annotated (the
        // reverse end asks the DECLARING doc's type as `from`, not its own), and
        // neither end of an undeclared one is.
        let (_tmp, store) = store_from_with(
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
            stories_mention_rfcs(),
        );

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(related_of(&nodes, "STORY-001"), vec!["RFC-001".to_string()]);
        assert_eq!(
            related_of(&nodes, "RFC-001"),
            vec!["STORY-001".to_string()],
            "the reverse end asks story -mentions-> rfc, the direction declared"
        );
        assert!(
            related_of(&nodes, "RFC-002").is_empty(),
            "rfc -mentions-> story is not the declared triple"
        );
        assert!(
            related_of(&nodes, "STORY-002").is_empty(),
            "and its reverse end is not annotated either"
        );
    }

    #[test]
    fn annotation_follows_a_wildcard_row() {
        // The starter shape ADR-031 keeps: one row, both endpoints wildcard.
        let (_tmp, store) = store_from_with(
            &[
                (
                    "docs/rfcs/RFC-001-a.md",
                    &doc_md("A", "rfc", "- related-to: STORY-001"),
                ),
                ("docs/stories/STORY-001-b.md", &doc_md("B", "story", "[]")),
            ],
            Config {
                relationships: Vec::new(),
                edges: vec![EdgeDef {
                    name: "anything-relates-to-anything".to_string(),
                    from: TypeSelector::Any,
                    to: TypeSelector::Any,
                    via: RelSelector::Named(vec!["related-to".to_string()]),
                    required: None,
                    traversal: Some(Traversal::Related),
                }],
                ..Config::default()
            },
        );

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(related_of(&nodes, "RFC-001"), vec!["STORY-001".to_string()]);
        assert_eq!(related_of(&nodes, "STORY-001"), vec!["RFC-001".to_string()]);
    }

    #[test]
    fn annotation_drops_a_neighbour_with_no_document_in_the_store() {
        let (_tmp, store) = store_from(&[(
            "docs/rfcs/RFC-001-a.md",
            &doc_md("A", "rfc", "- related-to: NOPE-001"),
        )]);

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert!(
            related_of(&nodes, "RFC-001").is_empty(),
            "a dangling target has no type to ask the triple about, so it is not a neighbour"
        );
    }

    // --- anchored reverse chain (STORY-247) -------------------------------

    /// `ITERATION-001 implements STORY-001 implements RFC-001`, the chain the
    /// reverse-chain tests pivot on.
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
    fn flatten_iteration_anchor_renders_ancestors_as_inverted_subtree() {
        // AC1 + AC2: the anchor is the root at depth 0, its story depth 1 and its
        // RFC depth 2, with the marker set on the reverse rows only.
        let (_tmp, store) = linear_chain_store();

        let nodes = flatten_forest(
            &resolve_forest(&store, Some("iteration")),
            &store,
            &GraphSort::default(),
        );

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("ITERATION-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, true),
                ("RFC-001".to_string(), 2, true),
            ]
        );
    }

    #[test]
    fn flatten_story_anchor_marks_ancestor_row_but_not_descendant_row() {
        // AC3: both directions under one anchor, each row once, only the upward
        // one marked.
        let (_tmp, store) = linear_chain_store();

        let nodes = flatten_forest(
            &resolve_forest(&store, Some("story")),
            &store,
            &GraphSort::default(),
        );

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("STORY-001".to_string(), 0, false),
                ("ITERATION-001".to_string(), 1, false),
                ("RFC-001".to_string(), 1, true),
            ]
        );
    }

    #[test]
    fn flatten_shared_ancestor_repeats_with_full_lineage_under_each_anchor() {
        // The dominant leaf-pivot shape: two iterations over one story over one RFC.
        // A shared ancestor re-encountered on a REVERSE edge is re-emitted AND
        // re-walked, so the second anchor shows RFC-001 too -- a childless
        // `STORY-001` row there would truncate exactly the lineage the pivot exists
        // to answer ("what story and RFC does each of these serve?").
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
        ]);

        let nodes = flatten_forest(
            &resolve_forest(&store, Some("iteration")),
            &store,
            &GraphSort::default(),
        );

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("ITERATION-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, true),
                ("RFC-001".to_string(), 2, true),
                ("ITERATION-002".to_string(), 0, false),
                ("STORY-001".to_string(), 1, true),
                ("RFC-001".to_string(), 2, true),
            ],
            "the (STORY-001, RFC-001) parent/child pair repeats because STORY-001's \
             own row repeats: the no-duplicates guarantee is per parent ROW, not per \
             parent DOC"
        );
    }

    #[test]
    fn flatten_forward_diamond_repeat_still_does_not_redraw_its_subtree() {
        // The other half of the re-encounter rule, unchanged by STORY-247: a FORWARD
        // re-encounter is a bare repeat row. ITERATION-001 is shared by both stories
        // and has a child of its own, which is drawn only under the first story.
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
                "docs/iterations/ITERATION-001-shared.md",
                &doc_md(
                    "Shared",
                    "iteration",
                    "- implements: STORY-001\n- implements: STORY-002",
                ),
            ),
            (
                "docs/iterations/ITERATION-002-child.md",
                &doc_md("Child", "iteration", "- implements: ITERATION-001"),
            ),
        ]);

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, false),
                ("ITERATION-001".to_string(), 2, false),
                ("ITERATION-002".to_string(), 3, false),
                ("STORY-002".to_string(), 1, false),
                ("ITERATION-001".to_string(), 2, false),
            ],
            "the repeat under STORY-002 carries no subtree"
        );
    }

    #[test]
    fn flatten_unanchored_forest_marks_nothing() {
        // AC6.
        let (_tmp, store) = linear_chain_store();

        let nodes = flatten_forest(&resolve_forest(&store, None), &store, &GraphSort::default());

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, false),
                ("ITERATION-001".to_string(), 2, false),
            ]
        );
    }

    #[test]
    fn flatten_anchored_upward_cycle_terminates_each_node_once() {
        // AC7: the two stories above the anchor implement each other. The inverted
        // edges close a loop, which the existing on_stack guard drops.
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

        let nodes = flatten_forest(
            &resolve_forest(&store, Some("iteration")),
            &store,
            &GraphSort::default(),
        );

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("ITERATION-001".to_string(), 0, false),
                ("STORY-001".to_string(), 1, true),
                ("STORY-002".to_string(), 2, true),
            ],
            "each node full once, the back-edge dropped"
        );
    }

    #[test]
    fn flatten_anchored_rootless_cycle_re_roots_an_ancestor_unmarked() {
        // The one shape where the leftover cycle pass touches an inverted node: the
        // two anchors implement each other, so neither is a root and the pass
        // re-roots whatever comes first in forest (path) order -- here the ancestor
        // RFC-001, at depth 0 and unmarked, because a depth-0 row was reached by no
        // edge. Its marked row still follows under the anchor that owns it.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-top.md", &doc_md("Top", "rfc", "[]")),
            (
                "docs/stories/STORY-001-a.md",
                &doc_md(
                    "A",
                    "story",
                    "- implements: STORY-002\n- implements: RFC-001",
                ),
            ),
            (
                "docs/stories/STORY-002-b.md",
                &doc_md("B", "story", "- implements: STORY-001"),
            ),
        ]);

        let nodes = flatten_forest(
            &resolve_forest(&store, Some("story")),
            &store,
            &GraphSort::default(),
        );

        assert_eq!(
            marked_rows(&nodes),
            vec![
                ("RFC-001".to_string(), 0, false),
                ("STORY-001".to_string(), 0, false),
                ("RFC-001".to_string(), 1, true),
                ("STORY-002".to_string(), 1, false),
            ]
        );
    }

    /// Levels of the stacked-diamond store below. Two stories per level, each
    /// implementing BOTH stories one level up, so the reverse expansion has 2^L
    /// distinct upward paths from the anchor and re-walks every one: 2^(L+1) - 1
    /// rows unbudgeted, 2,097,151 at 20 levels (41 docs). Chosen as the smallest
    /// shape that leaves a wide margin over [`MAX_REVERSE_EXPANSION_ROWS`], so the
    /// row assertion below fails loudly if the budget ever stops being applied.
    const DIAMOND_LEVELS: usize = 20;

    /// `ITERATION-001` under `DIAMOND_LEVELS` levels of two stories, every story
    /// implementing both stories of the level above. Nothing exotic: one relation
    /// (`implements`), declared twice per doc.
    fn stacked_diamond_store() -> (TempDir, Store) {
        let story_id = |level: usize, side: usize| format!("STORY-{:03}", level * 2 + side + 1);
        // The `related` block implementing both stories of `level`, or none when
        // `level` is past the top of the ladder.
        let implements_level = |level: usize| {
            if level < DIAMOND_LEVELS {
                format!(
                    "- implements: {}\n- implements: {}",
                    story_id(level, 0),
                    story_id(level, 1)
                )
            } else {
                "[]".to_string()
            }
        };

        let mut files: Vec<(String, String)> = Vec::new();
        for level in 0..DIAMOND_LEVELS {
            for side in 0..2 {
                let id = story_id(level, side);
                files.push((
                    format!("docs/stories/{id}-node.md"),
                    doc_md(&id, "story", &implements_level(level + 1)),
                ));
            }
        }
        files.push((
            "docs/iterations/ITERATION-001-anchor.md".to_string(),
            doc_md("Anchor", "iteration", &implements_level(0)),
        ));

        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();
        store_from(&refs)
    }

    #[test]
    fn flatten_anchored_reverse_expansion_stops_recursing_at_the_row_budget() {
        // Reverse recursion has no edge-count bound, so a pathological store must
        // degrade (truncated lineages) rather than run away: the TUI re-flattens on
        // every pivot keystroke.
        let (_tmp, store) = stacked_diamond_store();
        let forest = resolve_forest(&store, Some("iteration"));
        let edges: usize = forest.iter().map(|n| n.parents.len()).sum();

        let started = std::time::Instant::now();
        let nodes = flatten_forest(&forest, &store, &GraphSort::default());
        let elapsed = started.elapsed();

        assert!(
            nodes.len() >= MAX_REVERSE_EXPANSION_ROWS,
            "the store must actually cross the budget or this test proves nothing, got {} rows",
            nodes.len()
        );
        // Every row here but the 41 first encounters is re-expansion work, so the
        // budget bounds nearly the whole output: the rows the walk would emit anyway
        // (one per node, plus one per repeat edge) plus the frames already on the DFS
        // stack unwinding, each pending sibling emitting its own row.
        assert!(
            nodes.len() <= MAX_REVERSE_EXPANSION_ROWS + forest.len() + edges,
            "budget exceeded by more than the unwinding walk can add: {} rows",
            nodes.len()
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "flatten_forest must return promptly under the budget, took {elapsed:?}"
        );

        // Deterministic degradation: which rows lose their lineage must follow the
        // sorted walk order, never HashMap iteration order. Re-resolving is part of
        // the check -- a fresh `HashMap` gets a fresh iteration order.
        let again = flatten_forest(
            &resolve_forest(&store, Some("iteration")),
            &store,
            &GraphSort::default(),
        );
        assert_eq!(
            marked_rows(&nodes),
            marked_rows(&again),
            "the same store must always truncate the same rows"
        );
    }

    /// Stories in the wide-forward store below, each an anchor under the `story`
    /// pivot and each re-emitting every iteration as a forward repeat.
    const STORY_FANOUT: usize = 100;

    /// Iterations in the wide-forward store below, each implementing EVERY story.
    /// `STORY_FANOUT * ITERATION_FANOUT` forward rows must exceed
    /// [`MAX_REVERSE_EXPANSION_ROWS`] for the test to bite.
    const ITERATION_FANOUT: usize = 105;

    /// A store whose FORWARD rows alone outnumber [`MAX_REVERSE_EXPANSION_ROWS`]
    /// while its reverse re-expansion stays tiny: `STORY_FANOUT` stories, all
    /// implementing `RFC-001` which implements `RFC-002`, under `ITERATION_FANOUT`
    /// iterations that each implement EVERY story. Under the `story` pivot every
    /// story is an anchor/root, so each iteration is re-emitted as a forward repeat
    /// beneath every one of them — the superlinear-but-legitimate row count a
    /// few-thousand-doc backlog also reaches — while the upward lineage each anchor
    /// re-expands is only two rows.
    fn wide_forward_store() -> (TempDir, Store) {
        let story_id = |i: usize| format!("STORY-{:03}", i + 1);
        let implements_every_story = (0..STORY_FANOUT)
            .map(|i| format!("- implements: {}", story_id(i)))
            .collect::<Vec<_>>()
            .join("\n");

        let mut files: Vec<(String, String)> = vec![
            (
                "docs/rfcs/RFC-001-mid.md".to_string(),
                doc_md("Mid", "rfc", "- implements: RFC-002"),
            ),
            (
                "docs/rfcs/RFC-002-top.md".to_string(),
                doc_md("Top", "rfc", "[]"),
            ),
        ];
        for i in 0..STORY_FANOUT {
            files.push((
                format!("docs/stories/{}-anchor.md", story_id(i)),
                doc_md("Anchor", "story", "- implements: RFC-001"),
            ));
        }
        for i in 0..ITERATION_FANOUT {
            files.push((
                format!("docs/iterations/ITERATION-{:03}-leaf.md", i + 1),
                doc_md("Leaf", "iteration", &implements_every_story),
            ));
        }

        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();
        store_from(&refs)
    }

    #[test]
    fn flatten_reverse_budget_ignores_forward_rows_so_a_wide_store_keeps_its_lineage() {
        // The budget must cover reverse RE-EXPANSION only. A store big enough to emit
        // more than the budget's worth of ordinary forward rows must still show every
        // anchor its whole lineage: a budget counting TOTAL rows would silently drop
        // the tail anchors' `RFC-002` row, which renders as a childless `RFC-001` and
        // reads exactly like a story with no RFC above it.
        let (_tmp, store) = wide_forward_store();
        let forest = resolve_forest(&store, Some("story"));

        let nodes = flatten_forest(&forest, &store, &GraphSort::default());

        assert!(
            nodes.len() > MAX_REVERSE_EXPANSION_ROWS,
            "the store must emit more than the budget in forward rows or this test \
             proves nothing, got {} rows",
            nodes.len()
        );
        let occurrences = |id: &str| nodes.iter().filter(|n| id_of(n) == id).count();
        assert_eq!(
            occurrences("RFC-001"),
            STORY_FANOUT,
            "each anchor draws its inverted parent RFC"
        );
        assert_eq!(
            occurrences("RFC-002"),
            STORY_FANOUT,
            "and re-expands it to the row above, for EVERY anchor: forward rows must \
             not consume the reverse budget"
        );
        assert!(
            nodes
                .iter()
                .filter(|n| id_of(n) == "RFC-002")
                .all(|n| n.reverse && n.depth == 2),
            "every RFC-002 row is a marked depth-2 ancestor of its anchor"
        );
    }

    #[test]
    fn annotation_still_excludes_lineage_across_an_inverted_edge() {
        // An ancestor drawn as an inverted tree edge is still on the node's
        // lineage, so a related-to link along it is not re-surfaced as
        // cross-cutting (STORY-247 non-functional note). The lineage walk is
        // direction-agnostic: it reads the anchored forest's own parent edges.
        let (_tmp, store) = store_from(&[
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "[]"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: STORY-001\n- related-to: STORY-001",
                ),
            ),
        ]);

        let nodes = flatten_forest(
            &resolve_forest(&store, Some("iteration")),
            &store,
            &GraphSort::default(),
        );

        assert!(related_of(&nodes, "ITERATION-001").is_empty());
        assert!(related_of(&nodes, "STORY-001").is_empty());
    }

    // --- ITERATION-209 sibling sort ---------------------------------------

    use chrono::NaiveDate;
    use std::collections::BTreeMap;

    /// A bare `DocMeta` keyed by path and id with the given attribute map. Only
    /// the fields the comparator reads (`path`, `status`, `attributes`) matter.
    fn doc_meta(path: &str, status: &str, attrs: BTreeMap<String, AttrValue>) -> DocMeta {
        DocMeta {
            path: PathBuf::from(path),
            title: path.to_string(),
            doc_type: DocType::new("story"),
            status: Status::new(status),
            author: "t".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: Vec::new(),
            provenance: Vec::new(),
            related: Vec::new(),
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            id: path.to_string(),
            attributes: attrs,
        }
    }

    fn with_int(name: &str, v: i64) -> BTreeMap<String, AttrValue> {
        let mut m = BTreeMap::new();
        m.insert(name.to_string(), AttrValue::Int(v));
        m
    }

    fn sort_on(col: &str, rev: bool) -> GraphSort {
        GraphSort {
            col: col.to_string(),
            rev,
        }
    }

    // AC4: siblings where some lack the sort attribute -> the present values sort
    // by the column, the absent ones sort LAST, deterministically (path tiebreak),
    // in BOTH directions.
    #[test]
    fn comparator_sorts_missing_attribute_last_both_directions() {
        let a = doc_meta("docs/a.md", "draft", with_int("estimate", 5));
        let b = doc_meta("docs/b.md", "draft", with_int("estimate", 1));
        let m1 = doc_meta("docs/m1.md", "draft", BTreeMap::new());
        let m2 = doc_meta("docs/m2.md", "draft", BTreeMap::new());

        let asc = sort_on("estimate", false);
        // ascending: 1 (b), 5 (a), then missing by path (m1, m2).
        assert_eq!(compare_siblings(&b, &a, &asc), Ordering::Less);
        assert_eq!(compare_siblings(&a, &m1, &asc), Ordering::Less);
        assert_eq!(
            compare_siblings(&m1, &m2, &asc),
            Ordering::Less,
            "path tiebreak"
        );

        let desc = sort_on("estimate", true);
        // descending flips present values (5 before 1) but missing STILL last.
        assert_eq!(compare_siblings(&a, &b, &desc), Ordering::Less);
        assert_eq!(
            compare_siblings(&a, &m1, &desc),
            Ordering::Less,
            "present value precedes missing even when reversed"
        );
        assert_eq!(
            compare_siblings(&m1, &m2, &desc),
            Ordering::Less,
            "missing pair still ordered by path, not reversed"
        );
    }

    // AC4 (total order): the comparator is a total order over a mixed set; sorting
    // is stable and idempotent. Sort twice, same result.
    #[test]
    fn comparator_is_total_and_idempotent() {
        let mut docs = [
            doc_meta("docs/c.md", "draft", with_int("e", 3)),
            doc_meta("docs/a.md", "draft", BTreeMap::new()),
            doc_meta("docs/b.md", "draft", with_int("e", 3)),
            doc_meta("docs/d.md", "draft", with_int("e", 1)),
        ];
        let s = sort_on("e", false);
        docs.sort_by(|x, y| compare_siblings(x, y, &s));
        let order1: Vec<_> = docs.iter().map(|d| d.id.clone()).collect();
        // 1 (d), then 3 tie broken by path (b before c), then missing (a).
        assert_eq!(
            order1,
            vec!["docs/d.md", "docs/b.md", "docs/c.md", "docs/a.md"]
        );
        docs.sort_by(|x, y| compare_siblings(x, y, &s));
        let order2: Vec<_> = docs.iter().map(|d| d.id.clone()).collect();
        assert_eq!(order1, order2, "sort is idempotent");
    }

    // AC3 (sibling-scoped): siblings reorder within a parent's children by the
    // active column, but the parent grouping / topo order is preserved.
    #[test]
    fn sibling_sort_reorders_children_within_parent_only() {
        // RFC-001 parents STORY-001 (status review) and STORY-002 (status draft).
        // By path STORY-001 precedes STORY-002; sorting by status reverses them
        // (draft < review) WITHOUT detaching either from RFC-001.
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-a.md",
                &doc_md_status("A", "story", "- implements: RFC-001", "review"),
            ),
            (
                "docs/stories/STORY-002-b.md",
                &doc_md_status("B", "story", "- implements: RFC-001", "draft"),
            ),
        ]);

        let forest = resolve_forest(&store, None);
        let by_status = flatten_forest(&forest, &store, &sort_on("status", false));
        let ids: Vec<(String, usize)> = by_status.iter().map(|n| (id_of(n), n.depth)).collect();
        assert_eq!(
            ids,
            vec![
                ("RFC-001".to_string(), 0),
                ("STORY-002".to_string(), 1),
                ("STORY-001".to_string(), 1),
            ],
            "draft sibling (STORY-002) sorts before review sibling under the same parent"
        );
    }

    /// `doc_md` with an explicit status (the base helper hardcodes `draft`).
    fn doc_md_status(title: &str, doc_type: &str, related: &str, status: &str) -> String {
        let related_block = if related == "[]" {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: {status}\nauthor: t\ndate: 2026-04-01\ntags: []\n{related_block}\n---\n\n{title} body\n"
        )
    }
}
