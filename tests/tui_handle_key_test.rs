mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::app::{App, PreviewTab};
use std::path::PathBuf;

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
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));
    (fixture, app)
}

// --- Normal mode ---

#[test]
fn test_handle_key_quit() {
    let (fixture, mut app) = setup_app_with_docs();
    app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(app.should_quit);
}

#[test]
fn test_handle_key_ctrl_c_quit() {
    let (fixture, mut app) = setup_app_with_docs();
    app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL, fixture.root(), &fixture.config());
    assert!(app.should_quit);
}

#[test]
fn test_handle_key_help() {
    let (fixture, mut app) = setup_app_with_docs();
    app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(app.show_help);
}

#[test]
fn test_handle_key_dismiss_help() {
    let (fixture, mut app) = setup_app_with_docs();
    app.show_help = true;
    app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(!app.show_help);
}

#[test]
fn test_handle_key_navigation_j() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.selected_doc, 0);
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 1);
}

#[test]
fn test_handle_key_navigation_k() {
    let (fixture, mut app) = setup_app_with_docs();
    app.selected_doc = 1;
    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_doc, 0);
}

#[test]
fn test_handle_key_type_switch() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.selected_type, 0);
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.selected_type, 1);
}

#[test]
fn test_handle_key_enter_fullscreen() {
    let (fixture, mut app) = setup_app_with_docs();
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(app.fullscreen_doc);
}

#[test]
fn test_handle_key_enter_search() {
    let (fixture, mut app) = setup_app_with_docs();
    app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(app.search_mode);
}

#[test]
fn test_handle_key_tab_toggles_preview() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.preview_tab, PreviewTab::Preview);
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.preview_tab, PreviewTab::Relations);
}

// --- Search mode ---

#[test]
fn test_handle_key_search_esc() {
    let (fixture, mut app) = setup_app_with_docs();
    app.enter_search();
    assert!(app.search_mode);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(!app.search_mode);
}

#[test]
fn test_handle_key_search_typing() {
    let (fixture, mut app) = setup_app_with_docs();
    app.enter_search();
    app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.search_query, "a");
}

#[test]
fn test_handle_key_search_backspace() {
    let (fixture, mut app) = setup_app_with_docs();
    app.enter_search();
    app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('b'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.search_query, "ab");
    app.handle_key(KeyCode::Backspace, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.search_query, "a");
}

#[test]
fn test_handle_key_search_ctrl_j() {
    let (fixture, mut app) = setup_app_with_docs();
    app.enter_search();
    app.search_results = vec![PathBuf::from("a"), PathBuf::from("b")];
    assert_eq!(app.search_selected, 0);
    app.handle_key(KeyCode::Char('j'), KeyModifiers::CONTROL, fixture.root(), &fixture.config());
    assert_eq!(app.search_selected, 1);
}

// --- Fullscreen mode ---

#[test]
fn test_handle_key_fullscreen_esc() {
    let (fixture, mut app) = setup_app_with_docs();
    app.enter_fullscreen();
    assert!(app.fullscreen_doc);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(!app.fullscreen_doc);
}

#[test]
fn test_handle_key_fullscreen_scroll() {
    let (fixture, mut app) = setup_app_with_docs();
    app.enter_fullscreen();
    assert_eq!(app.scroll_offset, 0);
    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.scroll_offset, 1);
}

// --- Create form mode ---

#[test]
fn test_handle_key_create_form_esc() {
    let (fixture, mut app) = setup_app_with_docs();
    app.open_create_form();
    assert!(app.create_form.active);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(!app.create_form.active);
}

#[test]
fn test_handle_key_create_form_typing() {
    let (fixture, mut app) = setup_app_with_docs();
    app.open_create_form();
    app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.create_form.title, "a");
}

// --- Delete confirm mode ---

#[test]
fn test_handle_key_delete_confirm_esc() {
    let (fixture, mut app) = setup_app_with_docs();
    app.open_delete_confirm();
    assert!(app.delete_confirm.active);
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert!(!app.delete_confirm.active);
}
