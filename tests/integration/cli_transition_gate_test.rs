use crate::common::TestFixture;
use lazyspec::engine::config::Config;
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::store::Store;
use std::fs;

fn status_of(fixture: &TestFixture, rel: &str) -> String {
    let content = fs::read_to_string(fixture.root().join(rel)).unwrap();
    format!("{}", DocMeta::parse(&content).unwrap().status)
}

// AC4 — reject an off-edge move; the doc's status is unchanged after the call.
#[test]
fn transition_rejects_off_edge_move() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    // draft -> accepted is not an edge in the default lifecycle (only draft->review).
    let err = lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "accepted")],
        Some(&config),
    )
    .unwrap_err();
    assert!(err.to_string().contains("invalid transition"), "got: {err}");

    // The file is unchanged: the bail occurred before any write.
    assert_eq!(status_of(&fixture, "docs/rfcs/RFC-001-test.md"), "draft");
}

// AC4 — accept an on-edge move.
#[test]
fn transition_accepts_on_edge_move() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "review")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(status_of(&fixture, "docs/rfcs/RFC-001-test.md"), "review");
}

// AC4 — a `* -> rejected`/`* -> superseded` edge lets any state move there.
#[test]
fn transition_wildcard_edge_allows_move_from_any_state() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "complete");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "superseded")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(
        status_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        "superseded"
    );
}

// AC4 — setting the status to its current value is a no-op, not a missing edge.
#[test]
fn transition_no_op_to_same_status_is_allowed() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "draft")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(status_of(&fixture, "docs/rfcs/RFC-001-test.md"), "draft");
}

// AC4 — a non-status update performs no transition check.
#[test]
fn transition_check_skipped_for_non_status_updates() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("title", "Renamed")],
        Some(&config),
    )
    .unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.title, "Renamed");
    assert_eq!(format!("{}", meta.status), "draft");
}

// ADR-033 — status-conditioned create gating is abandoned, not relocated. No
// config can declare a gate any more, so `create` never consults a parent's
// status: a child lands with its parent still in the lifecycle's first state.
#[test]
fn create_ignores_the_parents_status() {
    let fixture = TestFixture::new();
    let config = Config::default();
    std::fs::write(
        fixture.root().join(".lazyspec.toml"),
        config.to_toml().unwrap(),
    )
    .unwrap();

    fixture.write_rfc("RFC-001-test.md", "Parent", "draft");
    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "story",
        "Child",
        "test",
        None,
        None,
        |_| {},
    )
    .unwrap()
    .0;
    assert!(path.exists(), "a draft parent does not refuse the child");
}

// ADR-033 — and with no parent document at all, which is the empty-project case
// the old existence-query gate turned into a hard wall.
#[test]
fn create_succeeds_with_no_parent_document_at_all() {
    let fixture = TestFixture::new();
    let config = Config::default();
    std::fs::write(
        fixture.root().join(".lazyspec.toml"),
        config.to_toml().unwrap(),
    )
    .unwrap();

    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "story",
        "Child",
        "test",
        None,
        None,
        |_| {},
    )
    .unwrap()
    .0;
    assert!(path.exists(), "child created freely on an empty project");
}
