use crate::common::TestFixture;
use lazyspec::engine::config::{
    starter_types, Config, Edge, Lifecycle, NumberingStrategy, StoreBackend, TypeDef,
};
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;
use std::fs;
use std::path::Path;

// BUG-002 regression scaffolding: a bug type whose lifecycle starts at
// `reported`, not the default `draft`. This is the scenario that previously
// dead-ended -- a new bug born `draft` had no edge out of `draft`.
fn bug_type() -> TypeDef {
    let edge = |from: &str, to: &str| Edge {
        from: from.into(),
        to: to.into(),
    };
    TypeDef {
        name: "bug".to_string(),
        plural: "bugs".to_string(),
        dir: "docs/bugs".to_string(),
        prefix: "BUG".to_string(),
        icon: None,
        numbering: NumberingStrategy::Incremental,
        subdirectory: false,
        store: StoreBackend::Filesystem,
        singleton: false,
        parent_type: None,
        agents: Vec::new(),
        intent: None,
        authorship: Default::default(),
        lifecycle: Lifecycle {
            states: ["reported", "triaged", "in-progress", "fixed", "wontfix"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            edges: vec![
                edge("reported", "triaged"),
                edge("triaged", "in-progress"),
                edge("in-progress", "fixed"),
                edge("reported", "wontfix"),
                edge("triaged", "wontfix"),
            ],
        },
        attributes: Default::default(),
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        clickup_list_id: None,
        clickup_task_type: None,
        clickup_custom_field_map: None,
    }
}

// A config carrying the standard default-lifecycle types plus the bug type, so
// both the seeded-first-state and default-unchanged assertions have a subject.
fn bug_config() -> Config {
    let mut config = Config::default();
    let mut types = starter_types();
    types.push(bug_type());
    config.documents.types = types;
    config
}

fn status_at(path: &Path) -> String {
    let content = fs::read_to_string(path).unwrap();
    format!("{}", DocMeta::parse(&content).unwrap().status)
}

// AC — `create` seeds a new bug's status with the type's FIRST lifecycle state
// (`reported`), not the hardcoded `draft`.
#[test]
fn create_bug_seeds_first_lifecycle_state() {
    let fixture = TestFixture::new();
    let config = bug_config();
    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "bug",
        "Widget crash",
        "tester",
        |_| {},
    )
    .unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("status: reported"),
        "new bug should be born at first lifecycle state, got: {content}"
    );
    assert!(
        !content.contains("status: draft"),
        "new bug must not seed the hardcoded draft, got: {content}"
    );
}

// BUG-002 core regression: a bug born `reported` can transition to `triaged`.
// Previously the bug was born `draft` and dead-ended with "no edge from draft".
#[test]
fn bug_created_then_transitions_to_triaged() {
    let fixture = TestFixture::new();
    let config = bug_config();
    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "bug",
        "Widget crash",
        "tester",
        |_| {},
    )
    .unwrap();
    assert_eq!(status_at(&path), "reported", "precondition: born reported");

    // Reload: the store is a snapshot taken before the create wrote the file.
    let store = Store::load(fixture.root(), &config).unwrap();
    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "BUG-001",
        &[("status", "triaged")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(
        status_at(&path),
        "triaged",
        "bug should transition reported -> triaged"
    );
}

// AC — `fix` repairs a filesystem doc whose status is not in its type's
// lifecycle, rewriting it to `states[0]`. `--dry-run` reports without writing.
#[test]
fn fix_repairs_planted_out_of_lifecycle_status() {
    let fixture = TestFixture::new();
    let config = bug_config();

    // TestFixture does not create docs/bugs; plant the dir and an out-of-lifecycle doc.
    fs::create_dir_all(fixture.root().join("docs/bugs")).unwrap();
    let rel = "docs/bugs/BUG-1-planted.md";
    let planted = "---\ntitle: \"Planted\"\ntype: bug\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n";
    let path = fixture.write_doc(rel, planted);

    // Dry run: reports the fix but does not write.
    let store = Store::load(fixture.root(), &config).unwrap();
    let output = lazyspec::engine::ops::fix::plan_field_and_conflict_fixes(
        fixture.root(),
        &store,
        &config,
        &[],
        true,
        &RealFileSystem,
    );
    let fix = output
        .status_fixes
        .iter()
        .find(|f| f.path.contains("BUG-1-planted.md"))
        .unwrap_or_else(|| panic!("expected a status fix, got: {:?}", output.status_fixes));
    assert_eq!(fix.old_status, "draft", "got: {:?}", fix);
    assert_eq!(fix.new_status, "reported", "got: {:?}", fix);
    assert!(!fix.written, "dry run must not write, got: {:?}", fix);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        planted,
        "dry run must leave the file untouched"
    );

    // Real run: rewrites the status to the first lifecycle state.
    let store = Store::load(fixture.root(), &config).unwrap();
    let output = lazyspec::engine::ops::fix::plan_field_and_conflict_fixes(
        fixture.root(),
        &store,
        &config,
        &[],
        false,
        &RealFileSystem,
    );
    let fix = output
        .status_fixes
        .iter()
        .find(|f| f.path.contains("BUG-1-planted.md"))
        .unwrap_or_else(|| panic!("expected a status fix, got: {:?}", output.status_fixes));
    assert!(fix.written, "real run should write, got: {:?}", fix);

    let content = fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("status: reported"),
        "status should be repaired to first lifecycle state, got: {content}"
    );
    assert!(
        !content.contains("status: draft"),
        "invalid status should be gone, got: {content}"
    );
}

// AC — a default-lifecycle type is unchanged: create still seeds `draft`.
#[test]
fn default_lifecycle_type_still_seeds_draft() {
    let fixture = TestFixture::new();
    let config = bug_config();
    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &store,
        "story",
        "Some story",
        "tester",
        |_| {},
    )
    .unwrap();

    assert_eq!(
        status_at(&path),
        "draft",
        "default-lifecycle type should still be born at draft"
    );
}
