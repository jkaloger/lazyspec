mod common;

use common::TestFixture;
use lazyspec::tui::app::App;

fn setup_app() -> (TestFixture, App) {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let app = App::new(store);
    (fixture, app)
}

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
    fixture.write_doc(
        "docs/rfcs/003-third.md",
        "---\ntitle: Third RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-03\ntags: []\n---\nBody\n",
    );

    let store = fixture.store();
    let app = App::new(store);
    (fixture, app)
}

#[test]
fn test_enter_fullscreen_with_doc() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_fullscreen();

    assert!(app.fullscreen_doc);
    assert_eq!(app.scroll_offset, 0);
}

#[test]
fn test_enter_fullscreen_without_doc() {
    let (_fixture, mut app) = setup_app();

    app.enter_fullscreen();

    assert!(!app.fullscreen_doc);
}

#[test]
fn test_exit_fullscreen() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_fullscreen();
    app.scroll_down();
    app.scroll_down();
    app.scroll_down();
    app.exit_fullscreen();

    assert!(!app.fullscreen_doc);
    assert_eq!(app.scroll_offset, 0);
}

#[test]
fn test_scroll_down() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_fullscreen();
    app.scroll_down();
    app.scroll_down();
    app.scroll_down();

    assert_eq!(app.scroll_offset, 3);
}

#[test]
fn test_scroll_up() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_fullscreen();
    app.scroll_offset = 5;
    app.scroll_up();

    assert_eq!(app.scroll_offset, 4);
}

#[test]
fn test_scroll_up_clamps_at_zero() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_fullscreen();
    assert_eq!(app.scroll_offset, 0);

    app.scroll_up();

    assert_eq!(app.scroll_offset, 0);
}

#[test]
fn test_move_to_top() {
    let (_fixture, mut app) = setup_app_with_docs();
    app.selected_doc = 2;

    app.move_to_top();

    assert_eq!(app.selected_doc, 0);
}

#[test]
fn test_move_to_bottom() {
    let (_fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.selected_doc, 0);

    app.move_to_bottom();

    assert_eq!(app.selected_doc, 2);
}

#[test]
fn test_move_to_bottom_empty() {
    let (_fixture, mut app) = setup_app();

    app.move_to_bottom();

    assert_eq!(app.selected_doc, 0);
}
