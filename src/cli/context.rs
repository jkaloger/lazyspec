use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{bold, dim, styled_status};
use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::{ResolveError, Store};
use anyhow::Result;
use console::colors_enabled;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

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

    // BFS upward over `implements` edges. The seen-set both dedups shared
    // ancestors (diamonds) and guards against cycles (re-entering a node).
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
            if rel.rel_type != RelationType::Implements {
                continue;
            }
            let Some(parent) = store.get(&PathBuf::from(&rel.target)) else {
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

    // Forward context: docs whose `implements` points at the target. Each is
    // one hop out, reached through the target it implements.
    let target_path = doc.path.clone();
    let forward: Vec<RelatedRef> = store
        .reverse_links
        .get(&target_path)
        .map(|links| {
            links
                .iter()
                .filter(|(rel_type, _)| *rel_type == RelationType::Implements)
                .filter_map(|(_, source_path)| store.get(source_path))
                .map(|d| RelatedRef {
                    doc: d,
                    relation: RelationType::Implements,
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
                    if *rel_type == RelationType::RelatedTo {
                        neighbours.push(target.clone());
                    }
                }
            }
            if let Some(rev) = store.reverse_links.get(from) {
                for (rel_type, source) in rev {
                    if *rel_type == RelationType::RelatedTo {
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
                        relation: RelationType::RelatedTo,
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

/// Deterministic topological ordering of the discovered DAG, root-first.
/// `node_parents` holds the `implements` edges (child -> parents). A node is
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
