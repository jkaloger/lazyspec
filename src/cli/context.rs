use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{bold, dim, styled_status};
use crate::engine::document::DocMeta;
use crate::engine::graph::MAX_REVERSE_EXPANSION_ROWS;
use crate::engine::status_colors::StatusColors;
use crate::engine::store::Store;
use anyhow::Result;
use console::colors_enabled;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use crate::engine::context::{
    merge_declared_related, resolve_chain, resolve_forest, ContextNode, RelatedRef, ResolvedContext,
};

pub fn run_json(store: &Store, id: &str, depth: usize) -> Result<String> {
    let mut resolved = resolve_chain(store, id, depth)?;
    merge_declared_related(store, &mut resolved);
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
///
/// An anchored forest also emits each anchor's chain ANCESTORS below it with the
/// edge inverted (STORY-247), so those nodes hang under a doc they do not
/// implement — it implements them. Their edges are therefore emitted under
/// `inverted_parents_in_context` instead, leaving `implements_in_context` to mean
/// only ever what its name says. The name is deliberately about the EDGE and not
/// about who implements whom: it holds a node's inverted parent edges, which is not
/// the same as "the docs that implement this one" — on `--anchor story` a story's
/// implementing iterations are FORWARD children, so they never appear in it and the
/// list is empty. `reverse_in_context` is the per-node marker AC2 asks for, and is
/// the engine's `ContextNode::parents_inverted`. Anchoring puts a doc on the
/// descendant side or the ancestor side, never both, so exactly one of the two lists
/// is non-empty per node and their union is the node's rendered parents. Both keys
/// are present on every node of an anchored forest and absent from the unanchored
/// one, whose output is byte-identical to before the reverse chain existed
/// (STORY-247 AC6).
pub fn run_forest_json(store: &Store, anchor: Option<&str>) -> Result<String> {
    let forest = resolve_forest(store, anchor);
    let anchored = anchor.is_some();
    let nodes: Vec<_> = forest
        .iter()
        .map(|n| {
            let mut value = doc_to_json_with_family(n.doc, store);
            let edges = |paths: &[PathBuf]| {
                serde_json::Value::Array(
                    paths
                        .iter()
                        .map(|p| serde_json::Value::String(p.to_string_lossy().to_string()))
                        .collect(),
                )
            };
            let (forward, inverted): (&[PathBuf], &[PathBuf]) = if n.parents_inverted {
                (&[], &n.parents)
            } else {
                (&n.parents, &[])
            };
            let obj = value.as_object_mut().unwrap();
            obj.insert("implements_in_context".to_string(), edges(forward));
            if anchored {
                obj.insert("inverted_parents_in_context".to_string(), edges(inverted));
                obj.insert(
                    "reverse_in_context".to_string(),
                    serde_json::Value::Bool(n.parents_inverted),
                );
            }
            value
        })
        .collect();
    let output = serde_json::json!({ "forest": nodes });
    Ok(serde_json::to_string_pretty(&output)?)
}

fn mini_card(colors: &StatusColors, doc: &DocMeta, marker: bool) -> String {
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
    let line2_styled = format!(
        "{} {} [{}]",
        shorthand,
        doc_type,
        styled_status(colors, doc.doc_type.as_str(), status)
    );
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
fn render_stack(
    resolved: &ResolvedContext,
    store: &Store,
    colors: &StatusColors,
    output: &mut String,
) {
    for (i, node) in resolved.nodes.iter().enumerate() {
        let doc = node.doc;
        if i > 0 {
            output.push_str(&chain_connector());
            output.push('\n');
        }
        output.push_str(&mini_card(colors, doc, doc.path == resolved.target.path));
        output.push('\n');
        push_card_children(store, colors, &doc.path, "", output);
    }
}

/// Marker for a row reached by an anchoring-INVERTED chain edge: the row is a
/// chain ancestor of the card above it, not a descendant (STORY-247). It replaces
/// the row's last indent unit, so a marked card stays aligned with its forward
/// siblings — the same trade the TUI makes swapping `└─▶ ` for `└─↑ `. `▲` is the
/// `story` type icon in both other renderers, so the glyph is `↑` here too.
const REVERSE_MARKER: &str = "\u{2191} ";

/// Mutable state threaded through the tree render: which docs have been drawn,
/// which are on the current DFS path (the cycle guard), and how many reverse
/// re-encounters have been reached (the reverse-recursion budget).
///
/// [`render_tree`] walks the forest itself rather than `flatten_forest`'s output, so
/// it enforces the engine's [`MAX_REVERSE_EXPANSION_ROWS`] against its own count —
/// and counts the same thing the engine does: a reverse re-encounter, whether it goes
/// on to be redrawn in full or degrades to the `(see above)` shorthand. Cards the
/// render would emit anyway (first encounters, forward diamonds) never count, so a
/// merely large store cannot reach the cap; only the 2^L upward-path blow-up L
/// stacked chain diamonds above an anchor produce can. Unlike the graph views, the
/// degraded row here is visibly a stub.
struct TreeWalk {
    drawn: HashSet<PathBuf>,
    on_stack: HashSet<PathBuf>,
    reverse_rows: usize,
}

/// Render the backward node set as an indented tree for a multi-parent DAG.
/// Roots (nodes with no in-graph parents) are drawn first, then their children
/// descend with increasing indentation. A node reached more than once (a
/// diamond) is drawn fully on first visit; a later FORWARD encounter emits a
/// one-line shorthand reference and does not recurse, since the subtree below it
/// was already drawn under the first parent. A later REVERSE encounter (an
/// anchored forest's inverted ancestor edge) IS redrawn in full and recurses, so
/// every anchor carries its own upward lineage instead of a truncated one —
/// matching `flatten_forest`'s split for the TUI and web trees.
fn render_tree(
    resolved: &ResolvedContext,
    store: &Store,
    colors: &StatusColors,
    output: &mut String,
) {
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

    let mut walk = TreeWalk {
        drawn: HashSet::new(),
        on_stack: HashSet::new(),
        reverse_rows: 0,
    };
    for root in roots {
        render_tree_node(
            root, 0, false, resolved, store, colors, &children, &by_path, &mut walk, output,
        );
    }

    // Cyclic input can leave a strongly-connected component with no root, so
    // the root traversal never reaches it. Draw any still-undrawn node as a
    // depth-0 subtree (in topological/path order) so the render is complete.
    // Like a root it carries no reverse marker: it was reached by no edge, and
    // anchoring can only re-root an inverted ancestor here when the anchor that
    // owns it sits in a rootless cycle.
    for node in &resolved.nodes {
        if !walk.drawn.contains(&node.doc.path) {
            render_tree_node(
                node, 0, false, resolved, store, colors, &children, &by_path, &mut walk, output,
            );
        }
    }
}

/// A tree row's leading whitespace, with the last indent unit replaced by
/// [`REVERSE_MARKER`] when the row was reached by an inverted edge. Depth and the
/// marker are keyed independently: a depth-0 row has no indent unit to mark and is
/// never reverse, since it was reached by no edge at all.
fn tree_indent(depth: usize, reverse: bool) -> String {
    if reverse && depth > 0 {
        format!("{}{}", "  ".repeat(depth - 1), REVERSE_MARKER)
    } else {
        "  ".repeat(depth)
    }
}

#[allow(clippy::too_many_arguments)]
fn render_tree_node(
    node: &ContextNode,
    depth: usize,
    reverse: bool,
    resolved: &ResolvedContext,
    store: &Store,
    colors: &StatusColors,
    children: &HashMap<&PathBuf, Vec<&PathBuf>>,
    by_path: &HashMap<&PathBuf, &ContextNode>,
    walk: &mut TreeWalk,
    output: &mut String,
) {
    let indent = "  ".repeat(depth);
    let marked_indent = tree_indent(depth, reverse);
    let doc = node.doc;

    if walk.drawn.contains(&doc.path) {
        // A reverse re-encounter is redrawn in full below, so this anchor gets the
        // ancestor's own lineage too — unless the edge closes a cycle (the node is
        // still on the DFS path, which is what makes the walk terminate) or the
        // budget is spent. Everything else is a forward diamond: shorthand
        // reference, no full card, no recurse.
        let re_encounter = reverse && !walk.on_stack.contains(&doc.path);
        if re_encounter {
            walk.reverse_rows += 1;
        }
        if !re_encounter || walk.reverse_rows >= MAX_REVERSE_EXPANSION_ROWS {
            output.push_str(&format!(
                "{}\u{21B3} {} (see above)\n",
                marked_indent,
                doc.id.to_uppercase()
            ));
            return;
        }
    }
    walk.drawn.insert(doc.path.clone());

    let card = mini_card(colors, doc, doc.path == resolved.target.path);
    for (i, line) in card.lines().enumerate() {
        // `mini_card`'s second line is its title line, where the marker reads as
        // an edge into the card the way the TUI's connector does.
        output.push_str(if i == 1 { &marked_indent } else { &indent });
        output.push_str(line);
        output.push('\n');
    }
    push_card_children(store, colors, &doc.path, &indent, output);

    walk.on_stack.insert(doc.path.clone());
    if let Some(kids) = children.get(&doc.path) {
        for child_path in kids {
            if let Some(child) = by_path.get(child_path) {
                render_tree_node(
                    child,
                    depth + 1,
                    child.parents_inverted,
                    resolved,
                    store,
                    colors,
                    children,
                    by_path,
                    walk,
                    output,
                );
            }
        }
    }
    walk.on_stack.remove(&doc.path);
}

/// Emit the forward (`implements`-pointing) children of `path` as `├─`/`└─`
/// lines, each prefixed by `indent`. Shared by the stack and tree renders.
fn push_card_children(
    store: &Store,
    colors: &StatusColors,
    path: &Path,
    indent: &str,
    output: &mut String,
) {
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
            styled_status(colors, child.doc_type.as_str(), &child.status)
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
    let colors = StatusColors::load(store.root()).unwrap_or_default();
    let mut output = String::new();
    render_tree(&resolved, store, &colors, &mut output);
    Ok(output)
}

pub fn run_human(store: &Store, id: &str, depth: usize) -> Result<String> {
    let mut resolved = resolve_chain(store, id, depth)?;
    merge_declared_related(store, &mut resolved);
    let colors = StatusColors::load(store.root()).unwrap_or_default();
    let mut output = String::new();

    let linear = resolved.nodes.iter().all(|n| n.parents.len() <= 1);
    if linear {
        render_stack(&resolved, store, &colors, &mut output);
    } else {
        render_tree(&resolved, store, &colors, &mut output);
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
                styled_status(&colors, f.doc.doc_type.as_str(), &f.doc.status)
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
                styled_status(&colors, rel.doc.doc_type.as_str(), &rel.doc.status)
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
