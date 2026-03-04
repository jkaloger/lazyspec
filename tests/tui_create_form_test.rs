use lazyspec::engine::config::Config;
use lazyspec::engine::document::DocType;
use lazyspec::engine::store::Store;
use lazyspec::tui::app::{App, FormField};
use std::fs;
use tempfile::TempDir;

fn setup_app() -> (TempDir, App) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::create_dir_all(root.join("docs/stories")).unwrap();
    fs::create_dir_all(root.join("docs/iterations")).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let app = App::new(store);
    (dir, app)
}

// AC1: Open create form with current type
#[test]
fn test_create_form_opens_with_current_type() {
    let (_dir, mut app) = setup_app();

    // Default selected type is index 0 = Rfc
    assert!(!app.create_form.active);

    app.open_create_form();

    assert!(app.create_form.active);
    assert_eq!(app.create_form.doc_type, DocType::Rfc);
}

#[test]
fn test_create_form_opens_with_selected_type() {
    let (_dir, mut app) = setup_app();

    // Select Story (index 2)
    app.selected_type = 2;
    app.open_create_form();

    assert!(app.create_form.active);
    assert_eq!(app.create_form.doc_type, DocType::Story);
}

// AC2: Initial state - fields present, author pre-filled, title focused
#[test]
fn test_create_form_initial_state() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    assert_eq!(app.create_form.focused_field, FormField::Title);
    assert!(app.create_form.title.is_empty());
    assert!(app.create_form.tags.is_empty());
    assert!(app.create_form.related.is_empty());
}

// AC3: Text input appears in focused field
#[test]
fn test_create_form_text_input() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    app.form_type_char('H');
    app.form_type_char('e');
    app.form_type_char('l');
    app.form_type_char('l');
    app.form_type_char('o');

    assert_eq!(app.create_form.title, "Hello");
}

#[test]
fn test_create_form_text_input_follows_focus() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    // Type in title
    app.form_type_char('A');

    // Move to author, type there
    app.form_next_field();
    app.form_type_char('B');

    // Move to tags, type there
    app.form_next_field();
    app.form_type_char('C');

    // Move to related, type there
    app.form_next_field();
    app.form_type_char('D');

    assert_eq!(app.create_form.title, "A");
    assert!(app.create_form.author.ends_with('B'));
    assert_eq!(app.create_form.tags, "C");
    assert_eq!(app.create_form.related, "D");
}

// AC4: Backspace removes last character
#[test]
fn test_create_form_backspace() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    app.form_type_char('A');
    app.form_type_char('B');
    app.form_type_char('C');
    app.form_backspace();

    assert_eq!(app.create_form.title, "AB");
}

#[test]
fn test_create_form_backspace_on_empty() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    // Should not panic on empty field
    app.form_backspace();
    assert!(app.create_form.title.is_empty());
}

// AC5: Tab cycles fields forward, Shift+Tab cycles backward
#[test]
fn test_create_form_tab_navigation() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    assert_eq!(app.create_form.focused_field, FormField::Title);

    app.form_next_field();
    assert_eq!(app.create_form.focused_field, FormField::Author);

    app.form_next_field();
    assert_eq!(app.create_form.focused_field, FormField::Tags);

    app.form_next_field();
    assert_eq!(app.create_form.focused_field, FormField::Related);

    // Wraps around
    app.form_next_field();
    assert_eq!(app.create_form.focused_field, FormField::Title);
}

#[test]
fn test_create_form_shift_tab_navigation() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    assert_eq!(app.create_form.focused_field, FormField::Title);

    // Wraps backward from Title to Related
    app.form_prev_field();
    assert_eq!(app.create_form.focused_field, FormField::Related);

    app.form_prev_field();
    assert_eq!(app.create_form.focused_field, FormField::Tags);

    app.form_prev_field();
    assert_eq!(app.create_form.focused_field, FormField::Author);

    app.form_prev_field();
    assert_eq!(app.create_form.focused_field, FormField::Title);
}

// AC6: Cancel closes form and discards input
#[test]
fn test_create_form_cancel() {
    let (_dir, mut app) = setup_app();
    app.open_create_form();

    app.form_type_char('T');
    app.form_type_char('e');
    app.form_type_char('s');
    app.form_type_char('t');

    app.close_create_form();

    assert!(!app.create_form.active);
    assert!(app.create_form.title.is_empty());
    assert!(app.create_form.tags.is_empty());
    assert!(app.create_form.related.is_empty());
}
