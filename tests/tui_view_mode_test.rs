mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::app::{App, PreviewTab, ViewMode};

fn setup_app_with_docs() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/001-first.md",
        "---\ntitle: First RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );
    fixture.write_doc(
        "docs/rfcs/002-second.md",
        "---\ntitle: Second RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-02\ntags: []\n---\nBody\n",
    );

    let store = fixture.store();
    let app = App::new(store, &fixture.config(), lazyspec::tui::terminal_caps::TerminalImageProtocol::Unsupported);
    (fixture, app)
}

#[test]
fn test_app_defaults_to_types_mode() {
    let (_fixture, app) = setup_app_with_docs();
    assert_eq!(app.view_mode, ViewMode::Types);
}

#[test]
fn test_view_mode_next_cycles() {
    assert_eq!(ViewMode::Types.next(), ViewMode::Filters);
    assert_eq!(ViewMode::Filters.next(), ViewMode::Metrics);
    assert_eq!(ViewMode::Metrics.next(), ViewMode::Graph);
    #[cfg(feature = "agent")]
    {
        assert_eq!(ViewMode::Graph.next(), ViewMode::Agents);
        assert_eq!(ViewMode::Agents.next(), ViewMode::Types);
    }
    #[cfg(not(feature = "agent"))]
    assert_eq!(ViewMode::Graph.next(), ViewMode::Types);
}

#[test]
fn test_backtick_cycles_mode() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.view_mode, ViewMode::Types);

    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Filters);

    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Metrics);
}

#[test]
fn test_types_mode_navigation_unchanged() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.view_mode, ViewMode::Types);

    // j moves selected doc down
    assert_eq!(app.selected_doc, 0);
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 1);
    assert_eq!(app.view_mode, ViewMode::Types);

    // k moves selected doc up
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 0);
    assert_eq!(app.view_mode, ViewMode::Types);

    // l switches type
    let before_type = app.selected_type;
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_ne!(app.selected_type, before_type);
    assert_eq!(app.view_mode, ViewMode::Types);

    // h switches type back
    app.handle_key(KeyCode::Char('h'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_type, before_type);
    assert_eq!(app.view_mode, ViewMode::Types);

    // Enter toggles fullscreen
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(app.fullscreen_doc);
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());

    // Tab toggles preview tab
    assert_eq!(app.preview_tab, PreviewTab::Preview);
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.preview_tab, PreviewTab::Relations);
    assert_eq!(app.view_mode, ViewMode::Types);
}

#[test]
fn test_backtick_ignored_in_modal_states() {
    let (fixture, mut app) = setup_app_with_docs();

    // Search mode
    app.enter_search();
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());

    // Fullscreen mode
    app.enter_fullscreen();
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());

    // Create form mode
    app.open_create_form();
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());

    // Delete confirm mode
    app.open_delete_confirm();
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());
}
