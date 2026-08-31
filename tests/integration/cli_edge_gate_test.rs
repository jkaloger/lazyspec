use crate::common::TestFixture;
use lazyspec::engine::config::{
    starter_relationships, starter_types, Config, DocumentConfig, EdgeDef, FilesystemConfig,
    Lifecycle, Naming, Templates, TypeDef,
};
use lazyspec::engine::store::Store;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// `bug` runs a lifecycle disjoint from the starter one: no `accepted` state, so
// a single required status could never gate both it and `story`.
fn bug_type() -> TypeDef {
    let lifecycle = Lifecycle {
        states: ["reported", "triaged", "in-progress", "fixed", "wontfix"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        edges: vec![],
    };
    let mut bug = starter_types()
        .into_iter()
        .find(|t| t.name == "story")
        .unwrap();
    bug.name = "bug".to_string();
    bug.plural = "bugs".to_string();
    bug.dir = "docs/bugs".to_string();
    bug.prefix = "BUG".to_string();
    bug.lifecycle = lifecycle;
    bug
}

fn config_with_edge(require_to_status: BTreeMap<String, String>) -> Config {
    let mut types = starter_types();
    types.push(bug_type());
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
        rules: vec![],
        edges: vec![EdgeDef {
            name: "iterations-implement-work".to_string(),
            from: "iteration".to_string(),
            to: vec!["story".to_string(), "bug".to_string()],
            via: "implements".to_string(),
            required: None,
            require_to_status,
        }],
        ref_count_ceiling: 15,
        certification: Default::default(),
        agents: Default::default(),
        skills: Default::default(),
        web: None,
        git_ref: Default::default(),
    }
}

fn story_and_bug_gate() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("story".to_string(), "accepted".to_string()),
        ("bug".to_string(), "triaged".to_string()),
    ])
}

// The in-process tests pass `config` directly; a subprocess reads it from disk.
fn write_config(fixture: &TestFixture, config: &Config) {
    fs::write(
        fixture.root().join(".lazyspec.toml"),
        config.to_toml().unwrap(),
    )
    .unwrap();
}

fn write_bug(fixture: &TestFixture, filename: &str, title: &str, status: &str) {
    fs::create_dir_all(fixture.root().join("docs/bugs")).unwrap();
    let content = format!(
        "---\ntitle: \"{}\"\ntype: bug\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
        title, status
    );
    fixture.write_doc(&format!("docs/bugs/{}", filename), &content);
}

fn create_iteration(fixture: &TestFixture, config: &Config) -> anyhow::Result<std::path::PathBuf> {
    let store = Store::load(fixture.root(), config).unwrap();
    lazyspec::cli::create::run_with_body(
        fixture.root(),
        config,
        &store,
        "iteration",
        "Slice",
        "test",
        None,
        None,
        |_| {},
    )
    .map(|(path, _)| path)
}

fn iterations_in(root: &Path) -> usize {
    fs::read_dir(root.join("docs/iterations")).unwrap().count()
}

// AC1 — no target sits at the status its own type is gated on, so create is
// refused and the message names the required status.
#[test]
fn edge_gate_refuses_create_when_no_target_reaches_its_required_status() {
    let fixture = TestFixture::new();
    let config = config_with_edge(story_and_bug_gate());
    fixture.write_story("STORY-001-work.md", "Work", "draft", None);

    let err = create_iteration(&fixture, &config).unwrap_err();

    assert_eq!(
        err.to_string(),
        "cannot create iteration: edge \"iterations-implement-work\" requires \
         story at \"accepted\" (found draft), bug at \"triaged\" (none exists)"
    );
    assert_eq!(iterations_in(fixture.root()), 0);
}

// AC2 — the gate is read per target type: a bug at its own required status
// satisfies the edge even while every story is still at draft.
#[test]
fn edge_gate_allows_create_when_one_target_type_reaches_its_own_status() {
    let fixture = TestFixture::new();
    let config = config_with_edge(story_and_bug_gate());
    fixture.write_story("STORY-001-work.md", "Work", "draft", None);
    write_bug(&fixture, "BUG-001-crash.md", "Crash", "triaged");

    let path = create_iteration(&fixture, &config).unwrap();

    assert!(path.exists());
}

// AC3 — a bug short of `triaged` leaves the edge unsatisfied. `bug` has no
// `accepted` state, so only a per-type gate can express this refusal.
#[test]
fn edge_gate_refuses_create_when_the_bug_is_short_of_its_own_status() {
    let fixture = TestFixture::new();
    let config = config_with_edge(story_and_bug_gate());
    fixture.write_story("STORY-001-work.md", "Work", "draft", None);
    write_bug(&fixture, "BUG-001-crash.md", "Crash", "reported");

    let err = create_iteration(&fixture, &config).unwrap_err();

    assert_eq!(
        err.to_string(),
        "cannot create iteration: edge \"iterations-implement-work\" requires \
         story at \"accepted\" (found draft), bug at \"triaged\" (found reported)"
    );
    assert_eq!(iterations_in(fixture.root()), 0);
}

// AC4 — an absent `require_to_status` key leaves that target type ungated, so
// any bug at all satisfies the edge.
#[test]
fn edge_gate_allows_create_against_an_ungated_target_type() {
    let fixture = TestFixture::new();
    let config = config_with_edge(BTreeMap::from([(
        "story".to_string(),
        "accepted".to_string(),
    )]));
    fixture.write_story("STORY-001-work.md", "Work", "draft", None);
    write_bug(&fixture, "BUG-001-crash.md", "Crash", "reported");

    let path = create_iteration(&fixture, &config).unwrap();

    assert!(path.exists());
}

// An edge declaring no `require_to_status` at all gates nothing, so create
// succeeds with no target document in the project.
#[test]
fn edge_without_require_to_status_gates_nothing() {
    let fixture = TestFixture::new();
    let config = config_with_edge(BTreeMap::new());

    let path = create_iteration(&fixture, &config).unwrap();

    assert!(path.exists());
}

// An ungated target type holding no document at all cannot stand in for the
// gated one, so the edge stays unsatisfied and create is still refused.
#[test]
fn edge_gate_refuses_create_when_the_ungated_target_type_has_no_documents() {
    let fixture = TestFixture::new();
    let config = config_with_edge(BTreeMap::from([(
        "story".to_string(),
        "accepted".to_string(),
    )]));
    fixture.write_story("STORY-001-work.md", "Work", "draft", None);

    let err = create_iteration(&fixture, &config).unwrap_err();

    assert_eq!(
        err.to_string(),
        "cannot create iteration: edge \"iterations-implement-work\" requires \
         story at \"accepted\" (found draft)"
    );
    assert_eq!(iterations_in(fixture.root()), 0);
}

// AC6 — the refusal reaches `--json` carrying the edge name and, per
// unsatisfied target type, the required status and the statuses actually held.
// It exits non-zero, as the human-readable refusal does: a refused create must
// not read as success to a caller chaining on `&&`.
#[test]
fn edge_gate_refusal_is_machine_readable_under_json() {
    let fixture = TestFixture::new();
    let config = config_with_edge(story_and_bug_gate());
    write_config(&fixture, &config);
    fixture.write_story("STORY-001-work.md", "Work", "draft", None);
    fixture.write_story("STORY-002-more.md", "More", "review", None);

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(["create", "iteration", "Slice", "--author", "test", "--json"])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec create");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({
            "error": "edge_status_gate",
            "edge": "iterations-implement-work",
            "type": "iteration",
            "unsatisfied": [
                {
                    "target_type": "story",
                    "required_status": "accepted",
                    "current_statuses": ["draft", "review"],
                },
                {
                    "target_type": "bug",
                    "required_status": "triaged",
                },
            ],
        })
    );
    assert_eq!(iterations_in(fixture.root()), 0);
}
