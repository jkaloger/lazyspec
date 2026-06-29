#![cfg(feature = "web")]
//! Integration tests for the read-only web view skeleton (STORY-176 /
//! ITERATION-239). The axum `Router` is driven directly via
//! `tower::ServiceExt::oneshot` so no real socket is bound and there is no
//! network flakiness.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use lazyspec::engine::store::Store;
use lazyspec::web::server::{router, DEFAULT_PORT};
use tower::ServiceExt;

use crate::common::TestFixture;

fn store(fixture: &TestFixture) -> Arc<Store> {
    Arc::new(fixture.store())
}

async fn get(store: Arc<Store>, uri: &str) -> (StatusCode, String) {
    let app = router(store);
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
