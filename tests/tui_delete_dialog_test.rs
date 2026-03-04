use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use lazyspec::tui::app::App;
use std::fs;
use tempfile::TempDir;

fn setup_dirs(root: &std::path::Path) {
    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::create_dir_all(root.join("docs/stories")).unwrap();
    fs::create_dir_all(root.join("docs/iterations")).unwrap();
}

fn write_rfc(root: &std::path::Path, filename: &str, title: &str) {
    fs::write(
        root.join(format!("docs/rfcs/{}", filename)),
        format!(
            r#"---
title: "{title}"
type: rfc
status: draft
author: "tester"
date: 2026-03-05
tags: []
related: []
---

## Summary
{title} body.
"#
        ),
    )
    .unwrap();
}

fn write_story_implementing(root: &std::path::Path, filename: &str, title: &str, rfc_path: &str) {
    fs::write(
        root.join(format!("docs/stories/{}", filename)),
        format!(
            r#"---
title: "{title}"
type: story
status: draft
author: "tester"
date: 2026-03-05
tags: []
related:
  - implements: "{rfc_path}"
---

## Summary
{title} body.
"#
        ),
    )
    .unwrap();
}

fn setup_app_with_rfc(title: &str) -> (TempDir, App) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    setup_dirs(root);
    write_rfc(root, "RFC-001-test.md", title);
    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let app = App::new(store);
    (dir, app)
}

// AC1: open_delete_confirm populates from selected doc
#[test]
fn test_open_delete_populates_from_selected_doc() {
    let (_dir, mut app) = setup_app_with_rfc("Delete Me");

    app.selected_type = 0; // RFC
    app.selected_doc = 0;
    app.open_delete_confirm();

    assert!(app.delete_confirm.active);
    assert_eq!(app.delete_confirm.doc_title, "Delete Me");
    assert_eq!(
        app.delete_confirm.doc_path,
        std::path::PathBuf::from("docs/rfcs/RFC-001-test.md")
    );
}

// AC2: open_delete_confirm collects reverse references
#[test]
fn test_open_delete_collects_references() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    setup_dirs(root);
    write_rfc(root, "RFC-001-target.md", "Target RFC");
    write_story_implementing(
        root,
        "STORY-001-impl.md",
        "Impl Story",
        "docs/rfcs/RFC-001-target.md",
    );

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let mut app = App::new(store);

    app.selected_type = 0; // RFC
    app.selected_doc = 0;
    app.open_delete_confirm();

    assert!(app.delete_confirm.active);
    assert_eq!(app.delete_confirm.references.len(), 1);
    let (rel_type, ref_path) = &app.delete_confirm.references[0];
    assert_eq!(rel_type, "implements");
    assert_eq!(
        *ref_path,
        std::path::PathBuf::from("docs/stories/STORY-001-impl.md")
    );
}

// AC3: open_delete_confirm with no references
#[test]
fn test_open_delete_no_references() {
    let (_dir, mut app) = setup_app_with_rfc("Lonely RFC");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_delete_confirm();

    assert!(app.delete_confirm.active);
    assert!(app.delete_confirm.references.is_empty());
}

// AC4: confirm_delete removes file from disk and store
#[test]
fn test_confirm_delete_removes_file() {
    let (dir, mut app) = setup_app_with_rfc("Doomed RFC");
    let root = dir.path();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_delete_confirm();
    app.confirm_delete(root).unwrap();

    assert!(!root.join("docs/rfcs/RFC-001-test.md").exists());
    assert!(app
        .store
        .get(std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .is_none());
    assert!(!app.delete_confirm.active);
}

// AC5: cancel preserves file
#[test]
fn test_cancel_delete_preserves_file() {
    let (dir, mut app) = setup_app_with_rfc("Safe RFC");
    let root = dir.path();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_delete_confirm();
    app.close_delete_confirm();

    assert!(!app.delete_confirm.active);
    assert!(root.join("docs/rfcs/RFC-001-test.md").exists());
}

// AC6: selection adjusts after deleting last item
#[test]
fn test_selection_adjusts_after_delete_last() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    setup_dirs(root);
    write_rfc(root, "RFC-001-first.md", "First RFC");
    write_rfc(root, "RFC-002-second.md", "Second RFC");

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let mut app = App::new(store);

    app.selected_type = 0;
    app.selected_doc = 1; // second RFC (sorted by path)
    app.open_delete_confirm();
    app.confirm_delete(root).unwrap();

    assert_eq!(app.selected_doc, 0);
    assert_eq!(app.docs_for_current_type().len(), 1);
}

// AC7: open on empty list is a no-op
#[test]
fn test_open_delete_empty_list_noop() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    setup_dirs(root);

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let mut app = App::new(store);

    app.selected_type = 0;
    app.open_delete_confirm();

    assert!(!app.delete_confirm.active);
}
