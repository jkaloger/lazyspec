use crate::common::TestFixture;
use lazyspec::tui::state::App;
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
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    (fixture, app)
}

/// Search is async in production (BUG-011): `update_search` only dispatches to
/// a worker. Drive the same sequence minus the thread: dispatch, run the
/// corpus search inline the way the worker does, and apply under the live
/// generation.
fn run_search(app: &mut App) {
    app.update_search();
    if app.search_query.is_empty() {
        return;
    }
    assert!(
        app.search_pending,
        "non-empty query must mark search in flight"
    );
    let results: Vec<PathBuf> = app
        .store
        .search_corpus(&*app.fs)
        .search(&app.search_query)
        .into_iter()
        .map(|r| r.path)
        .collect();
    app.apply_search_results(app.search_generation, results);
    assert!(
        !app.search_pending,
        "applied results must clear the pending flag"
    );
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
    run_search(&mut app);
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
    run_search(&mut app);

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
    run_search(&mut app);
    assert!(!app.search_results.is_empty());

    app.search_query.clear();
    app.update_search();

    assert!(app.search_results.is_empty());
    assert!(
        !app.search_pending,
        "empty query clears synchronously without dispatching a search"
    );
}

#[test]
fn test_update_search_resets_selected() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("rfc");
    run_search(&mut app);
    assert!(app.search_results.len() >= 2);
    app.search_selected = 1;

    app.search_query.clear();
    app.search_query.push_str("alpha");
    run_search(&mut app);

    assert_eq!(app.search_selected, 0);
}

#[test]
fn test_select_search_result_navigates_to_doc() {
    let (_fixture, mut app) = setup_app_with_docs();

    app.enter_search();
    app.search_query.push_str("unique");
    run_search(&mut app);
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
