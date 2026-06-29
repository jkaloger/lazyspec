//! HTTP route handlers for the read-only web view.
//!
//! Imports only from [`crate::engine`] (and `web::render`), never `cli`/`tui`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::engine::document::Status;
use crate::engine::fs::RealFileSystem;
use crate::engine::store::{Filter, Store};
use crate::web::render::{
    markdown_to_html, DocGroup, DocPage, DocRow, FilterOption, ListFragment, ListPage,
    NotFoundPage, SearchFragment,
};

/// Default lines per expanded `@ref` block, mirroring `show --expand-references`.
const MAX_REF_LINES: usize = 25;

/// Query parameters for the htmx filter fragment. Empty strings mean "no
/// filter" (the `all` option), so they are treated as absent.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
}

fn empty_to_none(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

/// Query parameters for `GET /search`. An absent or empty `q` is valid and
/// falls back to the full unfiltered list.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: Option<String>,
}

/// Build the type-grouped rows for the documents matching `filter`, sorted by
/// type name then by id for stable output.
fn build_groups(store: &Store, filter: &Filter) -> Vec<DocGroup> {
    let mut by_type: BTreeMap<String, Vec<DocRow>> = BTreeMap::new();
    for doc in store.list(filter) {
        by_type
            .entry(doc.doc_type.to_string())
            .or_default()
            .push(DocRow {
                id: doc.id.clone(),
                title: doc.title.clone(),
                status: doc.status.to_string(),
            });
    }

    by_type
        .into_iter()
        .map(|(doc_type, mut docs)| {
            docs.sort_by(|a, b| a.id.cmp(&b.id));
            DocGroup { doc_type, docs }
        })
        .collect()
}

/// Collect the distinct statuses and tags present across all documents, sorted,
/// to populate the filter controls.
fn filter_options(store: &Store) -> (Vec<FilterOption>, Vec<FilterOption>) {
    let mut statuses = std::collections::BTreeSet::new();
    let mut tags = std::collections::BTreeSet::new();
    for doc in store.all_docs() {
        statuses.insert(doc.status.to_string());
        for tag in &doc.tags {
            tags.insert(tag.clone());
        }
    }
    let to_opts = |set: std::collections::BTreeSet<String>| {
        set.into_iter()
            .map(|value| FilterOption { value })
            .collect()
    };
    (to_opts(statuses), to_opts(tags))
}

/// `GET /` -- the full document-list page with filter controls and the initial
/// (unfiltered) list grouped by type.
pub async fn list_page(State(store): State<Arc<Store>>) -> Html<String> {
    let groups = build_groups(&store, &Filter::default());
    let list = ListFragment { groups }.render_string();
    let (statuses, tags) = filter_options(&store);
    let page = ListPage {
        statuses,
        tags,
        list,
    };
    Html(page.render().unwrap_or_default())
}

/// `GET /fragment/list?status=&tag=` -- the htmx filter handler. Reuses
/// [`Store::list`] for the matching subset and returns only the list fragment.
pub async fn list_fragment(
    State(store): State<Arc<Store>>,
    Query(query): Query<ListQuery>,
) -> Html<String> {
    let filter = Filter {
        doc_type: None,
        status: empty_to_none(query.status).map(|s| Status::new(&s)),
        tag: empty_to_none(query.tag),
    };
    let groups = build_groups(&store, &filter);
    let fragment = ListFragment { groups };
    Html(fragment.render().unwrap_or_default())
}

/// `GET /search?q=` -- thin adapter over [`Store::search`]. An empty/absent `q`
/// renders the full unfiltered list fragment (identical to the `GET /` list
/// region). Otherwise the engine search runs and its results are rendered in
/// the engine's returned order; zero results yield the empty-result state.
pub async fn search(
    State(store): State<Arc<Store>>,
    Query(query): Query<SearchQuery>,
) -> Html<String> {
    let Some(q) = empty_to_none(query.q) else {
        let groups = build_groups(&store, &Filter::default());
        return Html(ListFragment { groups }.render().unwrap_or_default());
    };

    let rows = store
        .search(&q, &RealFileSystem)
        .into_iter()
        .map(|r| DocRow {
            id: r.doc.id.clone(),
            title: r.doc.title.clone(),
            status: r.doc.status.to_string(),
        })
        .collect();

    Html(SearchFragment { rows }.render().unwrap_or_default())
}

/// `GET /doc/{id}` -- the per-document page: frontmatter header, body rendered
/// to HTML with `@ref` directives expanded inline. Unknown ids yield a handled
/// 404 with the not-found page, never a 500.
pub async fn doc_page(State(store): State<Arc<Store>>, AxumPath(id): AxumPath<String>) -> Response {
    // Resolve id -> document. Try a literal path first, then shorthand,
    // mirroring the engine resolution that backs `show` without importing cli.
    let doc = store
        .get(Path::new(&id))
        .or_else(|| store.resolve_shorthand(&id).ok());

    let Some(doc) = doc else {
        let page = NotFoundPage { id }.render().unwrap_or_default();
        return (StatusCode::NOT_FOUND, Html(page)).into_response();
    };

    let expanded = store
        .get_body_expanded(&doc.path, MAX_REF_LINES, &RealFileSystem)
        .unwrap_or_default();
    let body_html = markdown_to_html(&expanded);

    let page = DocPage::from_doc(doc, &store, body_html);
    Html(page.render().unwrap_or_default()).into_response()
}
