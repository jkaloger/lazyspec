//! Askama templates for the document-list page and its htmx-swappable fragment.
//!
//! Template bodies live under `templates/` (askama's default lookup dir):
//! `list_page.html` and `list_fragment.html`.

use askama::Template;

use crate::engine::context::ResolvedContext;
use crate::engine::document::DocMeta;
use crate::engine::graph::GraphNode;
use crate::engine::store::{extract_id_from_name, Store};

/// Size of the categorical tag palette; `tag_hue` returns a value in `0..TAG_HUES`.
pub const TAG_HUES: u8 = 8;

/// A tag with its categorical hue index, for the colored tag chips.
pub struct TagChip {
    pub name: String,
    pub hue: u8,
}

/// One row in the document list: id, title, status, tags.
pub struct DocRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub tags: Vec<TagChip>,
}

/// A web-only categorical (wayfinding) hue for a tag label, in `0..TAG_HUES`.
/// Presentation-only: an FNV-1a byte hash mod the palette size, total over any
/// string. Collisions are acceptable since the label carries identity.
pub fn tag_hue(tag: &str) -> u8 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in tag.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash % TAG_HUES as u32) as u8
}

/// A group of rows sharing a `doc_type`, rendered under a type heading.
pub struct DocGroup {
    pub doc_type: String,
    pub docs: Vec<DocRow>,
}

/// A status or tag filter option offered in the list controls.
pub struct FilterOption {
    pub value: String,
}

/// One entry in the sidebar's Filter section: a type or tag, with a view-aware
/// href and active state.
pub struct SidebarEntry {
    pub label: String,
    pub href: String,
    pub active: bool,
    /// The collapsed-rail badge: the type's configured icon for type entries,
    /// the uppercased first character otherwise.
    pub glyph: String,
    /// Categorical hue (`0..TAG_HUES`) for tag entries; unused for types.
    pub hue: u8,
}

/// The unified left sidebar: a View section (List/Graph, active driven by
/// `view`), a type filter section (`types`), and a tag filter section (`tags`).
/// `view` is `list` or `graph`.
pub struct Sidebar {
    pub view: String,
    pub types: Vec<SidebarEntry>,
    pub tags: Vec<SidebarEntry>,
}

/// The full document-list page: filter controls plus the (server-rendered)
/// initial list fragment. htmx swaps `#doc-list` in place on filter changes.
#[derive(Template)]
#[template(path = "list_page.html")]
pub struct ListPage {
    pub statuses: Vec<FilterOption>,
    pub tags: Vec<FilterOption>,
    pub list: String,
    pub sidebar: Sidebar,
    pub repo_name: String,
    pub branch: Option<String>,
}

/// The swappable list fragment: documents grouped by type. Returned both inside
/// the full page and on its own for htmx filter requests.
#[derive(Template)]
#[template(path = "list_fragment.html")]
pub struct ListFragment {
    pub groups: Vec<DocGroup>,
}

impl ListFragment {
    /// Render the fragment to an HTML string, for embedding in the full page.
    pub fn render_string(&self) -> String {
        self.render().unwrap_or_default()
    }
}

/// The search-results fragment: a flat list of rows in the engine's returned
/// order (never re-sorted in the web layer), reusing the shared list-row
/// partial. An empty `rows` renders the no-results state.
#[derive(Template)]
#[template(path = "search_fragment.html")]
pub struct SearchFragment {
    pub rows: Vec<DocRow>,
}

/// A typed relationship shown in the frontmatter header (`implements`, etc.).
pub struct RelationLink {
    pub rel_type: String,
    pub target: String,
}

/// A parent/child link in the frontmatter header.
pub struct RelativeLink {
    pub id: String,
    pub title: String,
}

/// One document in the anchored context (ancestor, descendant, or related peer),
/// carrying the identity needed to render a row that links to its `/doc/{id}` page.
pub struct ContextEntry {
    pub id: String,
    pub doc_type: String,
    pub status: String,
    pub title: String,
}

/// The document-page view model: the structured frontmatter header fields plus
/// the body already rendered to HTML. Built from a [`DocMeta`] and its
/// parent/children in the [`Store`].
#[derive(Template)]
#[template(path = "doc_page.html")]
pub struct DocPage {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub status: String,
    pub author: String,
    pub date: String,
    pub tags: Vec<TagChip>,
    pub relations: Vec<RelationLink>,
    pub parent: Option<RelativeLink>,
    pub children: Vec<RelativeLink>,
    /// The anchored context, grouped by direction. Chain ancestors (the doc's
    /// own chain above it, target excluded), chain descendants (docs that
    /// implement/target this doc), and related-to peers. Empty groups render
    /// nothing; all-empty omits the Context section entirely.
    pub ancestors: Vec<ContextEntry>,
    pub descendants: Vec<ContextEntry>,
    pub related: Vec<ContextEntry>,
    /// The body, ref-expanded then rendered to HTML.
    pub body_html: String,
    /// The outbound "edit on GitHub" deep-link, or `None` when no link could be
    /// resolved (unresolvable coords, or a backend with no stable URL).
    pub github_url: Option<String>,
    /// Shell fields, set by the route after `from_doc`.
    pub sidebar: Sidebar,
    pub repo_name: String,
    pub branch: Option<String>,
}

impl DocPage {
    /// Build the frontmatter view model from a document and its store context.
    /// `body_html` is the already-expanded, already-rendered HTML body;
    /// `github_url` is the resolved outbound deep-link, or `None`. `context` is
    /// the resolved anchored context (`None` when resolution failed; the page
    /// then renders with empty context groups rather than erroring).
    pub fn from_doc(
        doc: &DocMeta,
        store: &Store,
        body_html: String,
        github_url: Option<String>,
        context: Option<&ResolvedContext>,
    ) -> Self {
        let relations = doc
            .related
            .iter()
            .map(|r| RelationLink {
                rel_type: r.rel_type.to_string(),
                target: r.target.clone(),
            })
            .collect();

        let parent = store
            .parent_of(&doc.path)
            .and_then(|p| store.get(p))
            .map(|p| RelativeLink {
                id: p.id.clone(),
                title: p.title.clone(),
            });

        let children = store
            .children_of(&doc.path)
            .iter()
            .filter_map(|cp| store.get(cp))
            .map(|c| RelativeLink {
                id: c.id.clone(),
                title: c.title.clone(),
            })
            .collect();

        let entry = |d: &DocMeta| ContextEntry {
            id: d.id.clone(),
            doc_type: d.doc_type.to_string(),
            status: d.status.to_string(),
            title: d.title.clone(),
        };
        let (ancestors, descendants, related) = match context {
            Some(ctx) => (
                ctx.nodes
                    .iter()
                    .filter(|n| n.doc.path != ctx.target.path)
                    .map(|n| entry(n.doc))
                    .collect(),
                ctx.forward.iter().map(|r| entry(r.doc)).collect(),
                ctx.related.iter().map(|r| entry(r.doc)).collect(),
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };

        DocPage {
            id: doc.id.clone(),
            title: doc.title.clone(),
            doc_type: doc.doc_type.to_string(),
            status: doc.status.to_string(),
            author: doc.author.clone(),
            date: doc.date.to_string(),
            tags: doc
                .tags
                .iter()
                .map(|t| TagChip {
                    name: t.clone(),
                    hue: tag_hue(t),
                })
                .collect(),
            relations,
            parent,
            children,
            ancestors,
            descendants,
            related,
            body_html,
            github_url,
            sidebar: Sidebar {
                view: String::new(),
                types: Vec::new(),
                tags: Vec::new(),
            },
            repo_name: String::new(),
            branch: None,
        }
    }
}

/// The not-found page for an unresolved document id.
#[derive(Template)]
#[template(path = "not_found.html")]
pub struct NotFoundPage {
    pub id: String,
}

/// One node in the rendered `/graph` tree: the doc identity plus its nested
/// children. Built from the engine's flat `Vec<GraphNode>` via [`GraphTreeNode::nest`].
/// A diamond re-emission is a child row with no children of its own (its subtree
/// was drawn under the first parent), so the depth-based nesting drops it as a
/// leaf — matching the TUI's "plain doc row, no subtree" rule.
#[derive(Template)]
#[template(path = "graph_node.html")]
pub struct GraphTreeNode {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub status: String,
    pub depth: usize,
    pub related: Vec<String>,
    pub children: Vec<GraphTreeNode>,
}

impl GraphTreeNode {
    /// The doc id for a flattened node, derived from its path file stem (the same
    /// derivation the engine's graph tests use).
    fn id_of(node: &GraphNode) -> String {
        node.path
            .file_stem()
            .map(|s| extract_id_from_name(&s.to_string_lossy()))
            .unwrap_or_default()
    }

    /// Rebuild the nested tree from the engine's depth-tagged flat order. The flat
    /// list is a pre-order DFS where `depth` increases by one when descending into
    /// a child and drops back when a subtree ends; this folds it back into nested
    /// `children` without re-running any ordering (the engine already fixed the
    /// order, diamond/cycle handling included).
    pub fn nest(flat: &[GraphNode]) -> Vec<GraphTreeNode> {
        let mut roots: Vec<GraphTreeNode> = Vec::new();
        // Stack of indices into the path from a root down to the last-pushed node,
        // by depth. `stack[d]` is the most recent node at depth `d` on the current
        // branch, addressed by the chain of child indices to reach it.
        let mut path: Vec<usize> = Vec::new();

        for node in flat {
            let view = GraphTreeNode {
                id: Self::id_of(node),
                title: node.title.clone(),
                doc_type: node.doc_type.to_string(),
                status: node.status.to_string(),
                depth: node.depth,
                related: node.related.clone(),
                children: Vec::new(),
            };

            path.truncate(node.depth);

            if node.depth == 0 {
                roots.push(view);
                path.push(roots.len() - 1);
            } else {
                let mut cur = &mut roots[path[0]];
                for &idx in &path[1..node.depth] {
                    cur = &mut cur.children[idx];
                }
                cur.children.push(view);
                path.push(cur.children.len() - 1);
            }
        }

        roots
    }
}

/// The `/graph` page: the relationship forest rendered as a topologically-sorted
/// nested `<ul>` tree, ordered by `GraphSort::default()` (RFC-052 / STORY-179).
/// The sidebar's Filter section re-roots the forest (parity with the TUI pivot
/// panel).
#[derive(Template)]
#[template(path = "graph_page.html")]
pub struct GraphPage {
    pub roots: Vec<GraphTreeNode>,
    pub sidebar: Sidebar,
    pub repo_name: String,
    pub branch: Option<String>,
}

/// Render a markdown `body` to an HTML string via `pulldown-cmark`. Callers must
/// expand `@ref` directives first; this is the HTML path only.
pub fn markdown_to_html(body: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(body, options);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_hue_is_deterministic() {
        assert_eq!(tag_hue("web"), tag_hue("web"));
        assert_eq!(tag_hue("render"), tag_hue("render"));
    }

    #[test]
    fn tag_hue_is_total_and_in_range() {
        for tag in ["", "web", "render", "日本語", "café"] {
            assert!(tag_hue(tag) < TAG_HUES, "{tag} out of range");
        }
    }

    #[test]
    fn tag_hue_spreads_across_palette() {
        let hues: std::collections::BTreeSet<u8> =
            ["web", "render", "engine", "tui", "cli", "docs"]
                .iter()
                .map(|t| tag_hue(t))
                .collect();
        assert!(hues.len() >= 2, "hues collapsed to one value: {hues:?}");
    }
}
