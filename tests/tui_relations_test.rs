mod common;

use common::TestFixture;
use lazyspec::tui::app::{App, PreviewTab};

fn setup_app_with_relations() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-001-test.md", "Test RFC", "accepted");
    fixture.write_story(
        "STORY-001-test.md",
        "Test Story",
        "draft",
        Some("docs/rfcs/RFC-001-test.md"),
    );
    fixture.write_story(
        "STORY-002-test.md",
        "Test Story Two",
        "draft",
        Some("docs/rfcs/RFC-001-test.md"),
    );
    fixture.write_iteration(
        "ITER-001-test.md",
        "Test Iter",
        "draft",
        Some("docs/stories/STORY-001-test.md"),
    );

    let store = fixture.store();
    let app = App::new(store);
    (fixture, app)
}

#[test]
fn test_toggle_preview_tab() {
    let (_fixture, mut app) = setup_app_with_relations();
    assert_eq!(app.preview_tab, PreviewTab::Preview);

    app.toggle_preview_tab();
    assert_eq!(app.preview_tab, PreviewTab::Relations);
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_toggle_preview_tab_back() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.toggle_preview_tab();
    app.toggle_preview_tab();
    assert_eq!(app.preview_tab, PreviewTab::Preview);
}

#[test]
fn test_toggle_preview_tab_resets_relation() {
    let (_fixture, mut app) = setup_app_with_relations();
    app.selected_relation = 1;

    app.toggle_preview_tab();
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_relation_count() {
    let (_fixture, mut app) = setup_app_with_relations();

    // RFC at index 0 has relations (stories implement it)
    app.selected_type = 0;
    app.selected_doc = 0;
    assert!(app.relation_count() > 0);

    // ADR type (index 1) has no docs, so relation_count is 0
    app.selected_type = 1;
    app.selected_doc = 0;
    assert_eq!(app.relation_count(), 0);
}

#[test]
fn test_move_relation_down() {
    let (_fixture, mut app) = setup_app_with_relations();

    // RFC has 2+ relations (two stories implement it)
    app.selected_type = 0;
    app.selected_doc = 0;
    let count = app.relation_count();
    assert!(count >= 2, "RFC should have at least 2 relations, got {count}");

    app.selected_relation = 0;
    app.move_relation_down();
    assert_eq!(app.selected_relation, 1);
}

#[test]
fn test_move_relation_down_clamps() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.selected_type = 0;
    app.selected_doc = 0;
    let count = app.relation_count();
    assert!(count > 0);

    app.selected_relation = count - 1;
    app.move_relation_down();
    assert_eq!(app.selected_relation, count - 1);
}

#[test]
fn test_move_relation_up() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.selected_type = 0;
    app.selected_doc = 0;
    let count = app.relation_count();
    assert!(count >= 2, "RFC should have at least 2 relations, got {count}");

    app.selected_relation = 1;
    app.move_relation_up();
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_move_relation_up_clamps() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.selected_relation = 0;
    app.move_relation_up();
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_navigate_to_relation() {
    let (_fixture, mut app) = setup_app_with_relations();

    // Start at the RFC
    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;

    let count = app.relation_count();
    assert!(count > 0, "RFC should have relations");

    app.selected_relation = 0;
    app.navigate_to_relation();

    // Should have navigated to the related doc (a Story, type index 2)
    assert_eq!(app.selected_type, 2, "should navigate to Story type");
    assert_eq!(app.preview_tab, PreviewTab::Preview);
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_navigate_to_relation_no_doc() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(store);

    let before_type = app.selected_type;
    let before_doc = app.selected_doc;

    app.navigate_to_relation();

    assert_eq!(app.selected_type, before_type);
    assert_eq!(app.selected_doc, before_doc);
}
