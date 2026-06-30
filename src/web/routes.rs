//! HTTP route handlers for the read-only web view.
//!
//! Imports only from [`crate::engine`] (and `web::render`), never `cli`/`tui`.

use std::collections::BTreeMap;
use std::path::Path;

use askama::Template;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::engine::context::{resolve_forest, resolve_forest_by_tag};
use crate::engine::document::{DocType, Status};
use crate::engine::fs::RealFileSystem;
use crate::engine::github_url::github_url;
use crate::engine::graph::{flatten_forest, GraphSort};
use crate::engine::store::{Filter, Store};
use crate::web::render::{
    markdown_to_html, DocGroup, DocPage, DocRow, FilterOption, GraphPage, GraphTreeNode,
    ListFragment, ListPage, NotFoundPage, PivotRow, SearchFragment,
};
use crate::web::server::AppState;

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
    #[serde(default)]
    pub r#type: Option<String>,
}

fn empty_to_none(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

/// Query parameters for `GET /graph`. `pivot` selects the forest re-root:
/// absent/empty = the whole-store forest (All), `type:{t}` re-roots on a
/// doc-type, `tag:{t}` re-roots on a tag. Mirrors the TUI `GraphAnchor`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct GraphQuery {
    #[serde(default)]
    pub pivot: Option<String>,
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

/// Collect the distinct doc-type names present across all documents, sorted, for
/// the sidebar type links.
fn doc_types(store: &Store) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for doc in store.all_docs() {
        set.insert(doc.doc_type.to_string());
    }
    set.into_iter().collect()
}

/// Collect the distinct tags present across all documents, sorted, for the
/// graph pivot picker's tag rows.
fn doc_tags(store: &Store) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for doc in store.all_docs() {
        for tag in &doc.tags {
            set.insert(tag.clone());
        }
    }
    set.into_iter().collect()
}

/// Build the [`Filter`] from list query params, treating empty strings as absent.
fn filter_from_query(query: ListQuery) -> Filter {
    Filter {
        doc_type: empty_to_none(query.r#type).map(|t| DocType::new(&t)),
        status: empty_to_none(query.status).map(|s| Status::new(&s)),
        tag: empty_to_none(query.tag),
    }
}

/// `GET /?status=&tag=&type=` -- the full document-list page with filter controls
/// and the (optionally filtered) list grouped by type.
pub async fn list_page(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Html<String> {
    let store = state.store;
    let filter = filter_from_query(query);
    let groups = build_groups(&store, &filter);
    let list = ListFragment { groups }.render_string();
    let (statuses, tags) = filter_options(&store);
    let page = ListPage {
        statuses,
        tags,
        list,
        types: doc_types(&store),
        repo_name: state.repo_name.clone(),
        branch: state.branch.clone(),
    };
    Html(page.render().unwrap_or_default())
}

/// `GET /fragment/list?status=&tag=` -- the htmx filter handler. Reuses
/// [`Store::list`] for the matching subset and returns only the list fragment.
pub async fn list_fragment(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Html<String> {
    let store = state.store;
    let filter = filter_from_query(query);
    let groups = build_groups(&store, &filter);
    let fragment = ListFragment { groups };
    Html(fragment.render().unwrap_or_default())
}

/// `GET /search?q=` -- thin adapter over [`Store::search`]. An empty/absent `q`
/// renders the full unfiltered list fragment (identical to the `GET /` list
/// region). Otherwise the engine search runs and its results are rendered in
/// the engine's returned order; zero results yield the empty-result state.
pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Html<String> {
    let store = state.store;
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

/// `GET /graph` -- the relationship forest as a topologically-sorted nested
/// `<ul>` tree, ordered by [`GraphSort::default`] (the static web view has no
/// interactive sort control). Reuses the engine's `resolve_forest` +
/// `flatten_forest` ordering, so diamonds (shared node re-emitted without its
/// subtree) and cycles (back-edge dropped, every node once) match the TUI.
pub async fn graph(State(state): State<AppState>, Query(query): Query<GraphQuery>) -> Html<String> {
    let store = state.store;
    let pivot = empty_to_none(query.pivot);

    // Re-root the forest per the pivot prefix, reusing the engine's anchor
    // logic. `type:`/`tag:` select a re-rooted forest; anything else (or absent)
    // is the whole-store All view.
    let forest = match pivot.as_deref() {
        Some(p) if p.starts_with("type:") => resolve_forest(&store, Some(&p["type:".len()..])),
        Some(p) if p.starts_with("tag:") => resolve_forest_by_tag(&store, &p["tag:".len()..]),
        _ => resolve_forest(&store, None),
    };
    let flat = flatten_forest(&forest, &store, &GraphSort::default());
    let roots = GraphTreeNode::nest(&flat);

    // Pivot rows in the TUI's flat order: All, then each doc-type, then each tag.
    // The row whose href-pivot matches the current selection is marked active.
    let active = pivot.as_deref();
    let mut pivots = vec![PivotRow {
        label: "All".to_string(),
        href: "/graph".to_string(),
        active: active.is_none(),
        kind: "all".to_string(),
    }];
    for t in doc_types(&store) {
        let value = format!("type:{t}");
        pivots.push(PivotRow {
            active: active == Some(value.as_str()),
            href: format!("/graph?pivot={value}"),
            label: t,
            kind: "type".to_string(),
        });
    }
    for tag in doc_tags(&store) {
        let value = format!("tag:{tag}");
        pivots.push(PivotRow {
            active: active == Some(value.as_str()),
            href: format!("/graph?pivot={value}"),
            label: tag,
            kind: "tag".to_string(),
        });
    }

    Html(
        GraphPage {
            roots,
            pivots,
            types: doc_types(&store),
            repo_name: state.repo_name.clone(),
            branch: state.branch.clone(),
        }
        .render()
        .unwrap_or_default(),
    )
}

/// `GET /doc/{id}` -- the per-document page: frontmatter header, body rendered
/// to HTML with `@ref` directives expanded inline. Unknown ids yield a handled
/// 404 with the not-found page, never a 500.
pub async fn doc_page(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> Response {
    let store = state.store;
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

    // The outbound "edit on GitHub" link. `None` when coords are unresolved or
    // the backend has no stable single-doc URL -- the template then renders no
    // link rather than a broken one.
    let github_url = state.coords.as_ref().and_then(|coords| {
        github_url(doc, coords, &state.config, &state.issue_map, None, None).map(|u| u.0)
    });

    let mut page = DocPage::from_doc(doc, &store, body_html, github_url);
    page.types = doc_types(&store);
    page.repo_name = state.repo_name.clone();
    page.branch = state.branch.clone();
    Html(page.render().unwrap_or_default()).into_response()
}
