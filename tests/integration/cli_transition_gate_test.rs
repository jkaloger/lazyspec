use crate::common::TestFixture;
use lazyspec::engine::config::{
    starter_relationships, starter_types, Config, DocumentConfig, FilesystemConfig, Naming,
    Severity, Templates, TypeDef, ValidationRule,
};
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::store::Store;
use std::fs;

fn status_of(fixture: &TestFixture, rel: &str) -> String {
    let content = fs::read_to_string(fixture.root().join(rel)).unwrap();
    format!("{}", DocMeta::parse(&content).unwrap().status)
}

// AC4 — reject an off-edge move; the doc's status is unchanged after the call.
#[test]
fn transition_rejects_off_edge_move() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    // draft -> accepted is not an edge in the default lifecycle (only draft->review).
    let err = lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "accepted")],
        Some(&config),
    )
    .unwrap_err();
    assert!(err.to_string().contains("invalid transition"), "got: {err}");

    // The file is unchanged: the bail occurred before any write.
    assert_eq!(status_of(&fixture, "docs/rfcs/RFC-001-test.md"), "draft");
}

// AC4 — accept an on-edge move.
#[test]
fn transition_accepts_on_edge_move() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "review")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(status_of(&fixture, "docs/rfcs/RFC-001-test.md"), "review");
}

// AC4 — a `* -> rejected`/`* -> superseded` edge lets any state move there.
#[test]
fn transition_wildcard_edge_allows_move_from_any_state() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "complete");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "superseded")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(
        status_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        "superseded"
    );
}

// AC4 — setting the status to its current value is a no-op, not a missing edge.
#[test]
fn transition_no_op_to_same_status_is_allowed() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("status", "draft")],
        Some(&config),
    )
    .unwrap();

    assert_eq!(status_of(&fixture, "docs/rfcs/RFC-001-test.md"), "draft");
}

// AC4 — a non-status update performs no transition check.
#[test]
fn transition_check_skipped_for_non_status_updates() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_with_config(
        fixture.root(),
        &store,
        "RFC-001",
        &[("title", "Renamed")],
        Some(&config),
    )
    .unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.title, "Renamed");
    assert_eq!(format!("{}", meta.status), "draft");
}

// A config whose rule requires the parent (rfc) at status `accepted` before a
// `story` child may be created.
fn config_with_parent_status_gate() -> Config {
    let types: Vec<TypeDef> = starter_types();
    Config {
        documents: DocumentConfig {
            types,
            naming: Naming {
                pattern: "{type}-{n:03}-{title}.md".to_string(),
            },
            sqids: None,
            reserved: None,
            github: None,
        },
        filesystem: FilesystemConfig {
            templates: Templates {
                dir: ".lazyspec/templates".to_string(),
            },
        },
        relationships: starter_relationships(),
        ui: Default::default(),
        rules: vec![ValidationRule::ParentChild {
            name: "stories-need-accepted-rfcs".to_string(),
            child: "story".to_string(),
            parent: "rfc".to_string(),
            severity: Severity::Error,
            require_parent_status: Some("accepted".to_string()),
        }],
        ref_count_ceiling: 15,
        certification: Default::default(),
        coordination: None,
        agents: Default::default(),
        skills: Default::default(),
    }
}

// AC5 — the gate blocks creation while no parent has reached the required status,
// then allows it once one has.
#[test]
fn parent_status_gate_blocks_then_allows() {
    let fixture = TestFixture::new();
    let config = config_with_parent_status_gate();

    // A parent rfc at draft (not accepted).
    fixture.write_rfc("RFC-001-test.md", "Parent", "draft");
    let store = Store::load(fixture.root(), &config).unwrap();

    let err = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "story",
        "Child",
        "test",
        None,
        None,
        |_| {},
    )
    .unwrap_err();
    assert!(err.to_string().contains("accepted"), "got: {err}");

    // No story document was written.
    let stories: Vec<_> = fs::read_dir(fixture.root().join("docs/stories"))
        .unwrap()
        .collect();
    assert!(stories.is_empty(), "no child should have been created");

    // Promote the parent to accepted, reload, and retry.
    fixture.write_rfc("RFC-001-test.md", "Parent", "accepted");
    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "story",
        "Child",
        "test",
        None,
        None,
        |_| {},
    )
    .unwrap();
    assert!(path.exists(), "child document should now exist");
}

// AC5 — a child type whose rule has no require_parent_status is created freely.
#[test]
fn no_gate_when_require_parent_status_unset() {
    let fixture = TestFixture::new();
    // Default config: rules carry no require_parent_status.
    let config = Config::default();
    std::fs::write(
        fixture.root().join(".lazyspec.toml"),
        config.to_toml().unwrap(),
    )
    .unwrap();

    // No parent rfc exists at all.
    let store = Store::load(fixture.root(), &config).unwrap();

    let path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "story",
        "Child",
        "test",
        None,
        None,
        |_| {},
    )
    .unwrap();
    assert!(path.exists(), "child created freely without a gate");
}
