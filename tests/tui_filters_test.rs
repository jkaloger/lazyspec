mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::document::Status;
use lazyspec::tui::app::{App, FilterField, ViewMode};

fn setup_filters_fixture() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth RFC\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: [security, backend]\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-ui.md",
        "---\ntitle: \"UI RFC\"\ntype: rfc\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: [frontend]\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-login.md",
        "---\ntitle: \"Login Story\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: [security]\n---\n",
    );
    fixture.write_doc(
        "docs/adrs/ADR-001-db.md",
        "---\ntitle: \"DB ADR\"\ntype: adr\nstatus: review\nauthor: \"test\"\ndate: 2026-01-01\ntags: [backend]\n---\n",
    );

    let store = fixture.store();
    let app = App::new(store);
    (fixture, app)
}

fn enter_filters_mode(app: &mut App, fixture: &TestFixture) {
    while app.view_mode != ViewMode::Filters {
        app.handle_key(
            KeyCode::Char('`'),
            KeyModifiers::NONE,
            fixture.root(),
            &fixture.config(),
        );
    }
}

#[test]
fn test_entering_filters_mode_collects_tags() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    assert_eq!(
        app.available_tags,
        vec!["backend", "frontend", "security"],
        "tags should be sorted unique set"
    );
}

#[test]
fn test_filter_field_navigation() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    assert_eq!(app.filter_focused, FilterField::Status);

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::Tag);

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::ClearAction);

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::Status);

    // BackTab goes backwards
    app.handle_key(KeyCode::BackTab, KeyModifiers::SHIFT, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::ClearAction);

    app.handle_key(KeyCode::BackTab, KeyModifiers::SHIFT, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::Tag);

    app.handle_key(KeyCode::BackTab, KeyModifiers::SHIFT, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::Status);
}

#[test]
fn test_cycle_status_filter() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    assert_eq!(app.filter_status, None);

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Draft));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Review));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Accepted));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Rejected));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Superseded));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, None, "should cycle back to None");
}

#[test]
fn test_cycle_tag_filter() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    // Tab to Tag field
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::Tag);

    assert_eq!(app.filter_tag, None);

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_tag, Some("backend".to_string()));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_tag, Some("frontend".to_string()));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_tag, Some("security".to_string()));

    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_tag, None, "should cycle back to None");
}

#[test]
fn test_filtered_docs_returns_matching() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    // Press l once to set status to Draft
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Draft));

    let docs = app.filtered_docs();
    assert_eq!(docs.len(), 2, "should have 2 draft docs");
    for doc in &docs {
        assert_eq!(doc.status, Status::Draft);
    }
}

#[test]
fn test_combined_filters() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    // Set status to Draft
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Draft));

    // Tab to Tag, set to "security"
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    // Cycle tags: None -> backend -> frontend -> security
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_tag, Some("security".to_string()));

    let docs = app.filtered_docs();
    assert_eq!(docs.len(), 2, "should have 2 draft+security docs (Auth RFC and Login Story)");
    for doc in &docs {
        assert_eq!(doc.status, Status::Draft);
        assert!(doc.tags.contains(&"security".to_string()));
    }
}

#[test]
fn test_clear_filters() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    // Set a status filter
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Draft));

    // Tab to Tag, set a tag filter
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(app.filter_tag.is_some());

    // Tab to ClearAction
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_focused, FilterField::ClearAction);

    // Press Enter to clear
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert_eq!(app.filter_status, None);
    assert_eq!(app.filter_tag, None);
    assert_eq!(app.filter_focused, FilterField::Status);
}

#[test]
fn test_doc_navigation_in_filters_mode() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    // All docs visible (no filter), should have 4
    let count = app.filtered_docs().len();
    assert_eq!(count, 4);

    assert_eq!(app.selected_doc, 0);

    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 1);

    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 2);

    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 1);

    // Clamp at 0
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 0);

    // Clamp at max
    for _ in 0..10 {
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }
    assert_eq!(app.selected_doc, count - 1);
}

#[test]
fn test_filters_reset_on_mode_switch() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    // Set a filter
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.filter_status, Some(Status::Draft));

    // Leave Filters mode (backtick)
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_ne!(app.view_mode, ViewMode::Filters);

    // Filters should be reset
    assert_eq!(app.filter_status, None);
    assert_eq!(app.filter_tag, None);

    // Cycle back to Filters
    while app.view_mode != ViewMode::Filters {
        app.handle_key(
            KeyCode::Char('`'),
            KeyModifiers::NONE,
            fixture.root(),
            &fixture.config(),
        );
    }

    assert_eq!(app.filter_status, None);
    assert_eq!(app.filter_tag, None);
}

#[test]
fn test_enter_opens_fullscreen_for_filtered_doc() {
    let (fixture, mut app) = setup_filters_fixture();
    enter_filters_mode(&mut app, &fixture);

    assert_eq!(app.filter_focused, FilterField::Status);
    assert!(!app.fullscreen_doc);

    // Press Enter (focused on Status, not ClearAction) with docs available
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert!(app.fullscreen_doc, "Enter should open fullscreen when not on ClearAction");
}
