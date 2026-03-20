mod common;

use common::TestFixture;
use lazyspec::tui::app::App;
use std::path::PathBuf;

fn setup_app_with_docs() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/001-alpha.md",
        "---\ntitle: Alpha RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );
    fixture.write_doc(
        "docs/rfcs/002-beta.md",
        "---\ntitle: Beta RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-02\ntags: []\n---\nBody\n",
    );
    fixture.write_doc(
        "docs/rfcs/003-gamma.md",
        "---\ntitle: Gamma RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-03\ntags: []\n---\nBody\n",
    );
    fixture.write_doc(
        "docs/stories/001-unique-story.md",
        "---\ntitle: Unique Story\ntype: story\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );

    let store = fixture.store();
    let config = fixture.config();
    let mut app = App::new(store, &config, ratatui_image::picker::Picker::halfblocks());
    app.refresh_validation(&config);
    (fixture, app)
}

#[test]
fn test_enter_search() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();

    assert!(app.search_mode);
    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_selected, 0);
}

#[test]
fn test_exit_search() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("alpha");
    app.update_search();
    assert!(!app.search_results.is_empty());

    app.exit_search();

    assert!(!app.search_mode);
    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_selected, 0);
}

#[test]
fn test_update_search_filters_by_title() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("unique");
    app.update_search();

    assert_eq!(app.search_results.len(), 1);
    assert!(app.search_results[0]
        .to_string_lossy()
        .contains("001-unique-story.md"));
}

#[test]
fn test_update_search_empty_query_clears_results() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("alpha");
    app.update_search();
    assert!(!app.search_results.is_empty());

    app.search_query.clear();
    app.update_search();

    assert!(app.search_results.is_empty());
}

#[test]
fn test_update_search_resets_selected() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("rfc");
    app.update_search();
    assert!(app.search_results.len() >= 2);
    app.search_selected = 1;

    app.search_query.clear();
    app.search_query.push_str("alpha");
    app.update_search();

    assert_eq!(app.search_selected, 0);
}

#[test]
fn test_select_search_result_navigates_to_doc() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("unique");
    app.update_search();
    assert_eq!(app.search_results.len(), 1);

    app.select_search_result();

    // Story is at index 1 in doc_types (Rfc=0, Story=1, Iteration=2, Adr=3)
    assert_eq!(app.selected_type, 1);
    assert_eq!(app.selected_doc, 0);
    assert!(!app.search_mode);
}

#[test]
fn test_select_search_result_with_no_results() {
    let (_fixture, mut app) = setup_app_with_docs();

    let original_type = app.selected_type;
    let original_doc = app.selected_doc;

    app.enter_search();
    assert!(app.search_results.is_empty());

    app.select_search_result();

    assert_eq!(app.selected_type, original_type);
    assert_eq!(app.selected_doc, original_doc);
}

#[test]
fn test_search_move_down() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.search_results = vec![
        PathBuf::from("a.md"),
        PathBuf::from("b.md"),
        PathBuf::from("c.md"),
    ];
    app.search_selected = 0;

    app.search_move_down();

    assert_eq!(app.search_selected, 1);
}

#[test]
fn test_search_move_down_clamps() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.search_results = vec![
        PathBuf::from("a.md"),
        PathBuf::from("b.md"),
        PathBuf::from("c.md"),
    ];
    app.search_selected = 2;

    app.search_move_down();

    assert_eq!(app.search_selected, 2);
}

#[test]
fn test_search_move_up() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.search_results = vec![
        PathBuf::from("a.md"),
        PathBuf::from("b.md"),
        PathBuf::from("c.md"),
    ];
    app.search_selected = 2;

    app.search_move_up();

    assert_eq!(app.search_selected, 1);
}

#[test]
fn test_search_move_up_clamps() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.search_results = vec![
        PathBuf::from("a.md"),
        PathBuf::from("b.md"),
        PathBuf::from("c.md"),
    ];
    app.search_selected = 0;

    app.search_move_up();

    assert_eq!(app.search_selected, 0);
}
