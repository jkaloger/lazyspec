mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::app::App;

fn setup_app_with_parse_errors() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-valid.md",
        "---\ntitle: Valid RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );

    // Broken docs: missing required frontmatter fields
    fixture.write_doc("docs/rfcs/RFC-002-broken.md", "---\ntitle: Broken\n---\n");
    fixture.write_doc(
        "docs/stories/STORY-001-broken.md",
        "---\ntitle: Also Broken\n---\n",
    );
    fixture.write_doc(
        "docs/adrs/ADR-001-broken.md",
        "---\ntitle: Third Broken\n---\n",
    );

    let store = fixture.store();
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());
    (fixture, app)
}

fn setup_app_no_errors() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-good.md",
        "---\ntitle: Good RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );

    let store = fixture.store();
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());
    (fixture, app)
}

#[test]
fn test_open_warnings_with_errors() {
    let (_fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();

    assert!(app.show_warnings);
    assert_eq!(app.warnings_selected, 0);
}

#[test]
fn test_open_warnings_no_errors_still_opens() {
    let (_fixture, mut app) = setup_app_no_errors();

    app.open_warnings();

    assert!(app.show_warnings);
}

#[test]
fn test_close_warnings() {
    let (_fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    assert!(app.show_warnings);

    app.close_warnings();

    assert!(!app.show_warnings);
    assert_eq!(app.warnings_selected, 0);
}

#[test]
fn test_warnings_move_down() {
    let (_fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    app.warnings_move_down();

    assert_eq!(app.warnings_selected, 1);
}

#[test]
fn test_warnings_move_down_clamps() {
    let (_fixture, mut app) = setup_app_with_parse_errors();
    let n = app.store.parse_errors().len();

    app.open_warnings();
    for _ in 0..n + 5 {
        app.warnings_move_down();
    }

    assert_eq!(app.warnings_selected, n - 1);
}

#[test]
fn test_warnings_move_up() {
    let (_fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    app.warnings_move_down();
    app.warnings_move_down();
    app.warnings_move_up();

    assert_eq!(app.warnings_selected, 1);
}

#[test]
fn test_warnings_move_up_clamps_at_zero() {
    let (_fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    app.warnings_move_up();

    assert_eq!(app.warnings_selected, 0);
}

#[test]
fn test_handle_key_w_toggles_warnings() {
    let (fixture, mut app) = setup_app_with_parse_errors();

    app.handle_key(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert!(app.show_warnings);

    app.handle_key(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert!(!app.show_warnings);
}

#[test]
fn test_handle_key_esc_closes_warnings() {
    let (fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    assert!(app.show_warnings);

    app.handle_key(
        KeyCode::Esc,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(!app.show_warnings);
}

#[test]
fn test_handle_key_q_closes_warnings() {
    let (fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    assert!(app.show_warnings);

    app.handle_key(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(!app.show_warnings);
}

#[test]
fn test_handle_key_f_sets_fix_request() {
    let (fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    assert!(app.show_warnings);

    app.handle_key(
        KeyCode::Char('f'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(app.fix_request);
    assert!(app.show_warnings);
}

#[test]
fn test_warnings_intercepts_keys() {
    let (fixture, mut app) = setup_app_with_parse_errors();

    app.open_warnings();
    assert!(app.show_warnings);

    app.handle_key(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert!(!app.should_quit);
}
