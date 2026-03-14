mod common;

use common::TestFixture;
use lazyspec::tui::app::App;

fn setup_app() -> (TestFixture, App) {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let app = App::new(store, &fixture.config(), lazyspec::tui::terminal_caps::TerminalImageProtocol::Unsupported);
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
    fixture.write_doc(
        "docs/adrs/001-first.md",
        "---\ntitle: First ADR\ntype: adr\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );

    let store = fixture.store();
    let app = App::new(store, &fixture.config(), lazyspec::tui::terminal_caps::TerminalImageProtocol::Unsupported);
    (fixture, app)
}

#[test]
fn test_move_type_next() {
    let (_fixture, mut app) = setup_app();
    assert_eq!(app.selected_type, 0);

    app.move_type_next();
    assert_eq!(app.selected_type, 1);
    assert_eq!(app.selected_doc, 0);
}

#[test]
fn test_move_type_next_resets_selected_doc() {
    let (_fixture, mut app) = setup_app_with_docs();
    app.selected_doc = 2;

    app.move_type_next();
    assert_eq!(app.selected_type, 1);
    assert_eq!(app.selected_doc, 0);
}

#[test]
fn test_move_type_prev() {
    let (_fixture, mut app) = setup_app();
    app.selected_type = 2;

    app.move_type_prev();
    assert_eq!(app.selected_type, 1);
    assert_eq!(app.selected_doc, 0);
}

#[test]
fn test_move_type_prev_resets_selected_doc() {
    let (_fixture, mut app) = setup_app_with_docs();
    app.selected_type = 1;
    app.selected_doc = 0;

    app.move_type_prev();
    assert_eq!(app.selected_type, 0);
    assert_eq!(app.selected_doc, 0);
}

#[test]
fn test_move_type_next_clamps_at_end() {
    let (_fixture, mut app) = setup_app();
    app.selected_type = app.doc_types.len() - 1;

    app.move_type_next();
    assert_eq!(app.selected_type, app.doc_types.len() - 1);
}

#[test]
fn test_move_type_prev_clamps_at_start() {
    let (_fixture, mut app) = setup_app();
    assert_eq!(app.selected_type, 0);

    app.move_type_prev();
    assert_eq!(app.selected_type, 0);
}

#[test]
fn test_move_down_always_navigates_docs() {
    let (_fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.selected_type, 0);
    assert_eq!(app.selected_doc, 0);

    app.move_down();
    assert_eq!(app.selected_doc, 1);
    assert_eq!(app.selected_type, 0);

    app.move_down();
    assert_eq!(app.selected_doc, 2);
    assert_eq!(app.selected_type, 0);
}

#[test]
fn test_move_up_always_navigates_docs() {
    let (_fixture, mut app) = setup_app_with_docs();
    app.selected_doc = 2;

    app.move_up();
    assert_eq!(app.selected_doc, 1);
    assert_eq!(app.selected_type, 0);

    app.move_up();
    assert_eq!(app.selected_doc, 0);
    assert_eq!(app.selected_type, 0);
}
