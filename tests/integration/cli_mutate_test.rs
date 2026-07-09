use crate::common::TestFixture;
use lazyspec::cli::json::doc_to_json;
use lazyspec::engine::config::{Config, NumberingStrategy, StoreBackend, TypeDef};
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::issue_map::IssueMap;
use std::fs;

/// A config whose only type is github-milestones-backed with NO `[github]`
/// config, plus a milestone cache doc + issue_map entry the store can resolve.
/// Proves the real update/delete command path routes the type into the
/// milestone branch (which errors on missing `[github]`) rather than falling
/// through to the filesystem path.
fn milestones_fixture() -> (TestFixture, Config) {
    let fixture = TestFixture::new();
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
        clickup_custom_field_map: None,
    }];
    config.documents.github = None;

    let cache_dir = fixture.root().join(".lazyspec/cache/milestone");
    fs::create_dir_all(&cache_dir).unwrap();
    fs::write(
        cache_dir.join("MILESTONE-1.md"),
        "---\ntitle: v1.0\ntype: milestone\nstatus: in-progress\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
    )
    .unwrap();

    let mut issue_map = IssueMap::load(fixture.root()).unwrap();
    issue_map.insert("MILESTONE-1", 1, "", "");
    issue_map.save(fixture.root()).unwrap();

    (fixture, config)
}

#[test]
fn update_status_in_frontmatter() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    lazyspec::cli::update::run(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-001-test.md",
        &[("status", "review")],
    )
    .unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(format!("{}", meta.status), "review");
}

#[test]
fn delete_removes_file() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    let path = fixture.root().join("docs/rfcs/RFC-001-test.md");
    assert!(path.exists());

    lazyspec::cli::delete::run(fixture.root(), &store, "docs/rfcs/RFC-001-test.md").unwrap();
    assert!(!path.exists());
}

#[test]
fn update_with_shorthand_id() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    lazyspec::cli::update::run(fixture.root(), &store, "RFC-001", &[("status", "review")]).unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(format!("{}", meta.status), "review");
}

#[test]
fn delete_with_shorthand_id() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    let path = fixture.root().join("docs/rfcs/RFC-001-test.md");
    assert!(path.exists());

    lazyspec::cli::delete::run(fixture.root(), &store, "RFC-001").unwrap();
    assert!(!path.exists());
}

#[test]
fn update_github_milestones_type_routes_to_milestone_branch() {
    let (fixture, config) = milestones_fixture();
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let err = lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "MILESTONE-1",
        &[("title", "v2.0")],
        Some(&config),
    )
    .unwrap_err()
    .to_string();

    assert!(
        err.contains("github-milestones store but no [github] config"),
        "expected milestone-branch error, got: {}",
        err
    );
}

#[test]
fn delete_github_milestones_type_routes_to_milestone_branch() {
    let (fixture, config) = milestones_fixture();
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let err = lazyspec::cli::delete::run_with_config(
        fixture.root(),
        &store,
        "MILESTONE-1",
        Some(&config),
    )
    .unwrap_err()
    .to_string();

    assert!(
        err.contains("github-milestones store but no [github] config"),
        "expected milestone-branch error, got: {}",
        err
    );
}

#[test]
fn update_with_json_flag() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    lazyspec::cli::update::run(fixture.root(), &store, "RFC-001", &[("status", "review")]).unwrap();

    // Reload store to pick up changes (mirrors what main.rs does for --json)
    let config = fixture.config();
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let doc = lazyspec::cli::resolve::resolve_shorthand_or_path(&store, "RFC-001").unwrap();
    let json_val = doc_to_json(doc);
    let output = serde_json::to_string_pretty(&json_val).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["status"], "review");
    assert_eq!(parsed["title"], "Test");
    assert_eq!(parsed["type"], "rfc");
    assert!(parsed["path"].as_str().unwrap().contains("RFC-001"));
}
