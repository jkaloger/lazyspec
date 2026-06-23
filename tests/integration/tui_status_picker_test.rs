use crate::common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::document::Status;
use lazyspec::tui::state::{App, ViewMode};

fn setup_app_with_rfc(title: &str, status: &str) -> (TestFixture, App) {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", title, status);
    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
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

// AC1: opening the picker populates fields from the selected doc. The options
// are the current status (index 0) plus the lifecycle edges out of it.
#[test]
fn test_open_status_picker_populates_from_selected_doc() {
    let (_fixture, mut app) = setup_app_with_rfc("Draft RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&_fixture.config());

    assert!(app.status_picker.active);
    assert_eq!(app.status_picker.selected, 0); // current status first
    assert_eq!(
        app.status_picker.states.first().map(String::as_str),
        Some("draft")
    );
    // draft -> review and the `* -> superseded` wildcard are the only valid moves.
    assert_eq!(
        app.status_picker.states,
        vec!["draft", "review", "superseded"]
    );
    assert_eq!(
        app.status_picker.doc_path,
        std::path::PathBuf::from("docs/rfcs/RFC-001-test.md")
    );
}

// AC1: picker pre-selects the current status (always index 0) and offers only
// the moves reachable from it.
#[test]
fn test_open_status_picker_preselects_current_status() {
    let (_fixture, mut app) = setup_app_with_rfc("Accepted RFC", "accepted");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&_fixture.config());

    assert!(app.status_picker.active);
    assert_eq!(app.status_picker.selected, 0);
    assert_eq!(
        app.status_picker.states.first().map(String::as_str),
        Some("accepted")
    );
    // accepted -> in-progress and the wildcard superseded.
    assert_eq!(
        app.status_picker.states,
        vec!["accepted", "in-progress", "superseded"]
    );
}

// AC2: j/k navigates, clamped at boundaries
#[test]
fn test_status_picker_navigation() {
    let (fixture, mut app) = setup_app_with_rfc("Nav RFC", "draft");
    let root = fixture.root();
    let config = fixture.config();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&fixture.config());
    assert_eq!(app.status_picker.selected, 0);

    // j moves down
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, root, &config);
    assert_eq!(app.status_picker.selected, 1);

    // k moves back up
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, root, &config);
    assert_eq!(app.status_picker.selected, 0);

    // k at 0 stays clamped
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, root, &config);
    assert_eq!(app.status_picker.selected, 0);

    // navigate to max -- draft offers [draft, review, superseded], so index 2.
    let max = app.status_picker.states.len() - 1;
    for _ in 0..10 {
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, root, &config);
    }
    assert_eq!(app.status_picker.selected, max);

    // j at the end stays clamped
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, root, &config);
    assert_eq!(app.status_picker.selected, max);
}

// AC4: confirming writes new status to frontmatter and reloads store
#[test]
fn test_confirm_status_change_updates_frontmatter() {
    let (fixture, mut app) = setup_app_with_rfc("Update RFC", "draft");
    let root = fixture.root();
    let config = fixture.config();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&fixture.config());

    // Select "review" (index 1) -- a valid draft -> review lifecycle edge.
    app.status_picker.selected = 1;
    app.confirm_status_change(root, &config).unwrap();

    // Verify file on disk
    let content = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-test.md")).unwrap();
    assert!(
        content.contains("status: review"),
        "frontmatter should contain 'status: review', got:\n{}",
        content
    );

    // Verify store updated
    let doc = app
        .store
        .get(std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .expect("doc should still exist in store");
    assert_eq!(doc.status, Status::new("review"));

    // Picker should be closed
    assert!(!app.status_picker.active);
}

// AC5: cancelling preserves original status
#[test]
fn test_cancel_status_picker_no_changes() {
    let (fixture, mut app) = setup_app_with_rfc("Safe RFC", "draft");
    let root = fixture.root();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&fixture.config());
    app.close_status_picker();

    assert!(!app.status_picker.active);

    let content = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-test.md")).unwrap();
    assert!(
        content.contains("status: draft"),
        "file should still have 'status: draft'"
    );
}

// AC1: opening picker on empty list is a no-op
#[test]
fn test_status_picker_on_empty_list_noop() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    app.selected_type = 0;
    app.open_status_picker(&fixture.config());

    assert!(!app.status_picker.active);
}

// AC6: picker works in Filters mode
#[test]
fn test_status_picker_in_filters_mode() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-filtered.md", "Filtered RFC", "review");
    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    enter_filters_mode(&mut app, &fixture);
    assert_eq!(app.view_mode, ViewMode::Filters);

    app.selected_doc = 0;
    app.open_status_picker(&fixture.config());

    assert!(app.status_picker.active);
    assert_eq!(app.status_picker.selected, 0); // current status first
    assert_eq!(
        app.status_picker.states.first().map(String::as_str),
        Some("review")
    );
    assert_eq!(
        app.status_picker.doc_path,
        std::path::PathBuf::from("docs/rfcs/RFC-001-filtered.md")
    );
}

// AC1, AC6: pressing 's' key opens the picker via handle_key
#[test]
fn test_handle_key_s_opens_picker() {
    let (fixture, mut app) = setup_app_with_rfc("Key RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.handle_key(
        KeyCode::Char('s'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(app.status_picker.active);
    assert_eq!(app.status_picker.selected, 0);
}
