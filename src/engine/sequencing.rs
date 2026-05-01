use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType, RelationType};
use crate::engine::store::Store;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Blocks,
    Implements,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeRef(pub String);

impl NodeRef {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for NodeRef {
    fn from(s: String) -> Self {
        NodeRef(s)
    }
}

impl From<&str> for NodeRef {
    fn from(s: &str) -> Self {
        NodeRef(s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Under(String),
    After(String),
}

#[derive(Debug, Clone, Default)]
pub struct Weights(pub HashMap<String, f64>);

#[derive(Debug, Clone)]
pub struct CycleError {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NextOpts {
    pub include_leased: bool,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyKind {
    Claimable,
    NeedsChildren,
    NeedsStatusUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyCandidate {
    pub id: String,
    pub kind: ReadyKind,
    pub lessee: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bottleneck {
    pub id: String,
    pub gates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphWarning {
    Cycle { ids: Vec<String> },
}

#[derive(Debug, Clone, Default)]
pub struct NextResult {
    pub ready: Vec<ReadyCandidate>,
    pub bottlenecks: Vec<Bottleneck>,
    pub warnings: Vec<GraphWarning>,
}

#[derive(Debug, Clone, Default)]
pub struct LeaseView {
    pub held: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayeredLayout {
    pub layers: Vec<Vec<NodeRef>>,
    pub edges: Vec<(NodeRef, NodeRef, EdgeKind)>,
}

pub struct Graph {
    inner: petgraph::Graph<String, EdgeKind>,
    index: HashMap<String, NodeIndex>,
    doc_types: HashMap<String, String>,
}

impl Graph {
    /// Pure-data constructor. Builds typed edges from `DocMeta.related` filtered
    /// by `RelationType::{Implements, Blocks}`. Other relation types are ignored.
    pub fn from_documents(docs: &[DocMeta]) -> Self {
        let mut inner = petgraph::Graph::<String, EdgeKind>::new();
        let mut index: HashMap<String, NodeIndex> = HashMap::new();
        let mut doc_types: HashMap<String, String> = HashMap::new();

        for doc in docs {
            let idx = inner.add_node(doc.id.clone());
            index.insert(doc.id.clone(), idx);
            doc_types.insert(doc.id.clone(), doc.doc_type.as_str().to_string());
        }

        for doc in docs {
            let Some(&src) = index.get(&doc.id) else {
                continue;
            };
            for rel in &doc.related {
                let kind = match rel.rel_type {
                    RelationType::Blocks => EdgeKind::Blocks,
                    RelationType::Implements => EdgeKind::Implements,
                    _ => continue,
                };
                if let Some(&dst) = index.get(&rel.target) {
                    inner.add_edge(src, dst, kind);
                }
            }
        }

        Graph {
            inner,
            index,
            doc_types,
        }
    }

    /// Production constructor. Delegates to `from_documents` since the canonical
    /// source of typed adjacency is `DocMeta.related`; `Store`'s forward/reverse
    /// link tables are derived from the same data, so reusing them would only
    /// duplicate work.
    pub fn from_store(store: &Store) -> Self {
        let docs: Vec<DocMeta> = store.all_docs().into_iter().cloned().collect();
        Self::from_documents(&docs)
    }

    pub fn node_ids(&self) -> impl Iterator<Item = &str> {
        self.inner.node_weights().map(|s| s.as_str())
    }

    pub fn blocks_edges(&self) -> impl Iterator<Item = (&str, &str)> {
        self.edges_of(EdgeKind::Blocks)
    }

    pub fn implements_edges(&self) -> impl Iterator<Item = (&str, &str)> {
        self.edges_of(EdgeKind::Implements)
    }

    pub fn cycle_check(&self) -> Result<(), CycleError> {
        let sccs = petgraph::algo::tarjan_scc(&self.inner);
        let mut ids: Vec<String> = Vec::new();
        for scc in sccs {
            if scc.len() > 1 {
                for n in scc {
                    ids.push(self.inner.node_weight(n).unwrap().clone());
                }
            } else if let Some(&n) = scc.first() {
                if self.inner.contains_edge(n, n) {
                    ids.push(self.inner.node_weight(n).unwrap().clone());
                }
            }
        }
        if ids.is_empty() {
            Ok(())
        } else {
            Err(CycleError { ids })
        }
    }

    pub fn topo_order(&self) -> Result<Vec<NodeRef>, CycleError> {
        petgraph::algo::toposort(&self.inner, None)
            .map(|order| {
                order
                    .into_iter()
                    .map(|idx| NodeRef(self.inner.node_weight(idx).unwrap().clone()))
                    .collect()
            })
            .map_err(|_| {
                self.cycle_check()
                    .err()
                    .unwrap_or_else(|| CycleError { ids: Vec::new() })
            })
    }

    fn node_idx(&self, id: &str) -> Option<NodeIndex> {
        self.index.get(id).copied()
    }

    fn id_of(&self, idx: NodeIndex) -> &str {
        self.inner.node_weight(idx).unwrap().as_str()
    }

    fn edges_of(&self, kind: EdgeKind) -> impl Iterator<Item = (&str, &str)> {
        self.inner
            .edge_references()
            .filter(move |e| *e.weight() == kind)
            .map(move |e| {
                let src = self.inner.node_weight(e.source()).unwrap().as_str();
                let dst = self.inner.node_weight(e.target()).unwrap().as_str();
                (src, dst)
            })
    }

    pub fn critical_path(&self, scope: Scope, weights: &Weights) -> Vec<NodeRef> {
        // TODO: STORY-120 follow-up — scope filtering for Under/After
        let _ = scope;

        let topo = match petgraph::algo::toposort(&self.inner, None) {
            Ok(order) => order,
            Err(_) => return Vec::new(),
        };
        if topo.is_empty() {
            return Vec::new();
        }

        let mut predecessors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        for e in self.inner.edge_references() {
            if *e.weight() == EdgeKind::Blocks {
                predecessors.entry(e.target()).or_default().push(e.source());
            }
        }

        let mut best: HashMap<NodeIndex, f64> = HashMap::new();
        let mut parent: HashMap<NodeIndex, Option<NodeIndex>> = HashMap::new();

        for n in &topo {
            let id = self.inner.node_weight(*n).unwrap();
            let w = weights.0.get(id).copied().unwrap_or(0.0);
            match predecessors.get(n) {
                None => {
                    best.insert(*n, w);
                    parent.insert(*n, None);
                }
                Some(preds) => {
                    let mut chosen: Option<NodeIndex> = None;
                    let mut chosen_score = f64::NEG_INFINITY;
                    for p in preds {
                        let score = *best.get(p).unwrap_or(&0.0);
                        if score > chosen_score {
                            chosen_score = score;
                            chosen = Some(*p);
                        }
                    }
                    match chosen {
                        Some(p) => {
                            best.insert(*n, w + chosen_score);
                            parent.insert(*n, Some(p));
                        }
                        None => {
                            best.insert(*n, w);
                            parent.insert(*n, None);
                        }
                    }
                }
            }
        }

        let mut sink: Option<NodeIndex> = None;
        let mut sink_score = f64::NEG_INFINITY;
        for n in &topo {
            let score = *best.get(n).unwrap_or(&0.0);
            if score > sink_score {
                sink_score = score;
                sink = Some(*n);
            }
        }

        let Some(mut cur) = sink else {
            return Vec::new();
        };
        let mut path: Vec<NodeIndex> = vec![cur];
        while let Some(Some(p)) = parent.get(&cur).copied() {
            path.push(p);
            cur = p;
        }
        path.reverse();
        path.into_iter()
            .map(|idx| NodeRef(self.inner.node_weight(idx).unwrap().clone()))
            .collect()
    }

    /// Layered layout for visualization. Sources (no incoming `Blocks` or
    /// `Implements` edges) live at layer 0; every other node sits at
    /// `1 + max(layer(parent))` over both edge kinds, i.e. longest path from
    /// any source. Cycle-affected nodes (SCCs of size > 1, self-loops) are
    /// skipped along with any edge that touches them, consistent with
    /// RFC-041's non-fatal cycle policy.
    pub fn layered_layout(&self) -> LayeredLayout {
        let cycle_nodes: HashSet<NodeIndex> = self.cycle_node_set();

        let mut nodes: Vec<NodeIndex> = self
            .inner
            .node_indices()
            .filter(|n| !cycle_nodes.contains(n))
            .collect();
        // Stable order: by id ascending. Layers are deterministic regardless
        // of insertion order this way.
        nodes.sort_by(|a, b| self.id_of(*a).cmp(self.id_of(*b)));

        let kept: HashSet<NodeIndex> = nodes.iter().copied().collect();

        let mut indeg: HashMap<NodeIndex, usize> = HashMap::new();
        for &n in &nodes {
            let count = self
                .inner
                .edges_directed(n, Direction::Incoming)
                .filter(|e| kept.contains(&e.source()))
                .count();
            indeg.insert(n, count);
        }

        let mut layer: HashMap<NodeIndex, usize> = HashMap::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();
        for &n in &nodes {
            if indeg.get(&n).copied().unwrap_or(0) == 0 {
                layer.insert(n, 0);
                queue.push_back(n);
            }
        }

        while let Some(n) = queue.pop_front() {
            let n_layer = *layer.get(&n).unwrap();
            for e in self.inner.edges_directed(n, Direction::Outgoing) {
                let t = e.target();
                if !kept.contains(&t) {
                    continue;
                }
                let candidate = n_layer + 1;
                let cur = layer.get(&t).copied();
                match cur {
                    Some(existing) if existing >= candidate => {}
                    _ => {
                        layer.insert(t, candidate);
                    }
                }
                let entry = indeg.entry(t).or_insert(0);
                if *entry > 0 {
                    *entry -= 1;
                    if *entry == 0 {
                        queue.push_back(t);
                    }
                }
            }
        }

        let max_layer = layer.values().copied().max().unwrap_or(0);
        let mut layers: Vec<Vec<NodeRef>> = vec![Vec::new(); if layer.is_empty() { 0 } else { max_layer + 1 }];
        let mut placed: Vec<(usize, &str)> = layer
            .iter()
            .map(|(idx, l)| (*l, self.id_of(*idx)))
            .collect();
        placed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        for (l, id) in placed {
            layers[l].push(NodeRef(id.to_string()));
        }

        let mut edges: Vec<(NodeRef, NodeRef, EdgeKind)> = self
            .inner
            .edge_references()
            .filter(|e| kept.contains(&e.source()) && kept.contains(&e.target()))
            .map(|e| {
                (
                    NodeRef(self.id_of(e.source()).to_string()),
                    NodeRef(self.id_of(e.target()).to_string()),
                    *e.weight(),
                )
            })
            .collect();
        edges.sort_by(|a, b| {
            a.0 .0
                .cmp(&b.0 .0)
                .then_with(|| a.1 .0.cmp(&b.1 .0))
                .then_with(|| format!("{:?}", a.2).cmp(&format!("{:?}", b.2)))
        });

        LayeredLayout { layers, edges }
    }

    /// Set of nodes selected by `scope`.
    ///
    /// - `All` — every node in the graph.
    /// - `Under(id)` — the anchor `id` itself, plus its implements-descendants
    ///   (transitive over incoming `Implements` edges, matching the
    ///   source-of-implements-edge convention used elsewhere in this module),
    ///   plus the transitive blocks-ancestors (incoming `Blocks` edges) of
    ///   every member. If `id` does not resolve to a known node, returns an
    ///   empty set.
    /// - `After(id)` — the anchor `id` itself, plus its transitive
    ///   blocks-descendants (outgoing `Blocks` edges). If `id` does not
    ///   resolve to a known node, returns an empty set.
    pub fn scope_membership(&self, scope: &Scope) -> HashSet<NodeRef> {
        match scope {
            Scope::All => self
                .inner
                .node_indices()
                .map(|n| NodeRef(self.id_of(n).to_string()))
                .collect(),
            Scope::Under(id) => {
                let Some(root) = self.node_idx(id) else {
                    return HashSet::new();
                };
                let mut members: HashSet<NodeIndex> = HashSet::new();
                members.insert(root);
                let mut stack = vec![root];
                while let Some(n) = stack.pop() {
                    for e in self.inner.edges_directed(n, Direction::Incoming) {
                        if *e.weight() == EdgeKind::Implements {
                            let child = e.source();
                            if members.insert(child) {
                                stack.push(child);
                            }
                        }
                    }
                }

                let mut out: HashSet<NodeIndex> = members.clone();
                let mut ancestor_stack: Vec<NodeIndex> = members.iter().copied().collect();
                while let Some(n) = ancestor_stack.pop() {
                    for e in self.inner.edges_directed(n, Direction::Incoming) {
                        if *e.weight() == EdgeKind::Blocks {
                            let parent = e.source();
                            if out.insert(parent) {
                                ancestor_stack.push(parent);
                            }
                        }
                    }
                }

                out.into_iter()
                    .map(|n| NodeRef(self.id_of(n).to_string()))
                    .collect()
            }
            Scope::After(id) => {
                let Some(root) = self.node_idx(id) else {
                    return HashSet::new();
                };
                let mut out: HashSet<NodeIndex> = HashSet::new();
                out.insert(root);
                let mut stack = vec![root];
                while let Some(n) = stack.pop() {
                    for e in self.inner.edges_directed(n, Direction::Outgoing) {
                        if *e.weight() == EdgeKind::Blocks {
                            let t = e.target();
                            if out.insert(t) {
                                stack.push(t);
                            }
                        }
                    }
                }
                out.into_iter()
                    .map(|n| NodeRef(self.id_of(n).to_string()))
                    .collect()
            }
        }
    }

    /// True iff `id` resolves to a doc whose type is `iteration`. Used by the
    /// TUI sequencing screen to reject scope anchors that point at an
    /// iteration (scope is defined relative to higher-level work items).
    pub fn is_iteration(&self, id: &str) -> bool {
        self.doc_types
            .get(id)
            .map(|t| t == DocType::ITERATION)
            .unwrap_or(false)
    }

    fn cycle_node_set(&self) -> HashSet<NodeIndex> {
        let mut set: HashSet<NodeIndex> = HashSet::new();
        for scc in petgraph::algo::tarjan_scc(&self.inner) {
            if scc.len() > 1 {
                for n in scc {
                    set.insert(n);
                }
            } else if let Some(&n) = scc.first() {
                if self.inner.contains_edge(n, n) {
                    set.insert(n);
                }
            }
        }
        set
    }
}

/// Smart-traversal entry point. Pure: same input → same output.
///
/// Algorithm summary (RFC-041):
/// 1. Cycle-check first; cycles are reported as warnings and excluded from
///    candidate consideration but do not abort the traversal.
/// 2. Optional `opts.scope` restricts the considered node set to the scope id
///    plus its `Implements`-descendants (transitive children).
/// 3. A node is a candidate iff every incoming `Blocks` edge originates at a
///    terminal node and the node itself is non-terminal.
/// 4. For each candidate, examine its `Implements`-descendants (incoming
///    `Implements` edges, transitive). If any descendant is itself a non-
///    terminal viable candidate, hide the parent and surface those descendants
///    instead. If all descendants are terminal but self is non-terminal,
///    classify as `NeedsStatusUpdate`. Leaf candidates classify by doc type:
///    decomposing types (`rfc`, `story`) → `NeedsChildren`, others →
///    `Claimable`.
/// 5. Apply lease filtering per `opts.include_leased`.
/// 6. Bottlenecks: for each non-terminal node, count downstream non-terminal
///    nodes reachable via outgoing `Blocks` edges. Top three by count
///    (desc, id-asc tiebreak); no padding.
pub fn next_ready(
    graph: &Graph,
    docs: &[DocMeta],
    opts: &NextOpts,
    leases: &LeaseView,
    config: &Config,
) -> NextResult {
    let docs_by_id: HashMap<&str, &DocMeta> =
        docs.iter().map(|d| (d.id.as_str(), d)).collect();

    let mut warnings: Vec<GraphWarning> = Vec::new();
    let mut excluded: HashSet<NodeIndex> = HashSet::new();
    if let Err(cycle) = graph.cycle_check() {
        let mut ids = cycle.ids.clone();
        ids.sort();
        ids.dedup();
        for id in &ids {
            if let Some(idx) = graph.node_idx(id) {
                excluded.insert(idx);
            }
        }
        warnings.push(GraphWarning::Cycle { ids });
    }

    let scope_set: Option<HashSet<NodeIndex>> = opts.scope.as_deref().and_then(|root| {
        graph.node_idx(root).map(|root_idx| {
            let mut set = HashSet::new();
            set.insert(root_idx);
            let mut stack = vec![root_idx];
            while let Some(n) = stack.pop() {
                for e in graph.inner.edges_directed(n, Direction::Incoming) {
                    if *e.weight() == EdgeKind::Implements {
                        let child = e.source();
                        if set.insert(child) {
                            stack.push(child);
                        }
                    }
                }
            }
            set
        })
    });

    let in_scope = |idx: NodeIndex| -> bool {
        if excluded.contains(&idx) {
            return false;
        }
        match &scope_set {
            Some(s) => s.contains(&idx),
            None => true,
        }
    };

    let is_node_terminal = |idx: NodeIndex| -> bool {
        let id = graph.id_of(idx);
        docs_by_id
            .get(id)
            .map(|d| is_terminal(d, config))
            .unwrap_or(false)
    };

    let blocks_cleared = |idx: NodeIndex| -> bool {
        graph
            .inner
            .edges_directed(idx, Direction::Incoming)
            .filter(|e| *e.weight() == EdgeKind::Blocks)
            .all(|e| is_node_terminal(e.source()))
    };

    let implements_children = |idx: NodeIndex| -> Vec<NodeIndex> {
        graph
            .inner
            .edges_directed(idx, Direction::Incoming)
            .filter(|e| *e.weight() == EdgeKind::Implements)
            .map(|e| e.source())
            .collect()
    };

    let implements_descendants = |idx: NodeIndex| -> Vec<NodeIndex> {
        let mut out: Vec<NodeIndex> = Vec::new();
        let mut seen: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();
        for c in implements_children(idx) {
            if seen.insert(c) {
                queue.push_back(c);
            }
        }
        while let Some(n) = queue.pop_front() {
            out.push(n);
            for c in implements_children(n) {
                if seen.insert(c) {
                    queue.push_back(c);
                }
            }
        }
        out
    };

    let classify_leaf = |idx: NodeIndex| -> ReadyKind {
        let id = graph.id_of(idx);
        match docs_by_id.get(id).map(|d| d.doc_type.as_str()) {
            Some("rfc") | Some("story") => ReadyKind::NeedsChildren,
            _ => ReadyKind::Claimable,
        }
    };

    #[allow(clippy::too_many_arguments)]
    fn surface(
        idx: NodeIndex,
        excluded: &HashSet<NodeIndex>,
        scope_check: &dyn Fn(NodeIndex) -> bool,
        is_term: &dyn Fn(NodeIndex) -> bool,
        blocks_ok: &dyn Fn(NodeIndex) -> bool,
        impl_desc: &dyn Fn(NodeIndex) -> Vec<NodeIndex>,
        leaf_kind: &dyn Fn(NodeIndex) -> ReadyKind,
        out: &mut Vec<(NodeIndex, ReadyKind)>,
        seen: &mut HashSet<NodeIndex>,
    ) {
        if excluded.contains(&idx) || !scope_check(idx) {
            return;
        }
        if is_term(idx) {
            return;
        }
        if !blocks_ok(idx) {
            return;
        }

        let descendants = impl_desc(idx);
        let nonterm_desc: Vec<NodeIndex> = descendants
            .iter()
            .copied()
            .filter(|d| !is_term(*d))
            .collect();

        if nonterm_desc.is_empty() {
            let kind = if descendants.is_empty() {
                leaf_kind(idx)
            } else {
                ReadyKind::NeedsStatusUpdate
            };
            if seen.insert(idx) {
                out.push((idx, kind));
            }
            return;
        }

        for d in nonterm_desc {
            surface(
                d, excluded, scope_check, is_term, blocks_ok, impl_desc, leaf_kind, out, seen,
            );
        }
    }

    let mut surfaced: Vec<(NodeIndex, ReadyKind)> = Vec::new();
    let mut seen: HashSet<NodeIndex> = HashSet::new();
    for idx in graph.inner.node_indices() {
        if !in_scope(idx) {
            continue;
        }
        if is_node_terminal(idx) {
            continue;
        }
        if !blocks_cleared(idx) {
            continue;
        }
        // Only kick off surface from a candidate parent; descendants are
        // surfaced recursively from within. To avoid double-walking the same
        // subtree, `seen` dedupes.
        surface(
            idx,
            &excluded,
            &in_scope,
            &is_node_terminal,
            &blocks_cleared,
            &implements_descendants,
            &classify_leaf,
            &mut surfaced,
            &mut seen,
        );
    }

    let mut ready: Vec<ReadyCandidate> = Vec::new();
    for (idx, kind) in surfaced {
        let id = graph.id_of(idx).to_string();
        match leases.held.get(&id) {
            Some(agent) => {
                if opts.include_leased {
                    ready.push(ReadyCandidate {
                        id,
                        kind,
                        lessee: Some(agent.clone()),
                    });
                }
            }
            None => ready.push(ReadyCandidate {
                id,
                kind,
                lessee: None,
            }),
        }
    }
    ready.sort_by(|a, b| a.id.cmp(&b.id));
    ready.dedup_by(|a, b| a.id == b.id);

    let bottlenecks = compute_bottlenecks(graph, &docs_by_id, &excluded, config);

    NextResult {
        ready,
        bottlenecks,
        warnings,
    }
}

fn compute_bottlenecks(
    graph: &Graph,
    docs_by_id: &HashMap<&str, &DocMeta>,
    excluded: &HashSet<NodeIndex>,
    config: &Config,
) -> Vec<Bottleneck> {
    let is_term = |idx: NodeIndex| -> bool {
        let id = graph.inner.node_weight(idx).unwrap().as_str();
        docs_by_id
            .get(id)
            .map(|d| is_terminal(d, config))
            .unwrap_or(false)
    };

    let mut counts: Vec<Bottleneck> = Vec::new();
    for n in graph.inner.node_indices() {
        if excluded.contains(&n) {
            continue;
        }
        if is_term(n) {
            continue;
        }
        let mut seen: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<NodeIndex> = VecDeque::new();
        for e in graph.inner.edges_directed(n, Direction::Outgoing) {
            if *e.weight() == EdgeKind::Blocks {
                let t = e.target();
                if !excluded.contains(&t) && seen.insert(t) {
                    queue.push_back(t);
                }
            }
        }
        let mut gates: usize = 0;
        while let Some(m) = queue.pop_front() {
            if !is_term(m) {
                gates += 1;
            }
            for e in graph.inner.edges_directed(m, Direction::Outgoing) {
                if *e.weight() == EdgeKind::Blocks {
                    let t = e.target();
                    if !excluded.contains(&t) && seen.insert(t) {
                        queue.push_back(t);
                    }
                }
            }
        }
        if gates > 0 {
            let id = graph.inner.node_weight(n).unwrap().clone();
            counts.push(Bottleneck { id, gates });
        }
    }

    counts.sort_by(|a, b| b.gates.cmp(&a.gates).then_with(|| a.id.cmp(&b.id)));
    counts.truncate(3);
    counts
}

pub fn is_terminal(doc: &DocMeta, config: &Config) -> bool {
    let Some(td) = config.type_by_name(doc.doc_type.as_str()) else {
        return false;
    };
    td.resolved_terminal_statuses().contains(&doc.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::{DocType, Relation, Status};
    use chrono::NaiveDate;
    use std::path::PathBuf;

    fn doc(
        id: &str,
        ty: &str,
        status: Status,
        blocks: &[&str],
        implements: &[&str],
    ) -> DocMeta {
        let mut related = Vec::new();
        for t in blocks {
            related.push(Relation {
                rel_type: RelationType::Blocks,
                target: t.to_string(),
            });
        }
        for t in implements {
            related.push(Relation {
                rel_type: RelationType::Implements,
                target: t.to_string(),
            });
        }
        DocMeta {
            id: id.to_string(),
            doc_type: DocType::new(ty),
            title: id.to_string(),
            status,
            author: "test".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            related,
            provenance: vec![],
            validate_ignore: false,
            path: PathBuf::from(format!("docs/{}.md", id)),
            virtual_doc: false,
            priority: None,
        }
    }

    #[test]
    fn from_documents_preserves_all_nodes_and_typed_edges() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "iteration", Status::Draft, &[], &["A"]),
            doc("C", "rfc", Status::Draft, &[], &[]),
        ];

        let g = Graph::from_documents(&docs);

        let mut nodes: Vec<&str> = g.node_ids().collect();
        nodes.sort();
        assert_eq!(nodes, vec!["A", "B", "C"]);

        let blocks: Vec<(&str, &str)> = g.blocks_edges().collect();
        assert_eq!(blocks, vec![("A", "B")]);

        let implements: Vec<(&str, &str)> = g.implements_edges().collect();
        assert_eq!(implements, vec![("B", "A")]);
    }

    #[test]
    fn cycle_check_returns_ok_on_acyclic_blocks_chain() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &[], &[]),
        ];

        let g = Graph::from_documents(&docs);

        assert!(g.cycle_check().is_ok());
    }

    #[test]
    fn cycle_check_reports_offending_ids_on_blocks_cycle() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &["A"], &[]),
        ];

        let g = Graph::from_documents(&docs);
        let err = g.cycle_check().expect_err("expected cycle");

        let mut ids = err.ids.clone();
        ids.sort();
        assert_eq!(ids, vec!["A", "B", "C"]);
    }

    #[test]
    fn topo_order_respects_blocks_predecessor_position() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &["D"], &[]),
            doc("D", "story", Status::Draft, &[], &[]),
        ];

        let g = Graph::from_documents(&docs);
        let order = g.topo_order().expect("acyclic");

        let pos = |id: &str| order.iter().position(|n| n.as_str() == id).unwrap();
        assert!(pos("A") < pos("B"));
        assert!(pos("B") < pos("C"));
        assert!(pos("C") < pos("D"));
    }

    #[test]
    fn critical_path_returns_heaviest_weighted_path_in_diamond() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B", "C"], &[]),
            doc("B", "story", Status::Draft, &["D"], &[]),
            doc("C", "story", Status::Draft, &["D"], &[]),
            doc("D", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let mut w = HashMap::new();
        w.insert("A".to_string(), 1.0);
        w.insert("B".to_string(), 5.0);
        w.insert("C".to_string(), 2.0);
        w.insert("D".to_string(), 1.0);
        let weights = Weights(w);

        let path: Vec<String> = g
            .critical_path(Scope::All, &weights)
            .into_iter()
            .map(|n| n.0)
            .collect();
        let path_refs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
        assert_eq!(path_refs, vec!["A", "B", "D"]);
    }

    #[test]
    fn critical_path_with_equal_weights_returns_edge_respecting_path() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B", "C"], &[]),
            doc("B", "story", Status::Draft, &["D"], &[]),
            doc("C", "story", Status::Draft, &["D"], &[]),
            doc("D", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let mut w = HashMap::new();
        w.insert("A".to_string(), 1.0);
        w.insert("B".to_string(), 1.0);
        w.insert("C".to_string(), 1.0);
        w.insert("D".to_string(), 1.0);
        let weights = Weights(w);

        let path1: Vec<String> = g
            .critical_path(Scope::All, &weights)
            .into_iter()
            .map(|n| n.0)
            .collect();
        let path2: Vec<String> = g
            .critical_path(Scope::All, &weights)
            .into_iter()
            .map(|n| n.0)
            .collect();

        assert!(!path1.is_empty(), "expected non-empty path");
        assert_eq!(path1, path2, "expected stable tie-break");

        let blocks_edges: std::collections::HashSet<(String, String)> = g
            .blocks_edges()
            .map(|(s, d)| (s.to_string(), d.to_string()))
            .collect();
        for pair in path1.windows(2) {
            let edge = (pair[0].clone(), pair[1].clone());
            assert!(
                blocks_edges.contains(&edge),
                "consecutive pair {:?} must be a blocks edge",
                edge
            );
        }
    }

    #[test]
    fn is_terminal_per_type_status_table() {
        let cases: Vec<(&str, Status, bool)> = vec![
            ("rfc", Status::Complete, true),
            ("rfc", Status::Accepted, false),
            ("story", Status::Rejected, true),
            ("iteration", Status::Superseded, false),
            ("iteration", Status::Complete, true),
            ("adr", Status::Accepted, true),
            ("adr", Status::Complete, false),
            ("convention", Status::Superseded, true),
            ("dictum", Status::Accepted, true),
        ];

        let cfg = crate::engine::config::Config::default();
        for (ty, status, expected) in cases {
            let d = doc("X", ty, status.clone(), &[], &[]);
            assert_eq!(
                is_terminal(&d, &cfg),
                expected,
                "expected is_terminal({}, {:?}) == {}",
                ty,
                status,
                expected
            );
        }
    }

    #[test]
    fn is_terminal_audit_uses_spec_defaults_when_type_in_config() {
        let cfg = crate::engine::config::Config::default();
        let complete = doc("A-1", "audit", Status::Complete, &[], &[]);
        let accepted = doc("A-2", "audit", Status::Accepted, &[], &[]);

        assert!(is_terminal(&complete, &cfg));
        assert!(!is_terminal(&accepted, &cfg));
    }

    #[test]
    fn is_terminal_unknown_type_returns_false() {
        let cfg = crate::engine::config::Config::default();
        let d = doc("X", "ghost", Status::Complete, &[], &[]);
        assert!(!is_terminal(&d, &cfg));
    }

    #[test]
    fn is_terminal_rejects_accepted_for_work_item_types() {
        let story = doc("S", "story", Status::Accepted, &[], &[]);
        let iter = doc("I", "iteration", Status::Accepted, &[], &[]);

        let cfg = crate::engine::config::Config::default();
        assert!(!is_terminal(&story, &cfg));
        assert!(!is_terminal(&iter, &cfg));
    }

    #[test]
    fn ac9b_is_terminal_honours_config_override() {
        // Override: story is terminal at status=accepted.
        let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
terminal_statuses = ["accepted"]
"#;
        let cfg = crate::engine::config::Config::parse(toml_str).unwrap();
        let story_accepted = doc("S-1", "story", Status::Accepted, &[], &[]);
        assert!(
            is_terminal(&story_accepted, &cfg),
            "config override should make story=accepted terminal"
        );

        // Same doc under default config: still non-terminal (accepted is not in the
        // RFC-041 spec defaults for story).
        let default_cfg = crate::engine::config::Config::default();
        assert!(
            !is_terminal(&story_accepted, &default_cfg),
            "default config: story=accepted is not terminal"
        );
    }

    fn sorted_ids(result: &NextResult) -> Vec<String> {
        let mut ids: Vec<String> = result.ready.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        ids
    }

    #[test]
    fn next_ready_returns_claimable_for_unblocked_leaf_iterations() {
        let docs = vec![
            doc("I-1", "iteration", Status::Draft, &[], &[]),
            doc("I-2", "iteration", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);
        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert_eq!(sorted_ids(&result), vec!["I-1", "I-2"]);
        for cand in &result.ready {
            assert_eq!(cand.kind, ReadyKind::Claimable, "id={}", cand.id);
            assert!(cand.lessee.is_none(), "id={}", cand.id);
        }
    }

    #[test]
    fn next_ready_classifies_leaf_story_with_no_children_as_needs_children() {
        let docs = vec![doc("S-1", "story", Status::Draft, &[], &[])];
        let g = Graph::from_documents(&docs);

        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert_eq!(result.ready.len(), 1);
        assert_eq!(result.ready[0].id, "S-1");
        assert_eq!(result.ready[0].kind, ReadyKind::NeedsChildren);
    }

    #[test]
    fn next_ready_classifies_parent_with_all_terminal_children_as_needs_status_update() {
        let docs = vec![
            doc("S-1", "story", Status::Draft, &[], &[]),
            doc("I-1", "iteration", Status::Complete, &[], &["S-1"]),
            doc("I-2", "iteration", Status::Complete, &[], &["S-1"]),
        ];
        let g = Graph::from_documents(&docs);

        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert_eq!(result.ready.len(), 1);
        assert_eq!(result.ready[0].id, "S-1");
        assert_eq!(result.ready[0].kind, ReadyKind::NeedsStatusUpdate);
    }

    #[test]
    fn next_ready_hides_parent_and_surfaces_non_terminal_descendants() {
        let docs = vec![
            doc("S-1", "story", Status::Draft, &[], &[]),
            doc("I-1", "iteration", Status::Complete, &[], &["S-1"]),
            doc("I-2", "iteration", Status::Draft, &[], &["S-1"]),
        ];
        let g = Graph::from_documents(&docs);

        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert_eq!(sorted_ids(&result), vec!["I-2"]);
        assert_eq!(result.ready[0].kind, ReadyKind::Claimable);
    }

    #[test]
    fn next_ready_excludes_leased_candidates_by_default() {
        let docs = vec![doc("I-1", "iteration", Status::Draft, &[], &[])];
        let g = Graph::from_documents(&docs);
        let mut leases = LeaseView::default();
        leases
            .held
            .insert("I-1".to_string(), "agent-x".to_string());

        let result = next_ready(&g, &docs, &NextOpts::default(), &leases, &Config::default());

        assert!(result.ready.is_empty());
    }

    #[test]
    fn next_ready_with_include_leased_returns_leased_doc_with_lessee() {
        let docs = vec![doc("I-1", "iteration", Status::Draft, &[], &[])];
        let g = Graph::from_documents(&docs);
        let mut leases = LeaseView::default();
        leases
            .held
            .insert("I-1".to_string(), "agent-x".to_string());
        let opts = NextOpts {
            include_leased: true,
            scope: None,
        };

        let result = next_ready(&g, &docs, &opts, &leases, &Config::default());

        assert_eq!(result.ready.len(), 1);
        assert_eq!(result.ready[0].id, "I-1");
        assert_eq!(result.ready[0].lessee, Some("agent-x".to_string()));
    }

    #[test]
    fn next_ready_returns_top_three_bottlenecks_in_descending_gate_order() {
        // Chain: A→B→C→D→E (Blocks). All non-terminal stories.
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &["D"], &[]),
            doc("D", "story", Status::Draft, &["E"], &[]),
            doc("E", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert_eq!(result.bottlenecks.len(), 3);
        let ids: Vec<&str> = result.bottlenecks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["A", "B", "C"]);
        let gates: Vec<usize> = result.bottlenecks.iter().map(|b| b.gates).collect();
        assert_eq!(gates, vec![4, 3, 2]);
    }

    #[test]
    fn next_ready_returns_fewer_than_three_bottlenecks_when_only_one_gates_downstream() {
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert_eq!(result.bottlenecks.len(), 1);
        assert_eq!(result.bottlenecks[0].id, "A");
    }

    #[test]
    fn next_ready_returns_no_bottlenecks_when_no_node_gates_downstream() {
        let docs = vec![
            doc("I-1", "iteration", Status::Draft, &[], &[]),
            doc("I-2", "iteration", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let result = next_ready(&g, &docs, &NextOpts::default(), &LeaseView::default(), &Config::default());

        assert!(result.bottlenecks.is_empty());
    }

    #[test]
    fn next_ready_combines_lease_filter_descent_hide_and_bottleneck_surfacing() {
        let docs = vec![
            doc("P", "story", Status::Draft, &["X"], &[]),
            doc("C1", "iteration", Status::Complete, &[], &["P"]),
            doc("C2", "iteration", Status::Draft, &[], &["P"]),
            doc("X", "iteration", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);
        let mut leases = LeaseView::default();
        leases.held.insert("C2".to_string(), "alice".to_string());

        let result = next_ready(&g, &docs, &NextOpts::default(), &leases, &Config::default());

        // C2: leased, default opts → excluded.
        // P: has non-terminal descendant C2 → hidden via AC8.
        // X: blocked by non-terminal P → not a candidate.
        // C1: terminal → not a candidate.
        assert!(
            result.ready.is_empty(),
            "expected empty ready, got {:?}",
            result.ready
        );

        // P gates X (downstream non-terminal via Blocks edge).
        let bottleneck_ids: Vec<&str> =
            result.bottlenecks.iter().map(|b| b.id.as_str()).collect();
        assert!(
            bottleneck_ids.contains(&"P"),
            "expected P in bottlenecks, got {:?}",
            bottleneck_ids
        );
    }

    fn layout_index_of(layout: &LayeredLayout, id: &str) -> Option<usize> {
        layout
            .layers
            .iter()
            .position(|layer| layer.iter().any(|n| n.as_str() == id))
    }

    #[test]
    fn layered_layout_assigns_longest_path_layers_across_both_edge_kinds() {
        // Mixed-edge fixture exercising AC1 (both edge kinds present and
        // distinguishable in `edges`) and longest-path layering across kinds.
        //
        // Edge direction recap (consistent with from_documents): a doc that
        // declares `blocks: T` produces edge self→T; same for `implements: T`.
        // Layer assignment treats outgoing edges as "self must come before T",
        // so T is one layer below self.
        //
        // Topology (edges, with kind):
        //   R    →  S    (blocks)        — R is a source
        //   I-1  →  I-2  (blocks)        — I-1 is a source
        //   I-1  →  S    (implements)
        //   I-2  →  S    (implements)
        //
        // Layers (longest path from sources):
        //   R   = 0  (source)
        //   I-1 = 0  (source)
        //   I-2 = layer(I-1) + 1 = 1
        //   S   = max(layer(R), layer(I-1), layer(I-2)) + 1 = 2
        let docs = vec![
            doc("R", "rfc", Status::Draft, &["S"], &[]),
            doc("S", "story", Status::Draft, &[], &[]),
            doc("I-1", "iteration", Status::Draft, &["I-2"], &["S"]),
            doc("I-2", "iteration", Status::Draft, &[], &["S"]),
        ];
        let g = Graph::from_documents(&docs);

        let layout = g.layered_layout();

        // AC1: edges expose both kinds.
        let kinds: HashSet<EdgeKind> = layout.edges.iter().map(|(_, _, k)| *k).collect();
        assert!(kinds.contains(&EdgeKind::Blocks));
        assert!(kinds.contains(&EdgeKind::Implements));

        // Multiple layers exist.
        assert!(layout.layers.len() >= 3);

        // Sources at layer 0; longest-path placement for the rest.
        assert_eq!(layout_index_of(&layout, "R"), Some(0));
        assert_eq!(layout_index_of(&layout, "I-1"), Some(0));
        assert_eq!(layout_index_of(&layout, "I-2"), Some(1));
        assert_eq!(layout_index_of(&layout, "S"), Some(2));

        // All four nodes are placed.
        let total: usize = layout.layers.iter().map(|l| l.len()).sum();
        assert_eq!(total, 4);
    }

    #[test]
    fn layered_layout_skips_cycle_affected_nodes() {
        // A→B→C→A is a 3-cycle (blocks). D blocks E, acyclic.
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &[]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &["A"], &[]),
            doc("D", "story", Status::Draft, &["E"], &[]),
            doc("E", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let layout = g.layered_layout();

        // Cycle nodes absent.
        assert_eq!(layout_index_of(&layout, "A"), None);
        assert_eq!(layout_index_of(&layout, "B"), None);
        assert_eq!(layout_index_of(&layout, "C"), None);

        // Acyclic part survives with correct layering.
        assert_eq!(layout_index_of(&layout, "D"), Some(0));
        assert_eq!(layout_index_of(&layout, "E"), Some(1));

        // No edge in `edges` references a skipped cycle node.
        for (s, t, _) in &layout.edges {
            assert!(!matches!(s.as_str(), "A" | "B" | "C"));
            assert!(!matches!(t.as_str(), "A" | "B" | "C"));
        }
    }

    #[test]
    fn layered_layout_takes_longest_path_when_alternate_routes_exist() {
        // Alternate paths to D: A→D direct vs A→B→C→D. Layer of D must be 3.
        let docs = vec![
            doc("A", "story", Status::Draft, &["B", "D"], &[]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &["D"], &[]),
            doc("D", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let layout = g.layered_layout();

        assert_eq!(layout_index_of(&layout, "A"), Some(0));
        assert_eq!(layout_index_of(&layout, "B"), Some(1));
        assert_eq!(layout_index_of(&layout, "C"), Some(2));
        assert_eq!(layout_index_of(&layout, "D"), Some(3));
    }

    fn ids_of(set: &HashSet<NodeRef>) -> Vec<String> {
        let mut v: Vec<String> = set.iter().map(|n| n.0.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn scope_membership_under_collects_implements_descendants_and_blocks_ancestors() {
        // AC3: Under(S) = S itself, plus implements-descendants of S, plus
        // transitive blocks-ancestors of every member.
        //
        // Topology:
        //   I-1, I-2 implement S
        //   I-1a implements I-1 (nested implements descendant)
        //   B blocks I-1   (blocks-ancestor of I-1 → must appear)
        //   B0 blocks B    (transitive blocks-ancestor → must appear)
        //   X is unrelated → must not appear
        let docs = vec![
            doc("S", "story", Status::Draft, &[], &[]),
            doc("I-1", "iteration", Status::Draft, &[], &["S"]),
            doc("I-2", "iteration", Status::Draft, &[], &["S"]),
            doc("I-1a", "iteration", Status::Draft, &[], &["I-1"]),
            doc("B", "story", Status::Draft, &["I-1"], &[]),
            doc("B0", "story", Status::Draft, &["B"], &[]),
            doc("X", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let members = g.scope_membership(&Scope::Under("S".to_string()));
        let got = ids_of(&members);

        assert_eq!(got, vec!["B", "B0", "I-1", "I-1a", "I-2", "S"]);
    }

    #[test]
    fn scope_membership_under_unknown_anchor_returns_empty() {
        let docs = vec![doc("S", "story", Status::Draft, &[], &[])];
        let g = Graph::from_documents(&docs);

        let members = g.scope_membership(&Scope::Under("NOPE".to_string()));

        assert!(members.is_empty());
    }

    #[test]
    fn scope_membership_after_collects_transitive_blocks_descendants() {
        // AC4: After(A) = A itself, plus transitive blocks-descendants of A.
        // A blocks B, B blocks C → After(A) = {A, B, C}.
        // D blocks E (unrelated) → must not appear.
        // A implements F (implements edge, irrelevant) → F must not appear.
        let docs = vec![
            doc("A", "story", Status::Draft, &["B"], &["F"]),
            doc("B", "story", Status::Draft, &["C"], &[]),
            doc("C", "story", Status::Draft, &[], &[]),
            doc("D", "story", Status::Draft, &["E"], &[]),
            doc("E", "story", Status::Draft, &[], &[]),
            doc("F", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let members = g.scope_membership(&Scope::After("A".to_string()));
        let got = ids_of(&members);

        assert_eq!(got, vec!["A", "B", "C"]);
    }

    #[test]
    fn scope_membership_all_returns_every_node() {
        let docs = vec![
            doc("A", "story", Status::Draft, &[], &[]),
            doc("B", "story", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let members = g.scope_membership(&Scope::All);
        let got = ids_of(&members);

        assert_eq!(got, vec!["A", "B"]);
    }

    #[test]
    fn is_iteration_true_only_for_iteration_doc_type() {
        let docs = vec![
            doc("S-1", "story", Status::Draft, &[], &[]),
            doc("I-1", "iteration", Status::Draft, &[], &[]),
            doc("R-1", "rfc", Status::Draft, &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        assert!(g.is_iteration("I-1"));
        assert!(!g.is_iteration("S-1"));
        assert!(!g.is_iteration("R-1"));
        assert!(!g.is_iteration("missing"));
    }
}
