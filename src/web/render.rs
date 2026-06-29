//! Askama templates for the document-list page and its htmx-swappable fragment.
//!
//! Template bodies live under `templates/` (askama's default lookup dir):
//! `list_page.html` and `list_fragment.html`.

use askama::Template;

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
