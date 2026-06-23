use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{bold, dim, styled_status};
use crate::engine::document::DocMeta;
use crate::engine::store::Store;
use anyhow::Result;
use console::colors_enabled;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use crate::engine::context::{
    resolve_chain, resolve_forest, ContextNode, RelatedRef, ResolvedContext,
};

pub fn run_json(store: &Store, id: &str, depth: usize) -> Result<String> {
    let resolved = resolve_chain(store, id, depth)?;
    let chain: Vec<_> = resolved
        .nodes
        .iter()
        .map(|n| {
            let mut value = doc_to_json_with_family(n.doc, store);
            let edges: Vec<_> = n
                .parents
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().to_string()))
                .collect();
            value.as_object_mut().unwrap().insert(
                "implements_in_context".to_string(),
                serde_json::Value::Array(edges),
            );
            value
        })
        .collect();
    let tag = |r: &RelatedRef| {
        let mut value = doc_to_json_with_family(r.doc, store);
        let obj = value.as_object_mut().unwrap();
        obj.insert(
            "relation".to_string(),
            serde_json::Value::String(r.relation.to_string()),
        );
        obj.insert("distance".to_string(), serde_json::json!(r.distance));
        obj.insert(
            "via".to_string(),
            serde_json::Value::String(r.via.to_string_lossy().to_string()),
        );
        value
    };
    let forward: Vec<_> = resolved.forward.iter().map(tag).collect();
    let related: Vec<_> = resolved.related.iter().map(tag).collect();
    let output = serde_json::json!({
        "chain": chain,
        "forward": forward,
        "related": related,
        "target": resolved.target.path.to_string_lossy(),
    });
    Ok(serde_json::to_string_pretty(&output)?)
}

/// Emit the context forest as JSON. `anchor` re-roots on a document type; `None`
/// yields the whole-store forest. Each node carries its in-context parent edges
/// under `implements_in_context`, matching the chain JSON shape.
pub fn run_forest_json(store: &Store, anchor: Option<&str>) -> Result<String> {
    let forest = resolve_forest(store, anchor);
    let nodes: Vec<_> = forest
        .iter()
        .map(|n| {
            let mut value = doc_to_json_with_family(n.doc, store);
            let edges: Vec<_> = n
                .parents
                .iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().to_string()))
                .collect();
            value.as_object_mut().unwrap().insert(
                "implements_in_context".to_string(),
                serde_json::Value::Array(edges),
            );
            value
        })
        .collect();
    let output = serde_json::json!({ "forest": nodes });
    Ok(serde_json::to_string_pretty(&output)?)
}

fn mini_card(doc: &DocMeta, marker: bool) -> String {
    let title = &doc.title;
    let doc_type = format!("{}", doc.doc_type).to_lowercase();
    let shorthand = doc.id.to_uppercase();
    let status = &doc.status;
    let status_str = format!("{}", status);
    let line2_plain = format!("{} {} [{}]", shorthand, doc_type, status_str);
    let content_width = title.len().max(line2_plain.len()) + 2;
    let marker_suffix = if marker {
        "  \u{2190} you are here"
    } else {
        ""
    };

    if !colors_enabled() {
        let border = "-".repeat(content_width);
        return format!(
            "+{}+\n| {:<width$}|{}\n| {:<width$}|\n+{}+",
            border,
            format!("{} ", title),
            marker_suffix,
            format!("{} ", line2_plain),
            border,
            width = content_width - 1,
        );
    }

    let styled_marker = if marker {
        format!("  {}", dim("\u{2190} you are here"))
    } else {
        String::new()
    };
    let top = format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(content_width));
    let pad1 = " ".repeat(content_width - 1 - title.len());
    let mid1 = format!("\u{2502} {}{}\u{2502}{}", bold(title), pad1, styled_marker);
    let pad2 = " ".repeat(content_width - 1 - line2_plain.len());
    let line2_styled = format!("{} {} [{}]", shorthand, doc_type, styled_status(status));
    let mid2 = format!("\u{2502} {}{}\u{2502}", line2_styled, pad2);
    let bot = format!("\u{2570}{}\u{256f}", "\u{2500}".repeat(content_width));
    format!("{}\n{}\n{}\n{}", top, mid1, mid2, bot)
}

fn chain_connector() -> String {
    if colors_enabled() {
        format!("  {}", dim("\u{2502}"))
    } else {
        "  \u{2193}".to_string()
    }
}

/// Render the backward node set as the vertical stack of mini-cards used for a
/// single-parent (linear) chain. Each card is separated by `chain_connector()`
/// and each card's forward children are listed below it.
fn render_stack(resolved: &ResolvedContext, store: &Store, output: &mut String) {
    for (i, node) in resolved.nodes.iter().enumerate() {
        let doc = node.doc;
        if i > 0 {
            output.push_str(&chain_connector());
            output.push('\n');
        }
        output.push_str(&mini_card(doc, doc.path == resolved.target.path));
        output.push('\n');
        push_card_children(store, &doc.path, "", output);
    }
}

/// Render the backward node set as an indented tree for a multi-parent DAG.
/// Roots (nodes with no in-graph parents) are drawn first, then their children
/// descend with increasing indentation. A node reached more than once (a
/// diamond) is drawn fully on first visit; later encounters emit a one-line
/// shorthand reference and do not recurse, so each node is drawn exactly once.
fn render_tree(resolved: &ResolvedContext, store: &Store, output: &mut String) {
    // child adjacency: parent path -> child paths (a child is a node whose
    // `parents` contains the parent's path).
    let mut children: HashMap<&PathBuf, Vec<&PathBuf>> = HashMap::new();
    for node in &resolved.nodes {
        for parent in &node.parents {
            children.entry(parent).or_default().push(&node.doc.path);
        }
    }
    for kids in children.values_mut() {
        kids.sort();
    }

    let mut roots: Vec<&ContextNode> = resolved
        .nodes
        .iter()
        .filter(|n| n.parents.is_empty())
        .collect();
    roots.sort_by(|a, b| a.doc.path.cmp(&b.doc.path));

    let by_path: HashMap<&PathBuf, &ContextNode> =
        resolved.nodes.iter().map(|n| (&n.doc.path, n)).collect();

    let mut drawn: HashSet<PathBuf> = HashSet::new();
    for root in roots {
        render_tree_node(
            root, 0, resolved, store, &children, &by_path, &mut drawn, output,
        );
    }

    // Cyclic input can leave a strongly-connected component with no root, so
    // the root traversal never reaches it. Draw any still-undrawn node as a
    // depth-0 subtree (in topological/path order) so the render is complete;
    // the drawn-set still guarantees each node is drawn exactly once.
    for node in &resolved.nodes {
        if !drawn.contains(&node.doc.path) {
            render_tree_node(
                node, 0, resolved, store, &children, &by_path, &mut drawn, output,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_tree_node(
    node: &ContextNode,
    depth: usize,
    resolved: &ResolvedContext,
    store: &Store,
    children: &HashMap<&PathBuf, Vec<&PathBuf>>,
    by_path: &HashMap<&PathBuf, &ContextNode>,
    drawn: &mut HashSet<PathBuf>,
    output: &mut String,
) {
    let indent = "  ".repeat(depth);
    let doc = node.doc;

    if drawn.contains(&doc.path) {
        // Diamond re-encounter: shorthand reference, no full card, no recurse.
        output.push_str(&format!(
            "{}\u{21B3} {} (see above)\n",
            indent,
            doc.id.to_uppercase()
        ));
        return;
    }
    drawn.insert(doc.path.clone());

    let card = mini_card(doc, doc.path == resolved.target.path);
    for line in card.lines() {
        output.push_str(&indent);
        output.push_str(line);
        output.push('\n');
    }
    push_card_children(store, &doc.path, &indent, output);

    if let Some(kids) = children.get(&doc.path) {
        for child_path in kids {
            if let Some(child) = by_path.get(child_path) {
                render_tree_node(
                    child,
                    depth + 1,
                    resolved,
                    store,
                    children,
                    by_path,
                    drawn,
                    output,
                );
            }
        }
    }
}

/// Emit the forward (`implements`-pointing) children of `path` as `├─`/`└─`
/// lines, each prefixed by `indent`. Shared by the stack and tree renders.
fn push_card_children(store: &Store, path: &Path, indent: &str, output: &mut String) {
    let child_paths = store.children_of(path);
    if child_paths.is_empty() {
        return;
    }
    let children: Vec<_> = child_paths.iter().filter_map(|cp| store.get(cp)).collect();
    for (j, child) in children.iter().enumerate() {
        let connector = if j == children.len() - 1 {
            "\u{2514}\u{2500}"
        } else {
            "\u{251c}\u{2500}"
        };
        let shorthand = child.id.to_uppercase();
        let title = &child.title;
        let status_display = if colors_enabled() {
            styled_status(&child.status)
        } else {
            format!("{}", child.status)
        };
        output.push_str(&format!(
            "{}  {} {} {} [{}]\n",
            indent, connector, shorthand, title, status_display
        ));
    }
}

/// Render the context forest as an indented tree, anchor/root-first. Reuses the
/// same tree renderer as the chain view (no "you are here" marker since the
/// forest has no single target).
pub fn run_forest_human(store: &Store, anchor: Option<&str>) -> Result<String> {
    let forest = resolve_forest(store, anchor);
    if forest.is_empty() {
        return Ok(String::new());
    }
    // The forest has no single "you are here" target. `render_tree` stamps the
    // marker on the node whose path equals `target.path`, so point `target` at a
    // sentinel doc with an empty path that matches no node, suppressing the
    // marker entirely.
    let sentinel = DocMeta {
        path: PathBuf::new(),
        ..forest[0].doc.clone()
    };
    let resolved = ResolvedContext {
        target: &sentinel,
        nodes: forest,
        forward: Vec::new(),
        related: Vec::new(),
    };
    let mut output = String::new();
    render_tree(&resolved, store, &mut output);
    Ok(output)
}

pub fn run_human(store: &Store, id: &str, depth: usize) -> Result<String> {
    let resolved = resolve_chain(store, id, depth)?;
    let mut output = String::new();

    let linear = resolved.nodes.iter().all(|n| n.parents.len() <= 1);
    if linear {
        render_stack(&resolved, store, &mut output);
    } else {
        render_tree(&resolved, store, &mut output);
    }

    if !resolved.forward.is_empty() {
        output.push_str(&chain_connector());
        output.push('\n');
        for (j, f) in resolved.forward.iter().enumerate() {
            let connector = if j == resolved.forward.len() - 1 {
                "\u{2514}\u{2500}"
            } else {
                "\u{251c}\u{2500}"
            };
            let shorthand = f.doc.id.to_uppercase();
            let title = &f.doc.title;
            let status_display = if colors_enabled() {
                styled_status(&f.doc.status)
            } else {
                format!("{}", f.doc.status)
            };
            output.push_str(&format!(
                "  {} {} {} [{}]\n",
                connector, shorthand, title, status_display
            ));
        }
    }

    if !resolved.related.is_empty() {
        output.push('\n');
        if colors_enabled() {
            output.push_str(&format!(
                "{}\n",
                dim("\u{2500}\u{2500}\u{2500} related \u{2500}\u{2500}\u{2500}")
            ));
        } else {
            output.push_str("--- related ---\n");
        }
        for rel in &resolved.related {
            let shorthand = rel.doc.id.to_uppercase();
            let status_display = if colors_enabled() {
                styled_status(&rel.doc.status)
            } else {
                format!("{}", rel.doc.status)
            };
            let suffix = if rel.distance > 1 {
                let via = store
                    .get(&rel.via)
                    .map(|d| d.id.to_uppercase())
                    .unwrap_or_else(|| rel.via.to_string_lossy().to_string());
                format!(" (via {}, d{})", via, rel.distance)
            } else {
                String::new()
            };
            output.push_str(&format!(
                "  {}  {} [{}]{}\n",
                shorthand, rel.doc.title, status_display, suffix
            ));
        }
    }

    Ok(output)
}
