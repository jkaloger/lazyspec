use crate::engine::document::{DocMeta, RelationType, Status};
use crate::engine::store::Store;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::HashMap;

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

pub struct Graph {
    inner: petgraph::Graph<String, EdgeKind>,
    #[allow(dead_code)]
    index: HashMap<String, NodeIndex>,
}

impl Graph {
    /// Pure-data constructor. Builds typed edges from `DocMeta.related` filtered
    /// by `RelationType::{Implements, Blocks}`. Other relation types are ignored.
    pub fn from_documents(docs: &[DocMeta]) -> Self {
        let mut inner = petgraph::Graph::<String, EdgeKind>::new();
        let mut index: HashMap<String, NodeIndex> = HashMap::new();

        for doc in docs {
            let idx = inner.add_node(doc.id.clone());
            index.insert(doc.id.clone(), idx);
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

        Graph { inner, index }
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
}

pub fn is_terminal(doc: &DocMeta) -> bool {
    match doc.doc_type.as_str() {
        "rfc" | "story" => matches!(
            doc.status,
            Status::Complete | Status::Superseded | Status::Rejected
        ),
        "iteration" | "audit" => matches!(doc.status, Status::Complete),
        "adr" | "convention" | "dictum" => {
            matches!(doc.status, Status::Accepted | Status::Superseded)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::{DocType, Relation};
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
            ("audit", Status::Complete, true),
            ("audit", Status::Accepted, false),
        ];

        for (ty, status, expected) in cases {
            let d = doc("X", ty, status.clone(), &[], &[]);
            assert_eq!(
                is_terminal(&d),
                expected,
                "expected is_terminal({}, {:?}) == {}",
                ty,
                status,
                expected
            );
        }
    }

    #[test]
    fn is_terminal_rejects_accepted_for_work_item_types() {
        let story = doc("S", "story", Status::Accepted, &[], &[]);
        let iter = doc("I", "iteration", Status::Accepted, &[], &[]);

        assert!(!is_terminal(&story));
        assert!(!is_terminal(&iter));
    }
}
