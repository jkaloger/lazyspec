mod common;

use common::TestFixture;
use lazyspec::tui::app::App;

fn setup_app_with_rfc(title: &str) -> (TestFixture, App) {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", title, "draft");
    let store = fixture.store();
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());
    (fixture, app)
}

// AC1: open_delete_confirm populates from selected doc
#[test]
fn test_open_delete_populates_from_selected_doc() {
    let (_fixture, mut app) = setup_app_with_rfc("Delete Me");

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
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-target.md", "Target RFC", "draft");
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        r#"---
title: "Impl Story"
type: story
status: draft
author: "tester"
date: 2026-03-05
tags: []
related:
  - implements: "docs/rfcs/RFC-001-target.md"
---

## Summary
Impl Story body.
"#,
    );

    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());

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
    let (_fixture, mut app) = setup_app_with_rfc("Lonely RFC");

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_delete_confirm();

    assert!(app.delete_confirm.active);
    assert!(app.delete_confirm.references.is_empty());
}

// AC4: confirm_delete removes file from disk and store
#[test]
fn test_confirm_delete_removes_file() {
    let (fixture, mut app) = setup_app_with_rfc("Doomed RFC");
    let root = fixture.root();

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
    let (fixture, mut app) = setup_app_with_rfc("Safe RFC");
    let root = fixture.root();

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
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-first.md", "First RFC", "draft");
    fixture.write_rfc("RFC-002-second.md", "Second RFC", "draft");

    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());

    app.selected_type = 0;
    app.selected_doc = 1; // second RFC (sorted by path)
    app.open_delete_confirm();
    app.confirm_delete(fixture.root()).unwrap();

    assert_eq!(app.selected_doc, 0);
    assert_eq!(app.docs_for_current_type().len(), 1);
}

// AC7: open on empty list is a no-op
#[test]
fn test_open_delete_empty_list_noop() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks());

    app.selected_type = 0;
    app.open_delete_confirm();

    assert!(!app.delete_confirm.active);
}
