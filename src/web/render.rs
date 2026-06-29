//! Askama templates for the document-list page and its htmx-swappable fragment.
//!
//! Template bodies live under `templates/` (askama's default lookup dir):
//! `list_page.html` and `list_fragment.html`.

use askama::Template;

use crate::engine::document::DocMeta;
use crate::engine::store::Store;

/// One row in the document list: id, title, status.
pub struct DocRow {
    pub id: String,
    pub title: String,
    pub status: String,
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

/// The full document-list page: filter controls plus the (server-rendered)
/// initial list fragment. htmx swaps `#doc-list` in place on filter changes.
#[derive(Template)]
#[template(path = "list_page.html")]
pub struct ListPage {
    pub statuses: Vec<FilterOption>,
    pub tags: Vec<FilterOption>,
    pub list: String,
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
    pub tags: Vec<String>,
    pub relations: Vec<RelationLink>,
    pub parent: Option<RelativeLink>,
    pub children: Vec<RelativeLink>,
    /// The body, ref-expanded then rendered to HTML.
    pub body_html: String,
}

impl DocPage {
    /// Build the frontmatter view model from a document and its store context.
    /// `body_html` is the already-expanded, already-rendered HTML body.
    pub fn from_doc(doc: &DocMeta, store: &Store, body_html: String) -> Self {
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

        DocPage {
            id: doc.id.clone(),
            title: doc.title.clone(),
            doc_type: doc.doc_type.to_string(),
            status: doc.status.to_string(),
            author: doc.author.clone(),
            date: doc.date.to_string(),
            tags: doc.tags.clone(),
            relations,
            parent,
            children,
            body_html,
        }
    }
}

/// The not-found page for an unresolved document id.
#[derive(Template)]
#[template(path = "not_found.html")]
pub struct NotFoundPage {
    pub id: String,
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
