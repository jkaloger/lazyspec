mod common;

use lazyspec::engine::config::{NumberingStrategy, TypeDef};
use lazyspec::engine::template;
use std::fs;

fn singleton_type(name: &str, dir: &str, prefix: &str) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        plural: format!("{}s", name),
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: None,
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: Default::default(),
        singleton: true,
        parent_type: None,
    }
}

#[test]
fn create_generates_doc_from_template() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();

    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(
        root.join(".lazyspec/templates/rfc.md"),
        r#"---
title: "{title}"
type: rfc
status: draft
author: "{author}"
date: {date}
tags: []
---

## Summary

TODO: Describe the proposal.
"#,
    )
    .unwrap();

    let config = fixture.config();
    let path =
        lazyspec::cli::create::run(root, &config, &fixture.store(), "rfc", "Event Sourcing", "jkaloger", |_| {}).unwrap();

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("title: \"Event Sourcing\""));
    assert!(content.contains("type: rfc"));
    assert!(content.contains("author: \"jkaloger\""));
}

#[test]
fn create_auto_increments_number() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();

    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(
        root.join(".lazyspec/templates/rfc.md"),
        "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n",
    )
    .unwrap();

    fs::write(root.join("docs/rfcs/RFC-001-old.md"), "").unwrap();

    let config = fixture.config();
    let path = lazyspec::cli::create::run(root, &config, &fixture.store(), "rfc", "New Feature", "a", |_| {}).unwrap();

    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("RFC-002"), "got: {}", filename);
}

#[test]
fn create_with_date_pattern() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();

    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(
        root.join(".lazyspec/templates/rfc.md"),
        "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n",
    )
    .unwrap();

    let mut config = fixture.config();
    config.documents.naming.pattern = "{date}-{title}.md".to_string();

    let path = lazyspec::cli::create::run(root, &config, &fixture.store(), "rfc", "My Feature", "a", |_| {}).unwrap();

    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.ends_with("-my-feature.md"), "got: {}", filename);
}

#[test]
fn create_uses_default_template_when_custom_missing() {
    let fixture = common::TestFixture::new();

    let config = fixture.config();
    let path =
        lazyspec::cli::create::run(fixture.root(), &config, &fixture.store(), "story", "API Design", "jkaloger", |_| {}).unwrap();

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("title: \"API Design\""));
    assert!(content.contains("type: story"));
    assert!(content.contains("status: draft"));
}

#[test]
fn create_story_uses_default_template_with_ac_sections() {
    let fixture = common::TestFixture::new();

    let config = fixture.config();
    let path = lazyspec::cli::create::run(fixture.root(), &config, &fixture.store(), "story", "User Auth", "jkaloger", |_| {}).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("type: story"));
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("**Given**"));
    assert!(content.contains("**When**"));
    assert!(content.contains("**Then**"));
    assert!(content.contains("## Scope"));
}

#[test]
fn create_iteration_uses_default_template() {
    let fixture = common::TestFixture::new();

    let config = fixture.config();
    let path = lazyspec::cli::create::run(fixture.root(), &config, &fixture.store(), "iteration", "Auth Impl 1", "agent", |_| {}).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("type: iteration"));
    assert!(content.contains("## Changes"));
    assert!(content.contains("## Test Plan"));
}

#[test]
fn create_unknown_type_returns_error_with_valid_types() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();
    let result = lazyspec::cli::create::run(fixture.root(), &config, &fixture.store(), "foobar", "Test", "a", |_| {});
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown doc type"), "got: {}", err);
    assert!(err.contains("rfc"), "error should list valid types, got: {}", err);
    assert!(err.contains("story"), "error should list valid types, got: {}", err);
}

#[test]
fn slugify_converts_title() {
    assert_eq!(template::slugify("Event Sourcing"), "event-sourcing");
    assert_eq!(template::slugify("API v2.0 Design"), "api-v2-0-design");
    assert_eq!(template::slugify("  Hello  World  "), "hello-world");
}

#[test]
fn singleton_create_first_succeeds() {
    let fixture = common::TestFixture::new();
    let mut config = fixture.config();
    config.documents.types.retain(|t| t.name != "convention");
    config.documents.types.push(singleton_type("convention", "docs/conventions", "CONVENTION"));
    fs::create_dir_all(fixture.root().join("docs/conventions")).unwrap();

    let store = fixture.store();
    let result = lazyspec::cli::create::run(
        fixture.root(), &config, &store, "convention", "Code Style", "alice", |_| {},
    );
    assert!(result.is_ok(), "first singleton create should succeed: {:?}", result.err());
    assert!(result.unwrap().exists());
}

#[test]
fn singleton_create_second_fails() {
    let fixture = common::TestFixture::new();
    let mut config = fixture.config();
    config.documents.types.retain(|t| t.name != "convention");
    config.documents.types.push(singleton_type("convention", "docs/conventions", "CONVENTION"));
    fs::create_dir_all(fixture.root().join("docs/conventions")).unwrap();

    let store = fixture.store();
    let _first = lazyspec::cli::create::run(
        fixture.root(), &config, &store, "convention", "Code Style", "alice", |_| {},
    ).unwrap();

    // Reload store so it picks up the newly created document
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = lazyspec::cli::create::run(
        fixture.root(), &config, &store, "convention", "Another Convention", "bob", |_| {},
    );

    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"), "expected 'already exists' error, got: {}", err);
    assert!(err.contains("docs/conventions"), "expected path in error, got: {}", err);
}

#[test]
fn non_singleton_create_multiple_succeeds() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let first = lazyspec::cli::create::run(
        fixture.root(), &config, &fixture.store(), "rfc", "First RFC", "alice", |_| {},
    );
    assert!(first.is_ok(), "first create should succeed: {:?}", first.err());

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let second = lazyspec::cli::create::run(
        fixture.root(), &config, &store, "rfc", "Second RFC", "bob", |_| {},
    );
    assert!(second.is_ok(), "second create of non-singleton should succeed: {:?}", second.err());
}
