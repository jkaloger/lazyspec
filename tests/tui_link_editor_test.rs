mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::state::{App, PreviewTab};

fn setup_app_with_rfc(title: &str, status: &str) -> (TestFixture, App) {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", title, status);
    let store = fixture.store();
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));
    (fixture, app)
}

// AC1: pressing 'r' on Relations tab opens the link editor
#[test]
fn test_open_link_editor_on_relations_tab() {
    let (_fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    assert!(app.link_editor.active);
    assert_eq!(
        app.link_editor.doc_path,
        std::path::PathBuf::from("docs/rfcs/RFC-001-test.md")
    );
    assert_eq!(app.link_editor.rel_type_index, 0);
    assert_eq!(app.link_editor.query, "");
    assert_eq!(app.link_editor.selected, 0);
}

// AC9: pressing 'r' with no document selected does not open
#[test]
fn test_open_link_editor_no_doc_selected_noop() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    assert!(!app.link_editor.active);
}

// AC6: pressing Esc closes the link editor without changes
#[test]
fn test_close_link_editor_resets_state() {
    let (_fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();
    assert!(app.link_editor.active);

    app.close_link_editor();

    assert!(!app.link_editor.active);
    assert_eq!(app.link_editor.doc_path, std::path::PathBuf::new());
    assert_eq!(app.link_editor.query, "");
    assert!(app.link_editor.results.is_empty());
}

// AC6: Esc key dispatches to close_link_editor via handle_key
#[test]
fn test_esc_key_closes_link_editor() {
    let (fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();
    assert!(app.link_editor.active);

    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert!(!app.link_editor.active);
}

// AC1: 'r' key opens link editor via handle_key when on Relations tab
#[test]
fn test_r_key_opens_link_editor_on_relations_tab() {
    let (fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;

    app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert!(app.link_editor.active);
}

// AC1: 'r' key does NOT open link editor when on Preview tab
#[test]
fn test_r_key_noop_on_preview_tab() {
    let (fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Preview;

    app.handle_key(KeyCode::Char('r'), KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert!(!app.link_editor.active);
}

// open_link_editor populates results excluding self
#[test]
fn test_open_link_editor_results_exclude_self() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source RFC", "draft");
    fixture.write_rfc("RFC-002-target.md", "Target RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    assert!(app.link_editor.active);
    assert!(!app.link_editor.results.contains(&app.link_editor.doc_path));
    assert!(!app.link_editor.results.is_empty());
}

// link editor intercepts keys (does not propagate to normal handler)
#[test]
fn test_link_editor_intercepts_keys() {
    let (fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // 'q' normally quits, but link editor should intercept it
    let should_quit_before = app.should_quit;
    app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.should_quit, should_quit_before);
}

// AC2: typing characters filters results in real time
#[test]
fn test_typing_filters_results() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha Feature", "draft");
    fixture.write_rfc("RFC-002-beta.md", "Beta Feature", "draft");
    fixture.write_story("STORY-001-gamma.md", "Gamma Story", "draft", None);
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    // Select RFC-001 as the source document
    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // All non-self docs should appear initially
    let initial_count = app.link_editor.results.len();
    assert!(initial_count >= 2);

    // Type "beta" to filter
    for c in "beta".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }

    assert_eq!(app.link_editor.query, "beta");
    assert_eq!(app.link_editor.results.len(), 1);
    assert_eq!(
        app.link_editor.results[0],
        std::path::PathBuf::from("docs/rfcs/RFC-002-beta.md")
    );
}

// AC2: backspace removes characters and re-filters
#[test]
fn test_backspace_updates_filter() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha Feature", "draft");
    fixture.write_rfc("RFC-002-beta.md", "Beta Feature", "draft");
    fixture.write_rfc("RFC-003-gamma.md", "Gamma Feature", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Should have 2 non-self results initially
    assert_eq!(app.link_editor.results.len(), 2);

    // Type "beta" to filter to 1
    for c in "beta".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }
    assert_eq!(app.link_editor.results.len(), 1);

    // Backspace all to clear filter
    for _ in 0..4 {
        app.handle_key(KeyCode::Backspace, KeyModifiers::NONE, fixture.root(), &fixture.config());
    }
    assert_eq!(app.link_editor.query, "");
    // All non-self docs visible again
    assert_eq!(app.link_editor.results.len(), 2);
}

// AC2: search is case-insensitive
#[test]
fn test_search_case_insensitive() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-002-target.md", "Target Doc", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Search with uppercase should still match
    for c in "TARGET".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }
    assert_eq!(app.link_editor.results.len(), 1);
}

// AC3: display format is TYPE-NNN: Title
#[test]
fn test_display_format_type_nnn_colon_title() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-028-doc-ref-ergonomics.md", "Document Reference Ergonomics", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Verify the display string can be constructed from the store
    let result_path = &app.link_editor.results[0];
    let doc = app.store.get(result_path).unwrap();
    let display = format!("{}: {}", doc.id.to_uppercase(), doc.title);
    assert_eq!(display, "RFC-028: Document Reference Ergonomics");
}

// AC10: self-link excluded from search results
#[test]
fn test_self_excluded_after_search() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-002-target.md", "Target Doc", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    let self_path = app.link_editor.doc_path.clone();

    // Type part of the self-doc's ID to try to match it
    for c in "RFC-001".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }

    // Self should never appear in results
    assert!(!app.link_editor.results.contains(&self_path));
}

// j/k navigation changes selected index
#[test]
fn test_jk_navigation() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-002-alpha.md", "Alpha", "draft");
    fixture.write_rfc("RFC-003-beta.md", "Beta", "draft");
    fixture.write_rfc("RFC-004-gamma.md", "Gamma", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    assert_eq!(app.link_editor.selected, 0);

    // j moves down
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 1);

    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 2);

    // k moves up
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 1);

    // k at 0 stays at 0
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 0);
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 0);
}

// Down/Up arrow navigation
#[test]
fn test_arrow_navigation() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-002-alpha.md", "Alpha", "draft");
    fixture.write_rfc("RFC-003-beta.md", "Beta", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    app.handle_key(KeyCode::Down, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 1);

    app.handle_key(KeyCode::Up, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 0);
}

// j/k clamps to bounds
#[test]
fn test_navigation_clamps_to_bounds() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-002-only.md", "Only Target", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Only 1 result, j should not go past 0
    assert_eq!(app.link_editor.results.len(), 1);
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 0);
}

// selected clamps when filtering reduces results
#[test]
fn test_selected_clamps_on_filter() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source Doc", "draft");
    fixture.write_rfc("RFC-002-alpha.md", "Alpha", "draft");
    fixture.write_rfc("RFC-003-beta.md", "Beta", "draft");
    fixture.write_rfc("RFC-004-gamma.md", "Gamma", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Navigate to last item
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.selected, 2);

    // Now filter to 1 result, selected should clamp
    for c in "alpha".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }
    assert_eq!(app.link_editor.results.len(), 1);
    assert_eq!(app.link_editor.selected, 0);
}

// results are sorted by display string
#[test]
fn test_results_sorted_by_display_string() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-003-charlie.md", "Charlie", "draft");
    fixture.write_rfc("RFC-001-alpha.md", "Alpha", "draft");
    fixture.write_rfc("RFC-002-beta.md", "Beta", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    // Select RFC-003 as source
    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Results should be sorted: RFC-001, RFC-002 (RFC-003 excluded as self)
    // or if RFC-001 is selected: RFC-002, RFC-003
    // Regardless, verify sorted order
    let labels: Vec<String> = app
        .link_editor
        .results
        .iter()
        .map(|p| {
            let doc = app.store.get(p).unwrap();
            format!("{}: {}", doc.id.to_uppercase(), doc.title)
        })
        .collect();

    let mut sorted = labels.clone();
    sorted.sort();
    assert_eq!(labels, sorted);
}

// AC4: Tab cycles through relationship types
#[test]
fn test_tab_cycles_rel_type() {
    let (fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    assert_eq!(app.link_editor.rel_type_index, 0); // implements

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.rel_type_index, 1); // supersedes

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.rel_type_index, 2); // blocks

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.rel_type_index, 3); // related-to

    // Wraps around
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.rel_type_index, 0); // back to implements
}

// AC5: Enter with a selected doc writes the link and closes overlay
#[test]
fn test_enter_creates_link_and_closes() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source RFC", "draft");
    fixture.write_rfc("RFC-002-target.md", "Target RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    assert!(app.link_editor.active);
    assert!(!app.link_editor.results.is_empty());

    // Press Enter to confirm link
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());

    // Overlay should be closed
    assert!(!app.link_editor.active);

    // Verify the link was written by reloading the store
    let store = fixture.store();
    let source = store.get(&std::path::PathBuf::from("docs/rfcs/RFC-001-source.md")).unwrap();
    assert!(!source.related.is_empty(), "source should have a relation after Enter");
    assert_eq!(source.related[0].rel_type, lazyspec::engine::document::RelationType::Implements);
    assert_eq!(source.related[0].target, "RFC-002");
}

// AC5: Enter with empty results is a no-op
#[test]
fn test_enter_with_empty_results_noop() {
    let (_fixture, mut app) = setup_app_with_rfc("Test RFC", "draft");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Only one doc in store, so results should be empty (self is excluded)
    assert!(app.link_editor.results.is_empty());

    // Enter should do nothing -- overlay stays open
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, _fixture.root(), &_fixture.config());

    assert!(app.link_editor.active);
}

// AC5: Enter with non-default rel type writes the correct type
#[test]
fn test_enter_with_tab_writes_correct_rel_type() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-source.md", "Source RFC", "draft");
    fixture.write_rfc("RFC-002-target.md", "Target RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;
    app.open_link_editor();

    // Tab twice to get to "blocks"
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.link_editor.rel_type_index, 2);

    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert!(!app.link_editor.active);

    let store = fixture.store();
    let source = store.get(&std::path::PathBuf::from("docs/rfcs/RFC-001-source.md")).unwrap();
    assert_eq!(source.related[0].rel_type, lazyspec::engine::document::RelationType::Blocks);
}
