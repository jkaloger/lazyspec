mod common;

use common::TestFixture;
use lazyspec::engine::config::Config;
use lazyspec::engine::document::{DocMeta, DocType};
use lazyspec::tui::state::App;
use std::fs;

fn setup_app_with_rfc() -> (TestFixture, App, Config) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-existing.md",
        r#"---
title: "Existing RFC"
type: rfc
status: accepted
author: "tester"
date: 2026-03-05
tags: []
related: []
---

## Summary
An existing RFC.
"#,
    );

    let config = fixture.config();
    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    (fixture, app, config)
}

// AC1: Submit creates document on disk
#[test]
fn test_submit_creates_document() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    app.open_create_form();
    for c in "My New RFC".chars() {
        app.form_type_char(c);
    }

    let result = app.submit_create_form(root, &config);
    assert!(result.is_ok());

    let rfcs: Vec<_> = fs::read_dir(root.join("docs/rfcs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("my-new-rfc"))
        .collect();
    assert_eq!(rfcs.len(), 1);

    let content = fs::read_to_string(rfcs[0].path()).unwrap();
    assert!(content.contains("title: \"My New RFC\""));
    assert!(content.contains("type: rfc"));
    assert!(content.contains("status: draft"));
}

// AC1: Submit with different doc type
#[test]
fn test_submit_creates_correct_type() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    // Select Story type (index 1)
    app.selected_type = 1;
    app.open_create_form();
    for c in "My Story".chars() {
        app.form_type_char(c);
    }

    let result = app.submit_create_form(root, &config);
    assert!(result.is_ok());

    let stories: Vec<_> = fs::read_dir(root.join("docs/stories"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(stories.len(), 1);
}

// AC2: Tags are applied
#[test]
fn test_submit_applies_tags() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    app.open_create_form();
    for c in "Tagged Doc".chars() {
        app.form_type_char(c);
    }
    app.form_next_field(); // author
    app.form_next_field(); // tags
    for c in "api, auth, v2".chars() {
        app.form_type_char(c);
    }

    app.submit_create_form(root, &config).unwrap();

    let rfcs: Vec<_> = fs::read_dir(root.join("docs/rfcs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("tagged-doc"))
        .collect();
    let content = fs::read_to_string(rfcs[0].path()).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.tags, vec!["api", "auth", "v2"]);
}

// AC3: Relations are applied with type prefix
#[test]
fn test_submit_applies_relations() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    // Create a story that implements RFC-001
    app.selected_type = 1; // Story
    app.open_create_form();
    for c in "Linked Story".chars() {
        app.form_type_char(c);
    }
    app.form_next_field(); // author
    app.form_next_field(); // tags
    app.form_next_field(); // related
    for c in "implements:RFC-001".chars() {
        app.form_type_char(c);
    }

    app.submit_create_form(root, &config).unwrap();

    let stories: Vec<_> = fs::read_dir(root.join("docs/stories"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let content = fs::read_to_string(stories[0].path()).unwrap();
    assert!(content.contains("implements"));
    assert!(content.contains("RFC-001"));
}

// AC4: Relation without prefix defaults to related-to
#[test]
fn test_submit_relation_defaults_to_related_to() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    app.selected_type = 1; // Story
    app.open_create_form();
    for c in "Default Rel".chars() {
        app.form_type_char(c);
    }
    app.form_next_field(); // author
    app.form_next_field(); // tags
    app.form_next_field(); // related
    for c in "RFC-001".chars() {
        app.form_type_char(c);
    }

    app.submit_create_form(root, &config).unwrap();

    let stories: Vec<_> = fs::read_dir(root.join("docs/stories"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let content = fs::read_to_string(stories[0].path()).unwrap();
    assert!(content.contains("related-to"));
    assert!(content.contains("RFC-001"));
}

// AC5: Empty title shows error, no file created
#[test]
fn test_submit_empty_title_shows_error() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    app.open_create_form();
    // Don't type anything in title

    let result = app.submit_create_form(root, &config);
    assert!(result.is_err() || app.create_form.error.is_some());
    assert!(app.create_form.active);

    // Count files - should only be the pre-existing RFC-001
    let rfcs: Vec<_> = fs::read_dir(root.join("docs/rfcs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(rfcs.len(), 1);
}

// AC6: Invalid relation shorthand shows error
#[test]
fn test_submit_invalid_relation_shows_error() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    app.open_create_form();
    for c in "Bad Rel Doc".chars() {
        app.form_type_char(c);
    }
    app.form_next_field(); // author
    app.form_next_field(); // tags
    app.form_next_field(); // related
    for c in "RFC-999".chars() {
        app.form_type_char(c);
    }

    let result = app.submit_create_form(root, &config);
    assert!(result.is_err() || app.create_form.error.is_some());
    assert!(app.create_form.active);

    // No new file created beyond existing RFC-001
    let rfcs: Vec<_> = fs::read_dir(root.join("docs/rfcs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(rfcs.len(), 1);
}

// AC7: Navigate to newly created document
#[test]
fn test_submit_navigates_to_new_doc() {
    let (fixture, mut app, config) = setup_app_with_rfc();
    let root = fixture.root();

    // Start on Story type
    app.selected_type = 1;
    app.open_create_form();
    for c in "Navigate Test".chars() {
        app.form_type_char(c);
    }

    app.submit_create_form(root, &config).unwrap();

    // Form should be closed
    assert!(!app.create_form.active);
    // Should be on Story type
    assert_eq!(*app.current_type(), DocType::new(DocType::STORY));
    // Should have a doc selected (the new one)
    assert!(app.selected_doc_meta().is_some());
}
