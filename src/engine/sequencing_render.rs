use crate::engine::document::DocMeta;
use crate::engine::sequencing::{Graph, NodeRef, Scope};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Render the in-scope subgraph as a d2 document. Header is `direction: down`.
/// Each node is `<id>: { label: "<title>" }`. Blocks edges render as
/// `A -> B`; implements edges include `{ style.stroke-dash: 4 }`.
///
/// Nodes whose ids do not resolve in `docs` are skipped (no label available);
/// edges with such endpoints are likewise omitted. Output is deterministic:
/// nodes sorted by id, edges sorted by (src, dst).
pub fn render_d2(graph: &Graph, scope: &Scope, docs: &[DocMeta]) -> String {
    let members = graph.scope_membership(scope);
    let member_ids: HashSet<&str> = members.iter().map(|n| n.0.as_str()).collect();
    let docs_by_id = index_docs(docs);

    let mut out = String::from("direction: down\n");

    let mut node_ids: Vec<&str> = graph
        .node_ids()
        .filter(|id| member_ids.contains(id))
        .collect();
    node_ids.sort();
    for id in &node_ids {
        if let Some(doc) = docs_by_id.get(id) {
            out.push_str(&format!(
                "{}: {{ label: \"{}\" }}\n",
                id,
                escape_label(&doc.title)
            ));
        }
    }

    let mut blocks: Vec<(&str, &str)> = graph
        .blocks_edges()
        .filter(|(s, t)| member_ids.contains(s) && member_ids.contains(t))
        .collect();
    blocks.sort();
    for (s, t) in &blocks {
        out.push_str(&format!("{} -> {}\n", s, t));
    }

    let mut implements: Vec<(&str, &str)> = graph
        .implements_edges()
        .filter(|(s, t)| member_ids.contains(s) && member_ids.contains(t))
        .collect();
    implements.sort();
    for (s, t) in &implements {
        out.push_str(&format!("{} -> {} {{ style.stroke-dash: 4 }}\n", s, t));
    }

    out
}

/// Render the in-scope subgraph as a Graphviz dot document. Implements edges
/// carry `[style=dashed]`; blocks edges are unstyled. Same skip/sort rules as
/// `render_d2`.
pub fn render_dot(graph: &Graph, scope: &Scope, docs: &[DocMeta]) -> String {
    let members = graph.scope_membership(scope);
    let member_ids: HashSet<&str> = members.iter().map(|n| n.0.as_str()).collect();
    let docs_by_id = index_docs(docs);

    let mut node_ids: Vec<&str> = graph
        .node_ids()
        .filter(|id| member_ids.contains(id))
        .collect();
    node_ids.sort();

    let mut blocks: Vec<(&str, &str)> = graph
        .blocks_edges()
        .filter(|(s, t)| member_ids.contains(s) && member_ids.contains(t))
        .collect();
    blocks.sort();

    let mut implements: Vec<(&str, &str)> = graph
        .implements_edges()
        .filter(|(s, t)| member_ids.contains(s) && member_ids.contains(t))
        .collect();
    implements.sort();

    if node_ids.is_empty() && blocks.is_empty() && implements.is_empty() {
        return "digraph G {}\n".to_string();
    }

    let mut out = String::from("digraph G {\n");
    for id in &node_ids {
        if let Some(doc) = docs_by_id.get(id) {
            out.push_str(&format!(
                "  \"{}\" [label=\"{}\"]\n",
                id,
                escape_label(&doc.title)
            ));
        }
    }
    for (s, t) in &blocks {
        out.push_str(&format!("  \"{}\" -> \"{}\"\n", s, t));
    }
    for (s, t) in &implements {
        out.push_str(&format!("  \"{}\" -> \"{}\" [style=dashed]\n", s, t));
    }
    out.push_str("}\n");
    out
}

/// Render the in-scope subgraph as JSON: `{ nodes: [...], edges: [...] }`.
/// Each node carries `id`, `type`, `status`, `priority`. Each edge carries
/// `from`, `to`, `kind` ∈ `"blocks"|"implements"`.
///
/// Nodes whose ids are in `scope_membership` but absent from `docs` emit a
/// minimal entry with empty `type`/`status` and null `priority`. This keeps
/// json strictly schema-stable for downstream consumers, vs. d2/dot which
/// elide unlabelled nodes.
pub fn render_json(graph: &Graph, scope: &Scope, docs: &[DocMeta]) -> String {
    let members = graph.scope_membership(scope);
    let member_ids: HashSet<&str> = members.iter().map(|n| n.0.as_str()).collect();
    let docs_by_id = index_docs(docs);

    let mut node_ids: Vec<&str> = graph
        .node_ids()
        .filter(|id| member_ids.contains(id))
        .collect();
    node_ids.sort();

    let nodes: Vec<NodeJson> = node_ids
        .iter()
        .map(|id| match docs_by_id.get(id) {
            Some(doc) => NodeJson {
                id: id.to_string(),
                ty: doc.doc_type.as_str().to_string(),
                status: doc.status.to_string(),
                priority: doc.priority.clone(),
            },
            None => NodeJson {
                id: id.to_string(),
                ty: String::new(),
                status: String::new(),
                priority: None,
            },
        })
        .collect();

    let mut edge_set: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    for (s, t) in graph.blocks_edges() {
        if member_ids.contains(s) && member_ids.contains(t) {
            edge_set.insert((s.to_string(), t.to_string(), "blocks"));
        }
    }
    for (s, t) in graph.implements_edges() {
        if member_ids.contains(s) && member_ids.contains(t) {
            edge_set.insert((s.to_string(), t.to_string(), "implements"));
        }
    }

    let edges: Vec<EdgeJson> = edge_set
        .into_iter()
        .map(|(from, to, kind)| EdgeJson {
            from,
            to,
            kind: kind.to_string(),
        })
        .collect();

    let g = GraphJson { nodes, edges };
    serde_json::to_string_pretty(&g).unwrap_or_else(|_| "{\"nodes\":[],\"edges\":[]}".to_string())
}

#[derive(Serialize)]
struct GraphJson {
    nodes: Vec<NodeJson>,
    edges: Vec<EdgeJson>,
}

#[derive(Serialize)]
struct NodeJson {
    id: String,
    #[serde(rename = "type")]
    ty: String,
    status: String,
    priority: Option<String>,
}

#[derive(Serialize)]
struct EdgeJson {
    from: String,
    to: String,
    kind: String,
}

fn index_docs(docs: &[DocMeta]) -> HashMap<&str, &DocMeta> {
    docs.iter().map(|d| (d.id.as_str(), d)).collect()
}

fn escape_label(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// Keep `NodeRef` import used (silences dead-imports diagnostics in some
// configs; the type is referenced via scope_membership return).
#[allow(dead_code)]
fn _node_ref_marker(_n: &NodeRef) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::{DocType, Relation, RelationType, Status};
    use chrono::NaiveDate;
    use serde_json::Value;
    use std::path::PathBuf;

    fn doc(id: &str, ty: &str, blocks: &[&str], implements: &[&str]) -> DocMeta {
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
            title: format!("Title of {}", id),
            status: Status::Draft,
            author: "test".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            related,
            provenance: vec![],
            validate_ignore: false,
            path: PathBuf::from(format!("docs/{}.md", id)),
            virtual_doc: false,
            priority: Some("must".to_string()),
        }
    }

    fn two_node_one_edge() -> Vec<DocMeta> {
        vec![
            doc("A", "story", &["B"], &[]),
            doc("B", "story", &[], &[]),
        ]
    }

    #[test]
    fn render_d2_emits_header_node_decls_and_blocks_edge() {
        let docs = two_node_one_edge();
        let g = Graph::from_documents(&docs);

        let out = render_d2(&g, &Scope::All, &docs);

        assert!(out.starts_with("direction: down\n"), "got: {}", out);
        assert!(out.contains("A: { label: \"Title of A\" }"), "got: {}", out);
        assert!(out.contains("B: { label: \"Title of B\" }"), "got: {}", out);
        assert!(out.contains("A -> B\n"), "got: {}", out);
    }

    #[test]
    fn render_d2_implements_edge_uses_dashed_style() {
        let docs = vec![
            doc("I", "iteration", &[], &["S"]),
            doc("S", "story", &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let out = render_d2(&g, &Scope::All, &docs);

        assert!(
            out.contains("I -> S { style.stroke-dash: 4 }"),
            "got: {}",
            out
        );
    }

    #[test]
    fn render_dot_emits_digraph_with_quoted_edge() {
        let docs = two_node_one_edge();
        let g = Graph::from_documents(&docs);

        let out = render_dot(&g, &Scope::All, &docs);

        assert!(out.starts_with("digraph G {\n"), "got: {}", out);
        assert!(out.contains("\"A\" -> \"B\""), "got: {}", out);
        assert!(out.trim_end().ends_with('}'), "got: {}", out);
    }

    #[test]
    fn render_dot_implements_edge_uses_dashed_attr() {
        let docs = vec![
            doc("I", "iteration", &[], &["S"]),
            doc("S", "story", &[], &[]),
        ];
        let g = Graph::from_documents(&docs);

        let out = render_dot(&g, &Scope::All, &docs);

        assert!(
            out.contains("\"I\" -> \"S\" [style=dashed]"),
            "got: {}",
            out
        );
    }

    #[test]
    fn render_json_serializes_nodes_and_edges_per_schema() {
        let docs = two_node_one_edge();
        let g = Graph::from_documents(&docs);

        let out = render_json(&g, &Scope::All, &docs);
        let v: Value = serde_json::from_str(&out).expect("valid json");

        let nodes = v["nodes"].as_array().expect("nodes array");
        assert_eq!(nodes.len(), 2);
        let a = nodes.iter().find(|n| n["id"] == "A").expect("A present");
        assert_eq!(a["type"], "story");
        assert_eq!(a["status"], "draft");
        assert_eq!(a["priority"], "must");

        let edges = v["edges"].as_array().expect("edges array");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "A");
        assert_eq!(edges[0]["to"], "B");
        assert_eq!(edges[0]["kind"], "blocks");
    }

    #[test]
    fn render_json_distinguishes_blocks_and_implements_kinds() {
        let docs = vec![
            doc("A", "story", &["B"], &[]),
            doc("B", "story", &[], &[]),
            doc("I", "iteration", &[], &["A"]),
        ];
        let g = Graph::from_documents(&docs);

        let out = render_json(&g, &Scope::All, &docs);
        let v: Value = serde_json::from_str(&out).expect("valid json");

        let kinds: HashSet<String> = v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap().to_string())
            .collect();
        assert!(kinds.contains("blocks"));
        assert!(kinds.contains("implements"));
    }

    #[test]
    fn empty_graph_all_three_renderers_emit_safe_minimums() {
        let docs: Vec<DocMeta> = vec![];
        let g = Graph::from_documents(&docs);

        let d2 = render_d2(&g, &Scope::All, &docs);
        assert_eq!(d2, "direction: down\n");

        let dot = render_dot(&g, &Scope::All, &docs);
        assert_eq!(dot, "digraph G {}\n");

        let json = render_json(&g, &Scope::All, &docs);
        let v: Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(v["edges"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn scope_under_filters_out_of_scope_nodes_in_all_renderers() {
        // R is the rfc anchor; I implements R → in scope under R.
        // X is unrelated → out of scope.
        let docs = vec![
            doc("R", "rfc", &[], &[]),
            doc("I", "iteration", &[], &["R"]),
            doc("X", "story", &[], &[]),
        ];
        let g = Graph::from_documents(&docs);
        let scope = Scope::Under("R".to_string());

        let d2 = render_d2(&g, &scope, &docs);
        assert!(d2.contains("R: {"), "d2 should include R: {}", d2);
        assert!(d2.contains("I: {"), "d2 should include I: {}", d2);
        assert!(!d2.contains("X:"), "d2 should not include X: {}", d2);

        let dot = render_dot(&g, &scope, &docs);
        assert!(dot.contains("\"R\""), "dot should include R: {}", dot);
        assert!(dot.contains("\"I\""), "dot should include I: {}", dot);
        assert!(!dot.contains("\"X\""), "dot should not include X: {}", dot);

        let json = render_json(&g, &scope, &docs);
        let v: Value = serde_json::from_str(&json).expect("valid json");
        let ids: HashSet<String> = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_string())
            .collect();
        assert!(ids.contains("R"));
        assert!(ids.contains("I"));
        assert!(!ids.contains("X"));
    }

    #[test]
    fn renderers_are_deterministic_across_invocations() {
        let docs = vec![
            doc("Z", "story", &["A"], &[]),
            doc("A", "story", &[], &[]),
            doc("M", "iteration", &[], &["A"]),
        ];
        let g = Graph::from_documents(&docs);

        assert_eq!(
            render_d2(&g, &Scope::All, &docs),
            render_d2(&g, &Scope::All, &docs)
        );
        assert_eq!(
            render_dot(&g, &Scope::All, &docs),
            render_dot(&g, &Scope::All, &docs)
        );
        assert_eq!(
            render_json(&g, &Scope::All, &docs),
            render_json(&g, &Scope::All, &docs)
        );
    }
}
