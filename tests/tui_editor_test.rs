mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::app::{resolve_editor_from, App, ViewMode};

#[test]
fn editor_env_set() {
    assert_eq!(resolve_editor_from(Some("nano"), None), "nano");
}

#[test]
fn editor_env_set_visual_ignored() {
    assert_eq!(resolve_editor_from(Some("nano"), Some("code")), "nano");
}

#[test]
fn visual_fallback() {
    assert_eq!(resolve_editor_from(None, Some("code")), "code");
}

#[test]
fn fallback_to_vi() {
    assert_eq!(resolve_editor_from(None, None), "vi");
}

#[test]
fn empty_editor_falls_through_to_visual() {
    assert_eq!(resolve_editor_from(Some(""), Some("code")), "code");
}

#[test]
fn empty_editor_and_no_visual_falls_to_vi() {
    assert_eq!(resolve_editor_from(Some(""), None), "vi");
}

#[test]
fn e_key_sets_editor_request_in_types_mode() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth RFC", "draft");

    let store = fixture.store();
    let mut app = App::new(store);

    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(app.editor_request.is_some());
    let path = app.editor_request.unwrap();
    assert!(
        path.ends_with("docs/rfcs/RFC-001-auth.md"),
        "expected path ending with docs/rfcs/RFC-001-auth.md, got {:?}",
        path
    );
}

#[test]
fn e_key_sets_editor_request_in_graph_mode() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth RFC", "accepted");
    fixture.write_story(
        "STORY-001-login.md",
        "Login Story",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );

    let store = fixture.store();
    let mut app = App::new(store);

    // Cycle to Graph mode: Types -> Filters -> Metrics -> Graph
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Graph);
    assert!(!app.graph_nodes.is_empty(), "graph_nodes should be populated");

    app.handle_key(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(
        app.editor_request.is_some(),
        "editor_request should be set in Graph mode"
    );
}

#[test]
fn e_key_noop_when_no_document_selected() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(store);

    app.handle_key(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(
        app.editor_request.is_none(),
        "editor_request should be None when no documents exist"
    );
}

#[test]
fn e_key_ignored_during_create_form() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth RFC", "draft");

    let store = fixture.store();
    let mut app = App::new(store);

    // Open the create form
    app.handle_key(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert!(app.create_form.active);

    // Press e while form is active
    app.handle_key(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(
        app.editor_request.is_none(),
        "editor_request should be None during modal create form"
    );
}
