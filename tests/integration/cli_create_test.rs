use lazyspec::engine::config::{Config, StoreBackend, TypeDef};
use lazyspec::engine::git_ref::GitCli;
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
        dir: "docs/milestones".to_string(),
        ..TypeDef::test_fixture("milestone", StoreBackend::GithubMilestones)
    }];
    config.documents.github = None;
    config
}

fn singleton_type(name: &str, dir: &str, prefix: &str) -> TypeDef {
    TypeDef {
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        singleton: true,
        ..TypeDef::test_fixture(name, Default::default())
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
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
        &GitCli,
        |_| {},
    )
    .unwrap()
    .0;

    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("title: \"Body Test\""),
        "should have title, got: {}",
        content
    );
    assert!(
        content.ends_with(&format!("---\n\n{}\n", body_content)),
        "should have body content after a separator blank line, got: {}",
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
        &GitCli,
        |_| {},
    )
    .unwrap()
    .0;

    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("title: \"Body File Test\""),
        "should have title, got: {}",
        content
    );
    assert!(
        content.ends_with("---\n\nBody from file.\n"),
        "should have body from file after a separator blank line, got: {}",
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
        &GitCli,
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
        &GitCli,
        |_| {},
    );
    assert!(
        second.is_ok(),
        "second create of non-singleton should succeed: {:?}",
        second.err()
    );
}

// ITERATION-316 / BUG-006: a create --json on a non-pushing (filesystem) store
// reports `synced: true` and carries no `warnings` key. This exercises the full
// create ops -> CLI serialization chain that threads the backend PushOutcome
// into the mutation JSON.
#[test]
fn create_json_filesystem_reports_synced_true_no_warnings() {
    let fixture = crate::common::TestFixture::new();
    let config = fixture.config();
    let store = fixture.store();

    let output = lazyspec::cli::create::run_json(
        fixture.root(),
        &config,
        &store,
        "rfc",
        "Synced RFC",
        "agent",
        &GitCli,
        |_| {},
    )
    .unwrap();

    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        json["synced"],
        serde_json::json!(true),
        "filesystem create must report synced:true, got: {output}"
    );
    assert!(
        json.get("warnings").is_none(),
        "a synced create must omit warnings, got: {output}"
    );
}

// RFC-074 AC2/AC5: `create` on a directory template scaffolds every file in
// it -- `.md` files substituted, non-`.md` copied verbatim -- and `--json`
// reports the scaffolded parts (in template order) and sidecars.
#[test]
fn create_scaffolds_a_directory_template_and_reports_parts_and_sidecars() {
    let fixture = crate::common::TestFixture::new();
    let root = fixture.root();

    let mut config = Config::default();
    config.documents.types.push(TypeDef {
        dir: "docs/changes".to_string(),
        prefix: "CHANGE".to_string(),
        subdirectory: true,
        ..TypeDef::test_fixture("change", StoreBackend::Filesystem)
    });
    fs::write(root.join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();

    let template_dir = root.join(".lazyspec/templates/change");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(
        template_dir.join("index.md"),
        "---\ntitle: \"{title}\"\ntype: {type}\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n\nParent for {title}.\n",
    )
    .unwrap();
    fs::write(template_dir.join("design.md"), "# Design for {title}\n").unwrap();
    fs::write(template_dir.join("tasks.md"), "# Tasks for {title}\n").unwrap();
    fs::write(template_dir.join("index.yaml"), "sidecar: \"{title}\"\n").unwrap();

    let store = lazyspec::engine::store::Store::load(root, &config).unwrap();
    let output = lazyspec::cli::create::run_json(
        root,
        &config,
        &store,
        "change",
        "Add caching",
        "agent",
        &GitCli,
        |_| {},
    )
    .unwrap();

    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let index_path = root.join(json["path"].as_str().unwrap());
    let index_content = fs::read_to_string(&index_path).unwrap();
    assert!(
        index_content.contains("title: \"Add caching\""),
        "got: {index_content}"
    );

    let design_content =
        fs::read_to_string(index_path.parent().unwrap().join("design.md")).unwrap();
    assert_eq!(design_content, "# Design for Add caching\n");

    let sidecar_content =
        fs::read_to_string(index_path.parent().unwrap().join("index.yaml")).unwrap();
    assert_eq!(
        sidecar_content, "sidecar: \"{title}\"\n",
        "a sidecar is copied verbatim, never substituted"
    );

    let part_names: Vec<&str> = json["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(part_names, vec!["design", "tasks"]);
    assert_eq!(
        json["parts"][0]["path"],
        serde_json::json!(format!(
            "{}",
            index_path
                .parent()
                .unwrap()
                .join("design.md")
                .strip_prefix(root)
                .unwrap()
                .display()
        ))
    );

    let sidecars: Vec<&str> = json["sidecars"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(
        sidecars,
        vec!["docs/changes/CHANGE-001-add-caching/index.yaml"]
    );
}

// RFC-074 AC5: a document scaffolded from a directory template keeps the
// template's declared order for its declared parts, then sorts any extra
// (undeclared) part alphabetically after them -- regression for a bug where
// `directory_template_part_order` returned filenames ("design.md") while the
// loader compared against stems ("design"), so nothing ever matched and every
// part fell back to alphabetical order regardless of the template.
#[test]
fn create_then_extra_part_orders_declared_parts_before_alphabetical_extras() {
    let fixture = crate::common::TestFixture::new();
    let root = fixture.root();

    let mut config = Config::default();
    config.documents.types.push(TypeDef {
        dir: "docs/changes".to_string(),
        prefix: "CHANGE".to_string(),
        subdirectory: true,
        ..TypeDef::test_fixture("change", StoreBackend::Filesystem)
    });
    fs::write(root.join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();

    let template_dir = root.join(".lazyspec/templates/change");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(
        template_dir.join("index.md"),
        "---\ntitle: \"{title}\"\ntype: {type}\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n\nParent for {title}.\n",
    )
    .unwrap();
    // Declared in template order design, tasks -- deliberately not alphabetical
    // relative to the extra part added below ("appendix" sorts before both).
    fs::write(template_dir.join("design.md"), "# Design for {title}\n").unwrap();
    fs::write(template_dir.join("tasks.md"), "# Tasks for {title}\n").unwrap();

    let store = lazyspec::engine::store::Store::load(root, &config).unwrap();
    let created = lazyspec::engine::ops::create::run_with_body_full(
        root,
        &config,
        &store,
        "change",
        "Add caching",
        "agent",
        None,
        None,
        &GitCli,
        |_| {},
    )
    .unwrap();

    // An extra, undeclared part dropped in after scaffolding -- not part of
    // the template, so it must sort alphabetically after the declared ones.
    let spec_dir = created.path.parent().unwrap();
    fs::write(spec_dir.join("appendix.md"), "# Appendix\n").unwrap();

    let store = lazyspec::engine::store::Store::load(root, &config).unwrap();
    let doc = store.resolve_shorthand("CHANGE-001").unwrap();
    let part_names: Vec<&str> = doc.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(part_names, vec!["design", "tasks", "appendix"]);
}

// RFC-074 AC1: a directory template with no `index.md` is a config error at
// create time.
#[test]
fn create_errors_when_directory_template_has_no_index_md() {
    let fixture = crate::common::TestFixture::new();
    let root = fixture.root();

    let mut config = Config::default();
    config.documents.types.push(TypeDef {
        dir: "docs/changes".to_string(),
        prefix: "CHANGE".to_string(),
        subdirectory: true,
        ..TypeDef::test_fixture("change", StoreBackend::Filesystem)
    });
    fs::write(root.join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();

    let template_dir = root.join(".lazyspec/templates/change");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("design.md"), "# Design\n").unwrap();

    let store = lazyspec::engine::store::Store::load(root, &config).unwrap();
    let err = lazyspec::cli::create::run(
        root,
        &config,
        &store,
        "change",
        "Add caching",
        "agent",
        &GitCli,
        |_| {},
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("index.md"), "got: {err:#}");
}
