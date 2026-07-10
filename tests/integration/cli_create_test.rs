use lazyspec::engine::config::{Config, NumberingStrategy, StoreBackend, TypeDef};
use lazyspec::engine::template;
use std::fs;

/// A config whose only type is a github-milestones-backed type with NO
/// `[github]` config. Used to prove the real command path routes such a type
/// into the milestone branch: if it falls through to the filesystem path the
/// call succeeds; if it enters the milestone branch it errors on the missing
/// `[github]` config.
fn milestones_only_config() -> Config {
    let mut config = Config::default();
    config.documents.types = vec![TypeDef {
        name: "milestone".to_string(),
        plural: "milestones".to_string(),
        dir: "docs/milestones".to_string(),
        prefix: "MILESTONE".to_string(),
        icon: None,
        numbering: NumberingStrategy::Incremental,
        subdirectory: false,
        store: StoreBackend::GithubMilestones,
        singleton: false,
        parent_type: None,
        agents: Vec::new(),
        intent: None,
        authorship: Default::default(),
        lifecycle: Default::default(),
        attributes: Default::default(),
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        clickup_list_id: None,
        clickup_task_type: None,
        clickup_custom_field_map: None,
    }];
    config.documents.github = None;
    config
}

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
        agents: Vec::new(),
        intent: None,
        authorship: Default::default(),
        lifecycle: Default::default(),
        attributes: Default::default(),
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        clickup_list_id: None,
        clickup_task_type: None,
        clickup_custom_field_map: None,
    }
}

#[test]
fn create_generates_doc_from_template() {
    let fixture = crate::common::TestFixture::new();
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
    let path = lazyspec::cli::create::run(
        root,
        &config,
        &fixture.store(),
        "rfc",
        "Event Sourcing",
        "jkaloger",
        |_| {},
    )
    .unwrap();

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("title: \"Event Sourcing\""));
    assert!(content.contains("type: rfc"));
    assert!(content.contains("author: \"jkaloger\""));
}

#[test]
fn create_auto_increments_number() {
    let fixture = crate::common::TestFixture::new();
    let root = fixture.root();

    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(
        root.join(".lazyspec/templates/rfc.md"),
        "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n",
    )
    .unwrap();

    fs::write(root.join("docs/rfcs/RFC-001-old.md"), "").unwrap();

    let config = fixture.config();
    let path = lazyspec::cli::create::run(
        root,
        &config,
        &fixture.store(),
        "rfc",
        "New Feature",
        "a",
        |_| {},
    )
    .unwrap();

    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("RFC-002"), "got: {}", filename);
}

#[test]
fn create_with_date_pattern() {
    let fixture = crate::common::TestFixture::new();
    let root = fixture.root();

    fs::create_dir_all(root.join(".lazyspec/templates")).unwrap();
    fs::write(
        root.join(".lazyspec/templates/rfc.md"),
        "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n",
    )
    .unwrap();

    let mut config = fixture.config();
    config.documents.naming.pattern = "{date}-{title}.md".to_string();

    let path = lazyspec::cli::create::run(
        root,
        &config,
        &fixture.store(),
        "rfc",
        "My Feature",
        "a",
        |_| {},
    )
    .unwrap();

    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.ends_with("-my-feature.md"), "got: {}", filename);
}

#[test]
fn create_uses_default_template_when_custom_missing() {
    let fixture = crate::common::TestFixture::new();

    let config = fixture.config();
    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "story",
        "API Design",
        "jkaloger",
        |_| {},
    )
    .unwrap();

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("title: \"API Design\""));
    assert!(content.contains("type: story"));
    assert!(content.contains("status: draft"));
}

#[test]
fn create_uses_generic_default_template() {
    let fixture = crate::common::TestFixture::new();

    let config = fixture.config();
    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "iteration",
        "Auth Impl 1",
        "agent",
        |_| {},
    )
    .unwrap();

    let content = fs::read_to_string(&path).unwrap();
    // The embedded fallback is type-agnostic: `{type}` is substituted, and the
    // generic intent/guidance scaffolding is present regardless of type.
    assert!(content.contains("type: iteration"));
    assert!(content.contains("<!-- intent:"));
    assert!(content.contains("<!-- guidance:"));
    assert!(content.contains("## "));
}

#[test]
fn create_unknown_type_returns_error_with_valid_types() {
    let fixture = crate::common::TestFixture::new();
    let config = fixture.config();
    let result = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "foobar",
        "Test",
        "a",
        |_| {},
    );
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown doc type"), "got: {}", err);
    assert!(
        err.contains("rfc"),
        "error should list valid types, got: {}",
        err
    );
    assert!(
        err.contains("story"),
        "error should list valid types, got: {}",
        err
    );
}

// Regression: a github-milestones type must route through the milestone branch
// of the real `create` command path, not fall through to the filesystem store.
// With no [github] config the milestone branch errors; a filesystem fall-through
// would instead succeed.
#[test]
fn create_github_milestones_type_routes_to_milestone_branch() {
    let fixture = crate::common::TestFixture::new();
    let config = milestones_only_config();
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let result = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "milestone",
        "v1.0",
        "author",
        |_| {},
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("github-milestones store but no [github] config"),
        "expected milestone-branch error, got: {}",
        err
    );
}

#[test]
fn slugify_converts_title() {
    assert_eq!(template::slugify("Event Sourcing"), "event-sourcing");
    assert_eq!(template::slugify("API v2.0 Design"), "api-v2-0-design");
    assert_eq!(template::slugify("  Hello  World  "), "hello-world");
}

#[test]
fn singleton_create_first_succeeds() {
    let fixture = crate::common::TestFixture::new();
    let mut config = fixture.config();
    config.documents.types.retain(|t| t.name != "convention");
    config.documents.types.push(singleton_type(
        "convention",
        "docs/conventions",
        "CONVENTION",
    ));
    fs::create_dir_all(fixture.root().join("docs/conventions")).unwrap();

    let store = fixture.store();
    let result = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "convention",
        "Code Style",
        "alice",
        |_| {},
    );
    assert!(
        result.is_ok(),
        "first singleton create should succeed: {:?}",
        result.err()
    );
    assert!(result.unwrap().exists());
}

#[test]
fn singleton_create_second_fails() {
    let fixture = crate::common::TestFixture::new();
    let mut config = fixture.config();
    config.documents.types.retain(|t| t.name != "convention");
    config.documents.types.push(singleton_type(
        "convention",
        "docs/conventions",
        "CONVENTION",
    ));
    fs::create_dir_all(fixture.root().join("docs/conventions")).unwrap();

    let store = fixture.store();
    let _first = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "convention",
        "Code Style",
        "alice",
        |_| {},
    )
    .unwrap();

    // Reload store so it picks up the newly created document
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "convention",
        "Another Convention",
        "bob",
        |_| {},
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("already exists"),
        "expected 'already exists' error, got: {}",
        err
    );
    assert!(
        err.contains("docs/conventions"),
        "expected path in error, got: {}",
        err
    );
}

#[test]
fn create_with_body_sets_content() {
    let fixture = crate::common::TestFixture::new();
    let config = fixture.config();
    let store = fixture.store();

    let body_content = "This is the body content.";
    let path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "rfc",
        "Body Test",
        "agent",
        None,
        Some(body_content),
        |_| {},
    )
    .unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("title: \"Body Test\""),
        "should have title, got: {}",
        content
    );
    assert!(
        content.contains(body_content),
        "should have body content, got: {}",
        content
    );
}

#[test]
fn create_with_body_file_sets_content() {
    let fixture = crate::common::TestFixture::new();
    let config = fixture.config();
    let store = fixture.store();

    let body_file = fixture.root().join("body.txt");
    fs::write(&body_file, "Body from file.").unwrap();

    let body_content = fs::read_to_string(&body_file).unwrap();

    let path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "rfc",
        "Body File Test",
        "agent",
        None,
        Some(body_content.as_str()),
        |_| {},
    )
    .unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("title: \"Body File Test\""),
        "should have title, got: {}",
        content
    );
    assert!(
        content.contains("Body from file."),
        "should have body from file, got: {}",
        content
    );
}

#[test]
fn resolve_body_rejects_both_flags() {
    let body = Some("inline".to_string());
    let body_file = Some("file.txt".to_string());
    let result = lazyspec::cli::resolve_body(&body, &body_file);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("cannot use both"),
        "should reject both flags"
    );
}

#[test]
fn non_singleton_create_multiple_succeeds() {
    let fixture = crate::common::TestFixture::new();
    let config = fixture.config();

    let first = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "rfc",
        "First RFC",
        "alice",
        |_| {},
    );
    assert!(
        first.is_ok(),
        "first create should succeed: {:?}",
        first.err()
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let second = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "rfc",
        "Second RFC",
        "bob",
        |_| {},
    );
    assert!(
        second.is_ok(),
        "second create of non-singleton should succeed: {:?}",
        second.err()
    );
}
