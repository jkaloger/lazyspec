#![cfg(feature = "web")]
//! Integration tests for the read-only web view skeleton (STORY-176 /
//! ITERATION-239). The axum `Router` is driven directly via
//! `tower::ServiceExt::oneshot` so no real socket is bound and there is no
//! network flakiness.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use lazyspec::engine::issue_map::IssueMap;
use lazyspec::engine::store::Store;
use lazyspec::web::server::{router, AppState, SharedStore, DEFAULT_PORT};
use tower::ServiceExt;

use crate::common::TestFixture;

fn store(fixture: &TestFixture) -> Store {
    fixture.store()
}

/// Build app state with deep-links disabled (no coords) for the skeleton tests.
fn state(store: Store) -> AppState {
    AppState {
        store: SharedStore::new(store),
        config: Arc::new(lazyspec::engine::config::Config::default()),
        coords: None,
        issue_map: Arc::new(IssueMap::default()),
        repo_name: "testrepo".into(),
        branch: Some("main".into()),
    }
}

async fn get(store: Store, uri: &str) -> (StatusCode, String) {
    let app = router(state(store));
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn root_returns_200_html_with_seeded_ids_grouped_by_type() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    fixture.write_story("STORY-001-beta.md", "Beta story", "in-progress", None);

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<!DOCTYPE html>"), "expected full HTML page");
    // Seeded ids present.
    assert!(body.contains("RFC-001"), "missing RFC-001:\n{body}");
    assert!(body.contains("STORY-001"), "missing STORY-001:\n{body}");
    // Titles and statuses rendered.
    assert!(body.contains("Alpha RFC"));
    assert!(body.contains("Beta story"));
    assert!(body.contains("draft"));
    assert!(body.contains("in-progress"));
    // Grouped by type: a section per doc_type.
    assert!(
        body.contains("data-doc-type=\"rfc\""),
        "no rfc group:\n{body}"
    );
    assert!(
        body.contains("data-doc-type=\"story\""),
        "no story group:\n{body}"
    );
}

#[tokio::test]
async fn filter_fragment_returns_only_matching_subset() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    fixture.write_rfc("RFC-002-gamma.md", "Gamma RFC", "accepted");
    fixture.write_story("STORY-001-beta.md", "Beta story", "draft", None);

    // Filter to status=accepted: only RFC-002 should appear.
    let (status, body) = get(store(&fixture), "/fragment/list?status=accepted").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("RFC-002"), "expected RFC-002:\n{body}");
    assert!(
        !body.contains("RFC-001"),
        "RFC-001 should be filtered out:\n{body}"
    );
    assert!(
        !body.contains("STORY-001"),
        "STORY-001 should be filtered out:\n{body}"
    );
    // Fragment only, not the full page chrome.
    assert!(
        !body.contains("<!DOCTYPE html>"),
        "fragment must omit page chrome"
    );
}

#[tokio::test]
async fn empty_filter_returns_all_documents() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    fixture.write_rfc("RFC-002-gamma.md", "Gamma RFC", "accepted");

    let (status, body) = get(store(&fixture), "/fragment/list?status=&tag=").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("RFC-001"));
    assert!(body.contains("RFC-002"));
}

#[tokio::test]
async fn doc_page_renders_frontmatter_header_and_body_html() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-042-page.md",
        "---\ntitle: \"Page RFC\"\ntype: rfc\nstatus: accepted\nauthor: \"alice\"\ndate: 2026-02-03\ntags: [web, render]\n---\n\n## Heading\n\nSome **bold** prose in the body.\n",
    );

    let (status, body) = get(store(&fixture), "/doc/RFC-042").await;

    assert_eq!(status, StatusCode::OK);
    // Frontmatter header fields present.
    assert!(body.contains("Page RFC"), "title missing:\n{body}");
    assert!(body.contains("rfc"), "type missing:\n{body}");
    assert!(body.contains("accepted"), "status missing:\n{body}");
    assert!(body.contains("alice"), "author missing:\n{body}");
    assert!(body.contains("2026-02-03"), "date missing:\n{body}");
    assert!(body.contains("web"), "tag missing:\n{body}");
    // Markdown body rendered to HTML, not raw markdown.
    assert!(
        body.contains("<h2>Heading</h2>"),
        "expected rendered <h2>:\n{body}"
    );
    assert!(
        body.contains("<strong>bold</strong>"),
        "expected rendered <strong>:\n{body}"
    );
}

#[tokio::test]
async fn doc_page_emits_grid_container_and_status_swatch_hooks() {
    // ITERATION-243: the document-page CSS styles existing class hooks, so the
    // template must keep emitting them. Asserts the asymmetric-grid container,
    // the metadata <dl>, and the label-first status swatch keyed off
    // `data-status` (the hook the status legend colors).
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-044-hooks.md",
        "---\ntitle: \"Hooks RFC\"\ntype: rfc\nstatus: in-progress\nauthor: \"alice\"\ndate: 2026-02-03\ntags: []\n---\n\nbody\n",
    );

    let (status, body) = get(store(&fixture), "/doc/RFC-044").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("class=\"doc-grid\""),
        "missing doc-grid container:\n{body}"
    );
    assert!(
        body.contains("class=\"doc-frontmatter\""),
        "missing frontmatter <dl>:\n{body}"
    );
    assert!(
        body.contains("class=\"doc-status\" data-status=\"in-progress\""),
        "status must carry data-status for the legend:\n{body}"
    );
    assert!(
        body.contains("class=\"status-swatch\""),
        "missing leading status swatch:\n{body}"
    );
    assert!(
        body.contains("class=\"doc-body\""),
        "missing reading-column body:\n{body}"
    );
}

#[tokio::test]
async fn doc_page_renders_github_deep_link_when_coords_resolved() {
    use lazyspec::engine::github_url::RepoCoords;

    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-042-page.md",
        "---\ntitle: \"Page RFC\"\ntype: rfc\nstatus: accepted\nauthor: \"alice\"\ndate: 2026-02-03\ntags: []\n---\n\nbody\n",
    );

    let mut st = state(store(&fixture));
    st.coords = Some(RepoCoords {
        owner: "acme".to_string(),
        repo: "widgets".to_string(),
        branch: Some("main".to_string()),
    });
    let app = router(st);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/doc/RFC-042")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    assert!(
        body.contains("https://github.com/acme/widgets/blob/main/docs/rfcs/RFC-042-page.md"),
        "expected blob deep-link:\n{body}"
    );
    assert!(
        body.contains("edit on GitHub"),
        "expected link text:\n{body}"
    );
}

#[tokio::test]
async fn doc_page_omits_github_link_when_coords_unresolved() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    // Default `state(..)` carries `coords: None` -> no link rendered.
    let (status, body) = get(store(&fixture), "/doc/RFC-001").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("edit on GitHub"),
        "deep-link should be omitted with no coords:\n{body}"
    );
}

#[tokio::test]
async fn doc_page_expands_refs_inline() {
    use std::process::Command;

    let fixture = TestFixture::new();
    let root = fixture.root();
    for args in [
        &["init"][..],
        &["config", "user.email", "test@test.com"][..],
        &["config", "user.name", "Test"][..],
    ] {
        Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
    }
    std::fs::write(root.join("snippet.txt"), "REF_EXPANDED_MARKER\n").unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "snippet"])
        .current_dir(root)
        .output()
        .unwrap();

    fixture.write_doc(
        "docs/rfcs/RFC-043-ref.md",
        "---\ntitle: \"Ref RFC\"\ntype: rfc\nstatus: draft\nauthor: \"bob\"\ndate: 2026-02-04\ntags: []\n---\n\nSee:\n\n@ref snippet.txt\n",
    );

    let (status, body) = get(store(&fixture), "/doc/RFC-043").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("REF_EXPANDED_MARKER"),
        "ref content not expanded inline:\n{body}"
    );
    assert!(
        !body.contains("@ref snippet.txt"),
        "literal @ref directive should not survive:\n{body}"
    );
}

#[tokio::test]
async fn doc_page_unknown_id_returns_404_not_found_page() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/doc/UNKNOWN-999").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.to_lowercase().contains("not found"),
        "expected a rendered not-found page:\n{body}"
    );
    assert!(body.contains("UNKNOWN-999"), "should echo the id:\n{body}");
}

#[tokio::test]
async fn list_rows_link_to_doc_pages() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("/doc/RFC-001"),
        "list row should link to /doc/RFC-001:\n{body}"
    );
}

#[tokio::test]
async fn list_and_search_fragment_rows_are_byte_identical() {
    // AC6 row-parity: list_row.html is the shared partial for both the grouped
    // list and the search fragment, so a doc's rendered <li> must be identical
    // across the two surfaces (no layout shift on an HTMX swap).
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-parity.md",
        "---\ntitle: \"Parity term doc\"\ntype: rfc\nstatus: in-progress\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let (list_status, list_body) = get(store(&fixture), "/").await;
    let (search_status, search_body) = get(store(&fixture), "/search?q=parity").await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(search_status, StatusCode::OK);
    assert!(
        !search_body.contains("<!DOCTYPE html>"),
        "search must return a fragment:\n{search_body}"
    );

    let list_row = row_for(&list_body, "RFC-001");
    let search_row = row_for(&search_body, "RFC-001");
    assert_eq!(
        list_row, search_row,
        "list and search rows must be byte-identical for swap parity"
    );
}

/// Extract the full `<li ...>...</li>` row for a given `data-id` from a list or
/// search fragment. Used to assert byte-identical row markup across surfaces.
fn row_for(body: &str, id: &str) -> String {
    let needle = format!("<li data-id=\"{id}\"");
    let start = body
        .find(&needle)
        .unwrap_or_else(|| panic!("no row for {id} in:\n{body}"));
    let rest = &body[start..];
    let end = rest.find("</li>").expect("row must be closed") + "</li>".len();
    rest[..end].to_string()
}

/// Extract `data-id` values in document order from a list/search fragment, which
/// equals the row render order.
fn ids_in_order(body: &str) -> Vec<String> {
    body.match_indices("data-id=\"")
        .map(|(i, m)| {
            let rest = &body[i + m.len()..];
            let end = rest.find('"').unwrap();
            rest[..end].to_string()
        })
        .collect()
}

#[tokio::test]
async fn search_id_order_matches_engine_search_order() {
    use lazyspec::engine::fs::RealFileSystem;

    let fixture = TestFixture::new();
    // Distinct dates so the engine's date-asc ordering is observable and crosses
    // type boundaries (search results are not grouped by type).
    fixture.write_doc(
        "docs/rfcs/RFC-010-search-alpha.md",
        "---\ntitle: \"Searchterm alpha\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-03-03\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-010-search-beta.md",
        "---\ntitle: \"Searchterm beta\"\ntype: story\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-010-search-gamma.md",
        "---\ntitle: \"Searchterm gamma\"\ntype: iteration\nstatus: draft\nauthor: \"t\"\ndate: 2026-02-02\ntags: []\n---\n",
    );
    // A non-matching doc to ensure filtering happens.
    fixture.write_rfc("RFC-011-other.md", "Unrelated title", "draft");

    let st = store(&fixture);

    // Engine order (oracle): ids in the order Store::search returns them.
    let engine_ids: Vec<String> = st
        .search("searchterm", &RealFileSystem)
        .iter()
        .map(|r| r.doc.id.clone())
        .collect();

    let (status, body) = get(st, "/search?q=searchterm").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<!DOCTYPE html>"),
        "search must return a fragment, not the full page"
    );
    assert_eq!(
        ids_in_order(&body),
        engine_ids,
        "search route id order must equal engine search order:\n{body}"
    );
    assert!(
        !body.contains("RFC-011"),
        "non-matching doc must be excluded:\n{body}"
    );
}

#[tokio::test]
async fn search_empty_query_returns_full_list() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    fixture.write_story("STORY-001-beta.md", "Beta story", "draft", None);

    let (status, body) = get(store(&fixture), "/search?q=").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("RFC-001"), "missing RFC-001:\n{body}");
    assert!(body.contains("STORY-001"), "missing STORY-001:\n{body}");
    // Same content shape as the GET / list region (grouped by type), no chrome.
    assert!(
        !body.contains("<!DOCTYPE html>"),
        "fragment must omit page chrome"
    );
    assert!(
        body.contains("data-doc-type=\"rfc\""),
        "empty query should reuse the grouped list fragment:\n{body}"
    );
}

#[tokio::test]
async fn search_no_match_renders_empty_state() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/search?q=zzznomatchzzz").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("RFC-001"),
        "stale list must not appear for a no-match query:\n{body}"
    );
    assert!(
        body.to_lowercase().contains("no results"),
        "expected an empty-result state:\n{body}"
    );
}

#[tokio::test]
async fn list_page_has_search_input_targeting_search_route() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("hx-get=\"/search\""),
        "search input should htmx-GET /search:\n{body}"
    );
    assert!(
        body.contains("name=\"q\""),
        "search input should send q:\n{body}"
    );
}

// --- GET /graph (STORY-179 / ITERATION-237) ----------------------------------

/// The `data-id` values in render (document) order across the whole graph tree.
fn graph_ids_in_order(body: &str) -> Vec<String> {
    ids_in_order(body)
}

#[tokio::test]
async fn graph_renders_nested_ul_tree_in_default_sort_order() {
    // A single implements chain: RFC-001 -> STORY-001 -> ITERATION-001 plus a
    // second root RFC-002 -> STORY-002. GraphSort::default() (path-asc) orders
    // roots and siblings by path, so RFC-001's subtree precedes RFC-002's.
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-base.md", "Base", "draft");
    fixture.write_rfc("RFC-002-other.md", "Other", "draft");
    fixture.write_story("STORY-001-mid.md", "Mid", "draft", Some("RFC-001"));
    fixture.write_story("STORY-002-side.md", "Side", "draft", Some("RFC-002"));
    fixture.write_iteration("ITERATION-001-leaf.md", "Leaf", "draft", Some("STORY-001"));

    let (status, body) = get(store(&fixture), "/graph").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<!DOCTYPE html>"), "expected full HTML page");
    // A nested <ul> tree is rendered (at least the root list and one nested list).
    assert!(
        body.matches("<ul").count() >= 2,
        "expected a nested <ul> tree:\n{body}"
    );
    // Topologically-sorted, default (path) order.
    assert_eq!(
        graph_ids_in_order(&body),
        vec![
            "RFC-001".to_string(),
            "STORY-001".to_string(),
            "ITERATION-001".to_string(),
            "RFC-002".to_string(),
            "STORY-002".to_string(),
        ],
        "graph tree must be ordered by GraphSort::default():\n{body}"
    );
    // Nesting: the iteration's depth attribute reflects its tree level.
    assert!(
        body.contains("data-id=\"ITERATION-001\" data-depth=\"2\""),
        "leaf should be nested at depth 2:\n{body}"
    );
}

#[tokio::test]
async fn graph_diamond_does_not_re_emit_shared_subtree() {
    // RFC-001 root; STORY-001 and STORY-002 both implement it; ITERATION-001
    // implements BOTH stories (the diamond). The shared leaf is drawn in full
    // under STORY-001, then repeated as a plain row under STORY-002 WITHOUT its
    // subtree re-emitted. Here the leaf has no children, so "no duplicate-subtree
    // recursion" shows as: ITERATION-001 appears exactly twice and never spawns a
    // second nested <ul>.
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-base.md", "Base", "draft");
    fixture.write_story("STORY-001-left.md", "Left", "draft", Some("RFC-001"));
    fixture.write_story("STORY-002-right.md", "Right", "draft", Some("RFC-001"));
    fixture.write_doc(
        "docs/iterations/ITERATION-001-leaf.md",
        "---\ntitle: \"Leaf\"\ntype: iteration\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: STORY-001\n- implements: STORY-002\n---\n",
    );

    let (status, body) = get(store(&fixture), "/graph").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        graph_ids_in_order(&body),
        vec![
            "RFC-001".to_string(),
            "STORY-001".to_string(),
            "ITERATION-001".to_string(),
            "STORY-002".to_string(),
            "ITERATION-001".to_string(),
        ],
        "shared leaf full under STORY-001, repeated as a plain row under STORY-002:\n{body}"
    );
    assert_eq!(
        body.matches("data-id=\"ITERATION-001\"").count(),
        2,
        "diamond leaf appears under each parent, exactly twice:\n{body}"
    );
}

#[tokio::test]
async fn graph_cycle_terminates_each_node_once() {
    // RFC-001 -> RFC-002 -> RFC-001 (a cycle with no root). The render must
    // terminate, draw each node exactly once, and drop the back-edge.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-a.md",
        "---\ntitle: \"A\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-002\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-b.md",
        "---\ntitle: \"B\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-001\n---\n",
    );

    let (status, body) = get(store(&fixture), "/graph").await;

    assert_eq!(status, StatusCode::OK);
    let ids = graph_ids_in_order(&body);
    assert_eq!(
        ids,
        vec!["RFC-001".to_string(), "RFC-002".to_string()],
        "cycle terminates: each node exactly once, back-edge dropped:\n{body}"
    );
    assert_eq!(
        body.matches("data-id=\"RFC-001\"").count(),
        1,
        "RFC-001 drawn once:\n{body}"
    );
    assert_eq!(
        body.matches("data-id=\"RFC-002\"").count(),
        1,
        "RFC-002 drawn once:\n{body}"
    );
}

#[tokio::test]
async fn graph_status_carries_data_status_and_swatch() {
    // ITERATION-244: graph status reuses the doc-page swatch treatment, so each
    // graph node's status must carry data-status plus a leading status-swatch
    // span (the hooks the shared per-status color rules key off).
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-base.md", "Base", "in-progress");

    let (status, body) = get(store(&fixture), "/graph").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("class=\"graph-status\" data-status=\"in-progress\""),
        "graph status must carry data-status for the swatch legend:\n{body}"
    );
    assert!(
        body.contains(
            "class=\"graph-status\" data-status=\"in-progress\"><span class=\"status-swatch\">"
        ),
        "graph status must lead with a status-swatch span:\n{body}"
    );
}

// --- GET /graph?pivot= : pivot picker parity with TUI (ITERATION-246) ---------

/// A graph fixture spanning two doc-types and one tagged story, enough to
/// exercise the All / type / tag pivots.
fn pivot_fixture() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-base.md", "Base", "draft");
    // A tagged story under the RFC so the tag pivot can re-root on it.
    fixture.write_doc(
        "docs/stories/STORY-001-tagged.md",
        "---\ntitle: \"Tagged\"\ntype: story\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags:\n- alpha\nrelated:\n- implements: RFC-001\n---\n",
    );
    fixture.write_iteration("ITERATION-001-leaf.md", "Leaf", "draft", Some("STORY-001"));
    fixture
}

#[tokio::test]
async fn graph_pivot_picker_lists_all_plus_types_and_tags() {
    let fixture = pivot_fixture();
    let (status, body) = get(store(&fixture), "/graph").await;

    assert_eq!(status, StatusCode::OK);
    // The View section offers the Graph link in place of the old All pivot row.
    assert!(
        body.contains("href=\"/graph\""),
        "sidebar must offer a Graph view link:\n{body}"
    );
    assert!(
        body.contains("href=\"/graph?pivot=type:rfc\""),
        "sidebar filter section must offer a type pivot:\n{body}"
    );
    assert!(
        body.contains("href=\"/graph?pivot=tag:alpha\""),
        "sidebar filter section must offer a tag pivot:\n{body}"
    );
}

#[tokio::test]
async fn graph_pivot_type_reroots_on_that_type() {
    let fixture = pivot_fixture();
    let (status, body) = get(store(&fixture), "/graph?pivot=type:rfc").await;

    assert_eq!(status, StatusCode::OK);
    // RFC-001 is the only rfc, so it is the sole root; its descendants follow.
    let ids = graph_ids_in_order(&body);
    assert_eq!(
        ids.first().map(String::as_str),
        Some("RFC-001"),
        "type:rfc pivot must root on the rfc:\n{body}"
    );

    // Now anchor on story: the rfc ancestor is pruned, the story becomes a root.
    let (_s, body) = get(store(&fixture), "/graph?pivot=type:story").await;
    let ids = graph_ids_in_order(&body);
    assert!(
        !ids.contains(&"RFC-001".to_string()),
        "type:story pivot prunes the ancestor rfc:\n{body}"
    );
    assert_eq!(
        ids.first().map(String::as_str),
        Some("STORY-001"),
        "type:story pivot roots on the story:\n{body}"
    );
}

#[tokio::test]
async fn graph_pivot_tag_reroots_on_tagged_docs() {
    let fixture = pivot_fixture();
    let (status, body) = get(store(&fixture), "/graph?pivot=tag:alpha").await;

    assert_eq!(status, StatusCode::OK);
    let ids = graph_ids_in_order(&body);
    // The tagged story re-roots; its untagged ancestor rfc is pruned.
    assert_eq!(
        ids,
        vec!["STORY-001".to_string(), "ITERATION-001".to_string()],
        "tag:alpha pivot keeps the tagged story and its descendant only:\n{body}"
    );
}

#[tokio::test]
async fn graph_pivot_marks_active_row() {
    let fixture = pivot_fixture();

    // Default view: no filter entry is active, but the View section marks Graph.
    let (_s, body) = get(store(&fixture), "/graph").await;
    assert!(
        body.contains("class=\"nav-item is-active\" href=\"/graph\""),
        "Graph view link must be active on the default graph view:\n{body}"
    );

    // Anchored view: the matching filter entry is active.
    let (_s, body) = get(store(&fixture), "/graph?pivot=type:rfc").await;
    assert!(
        body.contains("class=\"nav-item is-active\" href=\"/graph?pivot=type:rfc\""),
        "the selected pivot filter must carry the active marker:\n{body}"
    );
}

#[tokio::test]
async fn graph_has_no_second_pivot_rail_and_one_sidebar() {
    let fixture = pivot_fixture();
    let (status, body) = get(store(&fixture), "/graph").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("graph-pivot"),
        "graph page must not render the old second pivot rail:\n{body}"
    );
    assert_eq!(
        body.matches("class=\"app-sidebar\"").count(),
        1,
        "exactly one sidebar must be present:\n{body}"
    );
}

#[tokio::test]
async fn list_filter_entry_uses_list_href_graph_uses_pivot_href() {
    let fixture = pivot_fixture();
    let (_s, list) = get(store(&fixture), "/").await;
    assert!(
        list.contains("href=\"/?type=rfc\""),
        "list view type filter must use list href:\n{list}"
    );
    let (_s, graph) = get(store(&fixture), "/graph").await;
    assert!(
        graph.contains("href=\"/graph?pivot=type:rfc\""),
        "graph view same entry must use pivot href:\n{graph}"
    );
}

#[tokio::test]
async fn list_active_filter_matches_query_param() {
    let fixture = pivot_fixture();
    let (_s, body) = get(store(&fixture), "/?type=rfc").await;
    assert!(
        body.contains("class=\"nav-item is-active\" href=\"/?type=rfc\""),
        "the active type filter must be marked is-active:\n{body}"
    );
}

#[tokio::test]
async fn view_section_marks_list_active_on_root_and_graph_active_on_graph() {
    let fixture = pivot_fixture();
    let (_s, root) = get(store(&fixture), "/").await;
    assert!(
        root.contains("class=\"nav-item is-active\" href=\"/\""),
        "List must be active on /:\n{root}"
    );
    let (_s, graph) = get(store(&fixture), "/graph").await;
    assert!(
        graph.contains("class=\"nav-item is-active\" href=\"/graph\""),
        "Graph must be active on /graph:\n{graph}"
    );
}

#[tokio::test]
async fn pages_emit_sticky_app_header_markup() {
    let fixture = pivot_fixture();
    let (_s, body) = get(store(&fixture), "/").await;
    assert!(
        body.contains("app-header"),
        "header markup must carry app-header:\n{body}"
    );
}

#[test]
fn default_port_is_8787() {
    assert_eq!(DEFAULT_PORT, 8787);
}

/// `--port` overrides the default; absent the flag the default is used. Tested
/// against the pure `resolve_port` to avoid binding a real socket (the sandbox
/// denies real TCP binds).
#[test]
fn resolve_port_defaults_and_overrides() {
    use lazyspec::web::server::resolve_port;

    assert_eq!(resolve_port(None), 8787);
    assert_eq!(resolve_port(Some(9000)), 9000);
}

// --- Static assets: stylesheet + fonts (ITERATION-242) -----------------------

/// The `<link>` every page head must carry to pull in the stylesheet (AC1).
const STYLESHEET_LINK: &str = "<link rel=\"stylesheet\" href=\"/static/lazyspec.css\">";

#[tokio::test]
async fn stylesheet_route_returns_200_text_css() {
    use axum::http::header::CONTENT_TYPE;

    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    // Inspecting the Content-Type header requires the manual oneshot path; the
    // `get` helper discards response headers.
    let app = router(state(store(&fixture)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/lazyspec.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "text/css; charset=utf-8",
        "stylesheet must be served as CSS"
    );

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    // Proves the real stylesheet is served, not an empty stub.
    assert!(body.contains(":root"), "expected CSS :root block:\n{body}");
    assert!(
        body.contains("--accent"),
        "expected --accent token:\n{body}"
    );
}

/// AC3 embedded-asset guard (RFC-054 / STORY-188): the stylesheet must resolve
/// from bytes baked into the binary at compile time (`include_str!` in
/// `src/web/assets.rs`), never from a runtime filesystem path. The unsigned
/// `.app` bundle ships no external asset dir, so a regression that reintroduced a
/// runtime path read would 404 (or panic) in the packaged app while still passing
/// under `cargo test` from the repo root.
///
/// This proves the no-runtime-path property behaviorally: it serves the
/// stylesheet with the process CWD moved into an empty temp dir that contains no
/// `static/` directory. An `include_str!`-embedded stylesheet is unaffected; a
/// runtime `static/lazyspec.css` read would fail. Serialized against every other
/// CWD-touching test via a shared mutex, and the CWD is always restored, so the
/// parallel suite stays deterministic.
static CWD_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test(flavor = "current_thread")]
async fn stylesheet_serves_embedded_bytes_with_no_runtime_asset_path() {
    let _lock = CWD_GUARD.lock().unwrap_or_else(|e| e.into_inner());

    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    let app = router(state(store(&fixture)));

    // An empty dir with no `static/` -- a runtime path read for the CSS would fail
    // here; an embedded (`include_str!`) stylesheet is indifferent to the CWD.
    let empty = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(empty.path()).unwrap();

    let result = app
        .oneshot(
            Request::builder()
                .uri("/static/lazyspec.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    std::env::set_current_dir(&original).unwrap();

    let response = result.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "embedded stylesheet must serve regardless of CWD (no runtime asset path)"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.is_empty(), "embedded stylesheet must be non-empty");
    // Stable markers from static/lazyspec.css, proving the real embedded content.
    assert!(
        body.contains(":root"),
        "embedded CSS must carry the :root block:\n{body}"
    );
    assert!(
        body.contains("--accent"),
        "embedded CSS must carry the --accent token:\n{body}"
    );
}

/// Fonts are a SANDBOX BLOCKER: the woff2 binaries cannot be fetched (network is
/// restricted to github.com, no Google Fonts CDN; git object writes to github are
/// also blocked), so no font is embedded yet and `src/web/assets.rs` 404s every
/// request by design (its `font_bytes` match has only commented-out arms).
///
/// TODO(fonts): WHEN the woff2 binaries land in `static/fonts/` and the
/// `font_bytes` match arms in `src/web/assets.rs` are uncommented, FLIP this test
/// to assert `StatusCode::OK` and Content-Type `font/woff2`.
#[tokio::test]
async fn font_route_returns_404_until_fonts_embedded() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let app = router(state(store(&fixture)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/fonts/archivo-latin.woff2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn doc_page_head_links_stylesheet() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/doc/RFC-001").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(STYLESHEET_LINK),
        "doc page head must link the stylesheet:\n{body}"
    );
}

#[tokio::test]
async fn list_page_head_links_stylesheet() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(STYLESHEET_LINK),
        "list page head must link the stylesheet:\n{body}"
    );
}

#[tokio::test]
async fn graph_page_head_links_stylesheet() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/graph").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(STYLESHEET_LINK),
        "graph page head must link the stylesheet:\n{body}"
    );
}

// --- App chrome: header, sidebar, search modal (ITERATION-245) ----------------

/// All three top-level pages must carry the shared shell chrome.
#[tokio::test]
async fn all_pages_carry_header_sidebar_and_search_modal() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    for uri in ["/", "/graph", "/doc/RFC-001"] {
        let (status, body) = get(store(&fixture), uri).await;
        assert_eq!(status, StatusCode::OK, "page {uri} not OK");
        assert!(
            body.contains("class=\"app-header\""),
            "{uri} missing app-header:\n{body}"
        );
        assert!(
            body.contains("class=\"app-sidebar\""),
            "{uri} missing app-sidebar:\n{body}"
        );
        assert!(
            body.contains("data-search-modal"),
            "{uri} missing search modal:\n{body}"
        );
        assert!(
            body.contains("class=\"search-trigger\""),
            "{uri} missing search trigger:\n{body}"
        );
    }
}

// --- Collapsing icon rail + responsive drawer (ITERATION-251) ----------------

/// The sidebar carries a collapse toggle and inline View icons.
#[tokio::test]
async fn sidebar_has_collapse_toggle_and_view_icons() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("data-sidebar-toggle"),
        "missing collapse toggle:\n{body}"
    );
    assert!(
        body.contains("class=\"nav-icon\""),
        "missing nav-icon svg class:\n{body}"
    );
    assert!(
        body.contains("<svg") && body.contains("aria-hidden=\"true\""),
        "missing inline aria-hidden svg:\n{body}"
    );
    // Existing anchor opening tags must remain intact.
    assert!(
        body.contains("class=\"nav-item is-active\" href=\"/\""),
        "list nav-item anchor changed:\n{body}"
    );
}

/// Filter entries carry a mono first-letter glyph badge for the collapsed rail.
#[tokio::test]
async fn sidebar_filter_entries_carry_nav_glyph() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("class=\"nav-glyph\""),
        "missing nav-glyph badge:\n{body}"
    );
}

/// The header carries a drawer menu button for the mobile off-canvas nav.
#[tokio::test]
async fn header_has_drawer_toggle() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("data-drawer-toggle"),
        "missing drawer toggle:\n{body}"
    );
}

/// All three top-level pages carry both the collapse and drawer controls.
#[tokio::test]
async fn all_pages_carry_collapse_and_drawer_controls() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    for uri in ["/", "/graph", "/doc/RFC-001"] {
        let (status, body) = get(store(&fixture), uri).await;
        assert_eq!(status, StatusCode::OK, "page {uri} not OK");
        assert!(
            body.contains("data-sidebar-toggle"),
            "{uri} missing collapse toggle:\n{body}"
        );
        assert!(
            body.contains("data-drawer-toggle"),
            "{uri} missing drawer toggle:\n{body}"
        );
    }
}

/// Each page emits the no-flash head script keyed on the localStorage key.
#[tokio::test]
async fn pages_have_no_flash_sidebar_script() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    for uri in ["/", "/graph", "/doc/RFC-001"] {
        let (status, body) = get(store(&fixture), uri).await;
        assert_eq!(status, StatusCode::OK, "page {uri} not OK");
        assert!(
            body.contains("ls-sidebar"),
            "{uri} missing no-flash sidebar script:\n{body}"
        );
    }
}

/// The header repo chip shows repo and branch from AppState, joined by a `·`.
#[tokio::test]
async fn header_chip_shows_repo_and_branch() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("testrepo"), "missing repo name:\n{body}");
    assert!(body.contains("· main"), "missing branch chip:\n{body}");
}

/// The sidebar lists each distinct doc-type as a `/?type=...` link.
#[tokio::test]
async fn sidebar_lists_doc_type_links() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    fixture.write_story("STORY-001-beta.md", "Beta story", "draft", None);

    let (status, body) = get(store(&fixture), "/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("href=\"/?type=rfc\""),
        "missing rfc type link:\n{body}"
    );
    assert!(
        body.contains("href=\"/?type=story\""),
        "missing story type link:\n{body}"
    );
    assert!(
        body.contains("href=\"/graph\""),
        "missing graph link:\n{body}"
    );
}

/// `GET /?type=rfc` filters the list to only rfc rows.
#[tokio::test]
async fn list_page_type_param_filters_to_type() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");
    fixture.write_story("STORY-001-beta.md", "Beta story", "draft", None);

    let (status, body) = get(store(&fixture), "/?type=rfc").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("RFC-001"), "rfc should appear:\n{body}");
    assert!(
        body.contains("data-doc-type=\"rfc\""),
        "rfc group should appear:\n{body}"
    );
    assert!(
        !body.contains("STORY-001"),
        "story should be filtered out:\n{body}"
    );
    assert!(
        !body.contains("data-doc-type=\"story\""),
        "story group should be filtered out:\n{body}"
    );
}

/// The doc page renders each tag as a `/?tag=...` filter link.
#[tokio::test]
async fn doc_page_tags_are_filter_links() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-050-tagged.md",
        "---\ntitle: \"Tagged RFC\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: [web]\n---\n\nbody\n",
    );

    let (status, body) = get(store(&fixture), "/doc/RFC-050").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("class=\"tag tag--h"),
        "tag should carry a categorical hue class:\n{body}"
    );
    assert!(
        body.contains("href=\"/?tag=web\""),
        "tag should be a filter link:\n{body}"
    );
    assert!(body.contains(">web</a>"), "tag text missing:\n{body}");
}

/// A list-fragment row for a tagged doc renders colored tag chips that link to
/// the tag filter, and its status carries the data-status + swatch hooks.
#[tokio::test]
async fn list_row_renders_tag_chips_and_status_swatch() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-051-tagged.md",
        "---\ntitle: \"Tagged RFC\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: [web]\n---\n\nbody\n",
    );

    let (status, body) = get(store(&fixture), "/fragment/list?status=&tag=").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("tag--h"),
        "tag chip should carry a hue class:\n{body}"
    );
    assert!(
        body.contains("href=\"/?tag=web\""),
        "tag chip should link to the tag filter:\n{body}"
    );
    assert!(body.contains(">web</a>"), "tag text missing:\n{body}");
    assert!(
        body.contains("class=\"doc-status\" data-status=\"draft\""),
        "row status must carry data-status:\n{body}"
    );
    assert!(
        body.contains("class=\"status-swatch\""),
        "row status must carry the swatch:\n{body}"
    );
}

// --- GET /doc/{id}: anchored context graph (STORY-183 / ITERATION-248) --------

/// Extract the inner markup of the `<section class="doc-context" ...>` block, or
/// `None` when the page has no Context section. Used to scope group/order asserts
/// to the context band (so a body mention of an id never trips a context assert).
fn context_section(body: &str) -> Option<String> {
    let start = body.find("<section class=\"doc-context\"")?;
    let rest = &body[start..];
    let end = rest
        .find("</section>")
        .expect("context section must be closed")
        + "</section>".len();
    Some(rest[..end].to_string())
}

/// A doc that both implements a parent (chain ancestor) and is implemented-by a
/// child (chain descendant), plus a related-to peer, surfaces all three in the
/// right groups -- matching `lazyspec context <id>`.
#[tokio::test]
async fn doc_page_renders_context_in_three_groups() {
    let fixture = TestFixture::new();
    // RFC-001 (parent) <- STORY-001 (target) <- ITERATION-001 (child). STORY-001
    // is also related-to RFC-002 (a peer).
    fixture.write_rfc("RFC-001-parent.md", "Parent RFC", "accepted");
    fixture.write_rfc("RFC-002-peer.md", "Peer RFC", "draft");
    fixture.write_doc(
        "docs/stories/STORY-001-target.md",
        "---\ntitle: \"Target story\"\ntype: story\nstatus: in-progress\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-001\n- related-to: RFC-002\n---\n\nbody\n",
    );
    fixture.write_iteration(
        "ITERATION-001-child.md",
        "Child iter",
        "draft",
        Some("STORY-001"),
    );

    let (status, body) = get(store(&fixture), "/doc/STORY-001").await;

    assert_eq!(status, StatusCode::OK);
    let ctx = context_section(&body).expect("expected a Context section:\n");

    // Ancestor (parent), descendant (child), and related peer each appear, each
    // linking to its /doc page, scoped to their direction group.
    let ancestors = group_block(&ctx, "ancestors");
    let descendants = group_block(&ctx, "descendants");
    let related = group_block(&ctx, "related");

    assert!(
        ancestors.contains("/doc/RFC-001"),
        "parent must be an ancestor:\n{ctx}"
    );
    assert!(
        descendants.contains("/doc/ITERATION-001"),
        "child must be a descendant:\n{ctx}"
    );
    assert!(
        related.contains("/doc/RFC-002"),
        "peer must be related:\n{ctx}"
    );

    // Cross-group leakage guard: the child is not an ancestor, the parent not a
    // descendant.
    assert!(
        !ancestors.contains("/doc/ITERATION-001"),
        "child must not appear under ancestors:\n{ctx}"
    );
    assert!(
        !descendants.contains("/doc/RFC-001"),
        "parent must not appear under descendants:\n{ctx}"
    );
}

/// Extract the inner markup of one `data-direction="..."` context group.
fn group_block(ctx: &str, direction: &str) -> String {
    let needle = format!("data-direction=\"{direction}\"");
    let start = ctx
        .find(&needle)
        .unwrap_or_else(|| panic!("no {direction} group in:\n{ctx}"));
    let rest = &ctx[start..];
    let end = rest.find("</div>").expect("group must be closed") + "</div>".len();
    rest[..end].to_string()
}

/// A doc with no relations renders cleanly: a 200, and no Context section at all.
#[tokio::test]
async fn doc_page_with_no_relations_omits_context_section() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-lonely.md", "Lonely RFC", "draft");

    let (status, body) = get(store(&fixture), "/doc/RFC-001").await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        context_section(&body).is_none(),
        "a relation-less doc must render no Context section:\n{body}"
    );
}

/// The doc/graph back-link carries the styled `back-link` class.
#[tokio::test]
async fn back_link_is_styled() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha RFC", "draft");

    for uri in ["/graph", "/doc/RFC-001"] {
        let (status, body) = get(store(&fixture), uri).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains("class=\"back-link\""),
            "{uri} missing styled back-link:\n{body}"
        );
    }
}
