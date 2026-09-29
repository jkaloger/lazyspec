// STORY-292 AC2: `missing-part` findings must reach every CLI surface that
// carries validation findings -- not just the engine's own `ValidationResult`
// -- so this spawns the real binary against a project on disk rather than
// calling an engine function in-process.
use lazyspec::engine::config::{Config, StoreBackend, TypeDef};
use std::fs;
use std::process::Command;

/// A project with one `change` bundle whose declared `design.md` part is
/// absent, so `missing-part` fires for it (STORY-292 AC1).
fn project_missing_a_part() -> crate::common::TestFixture {
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
        "---\ntitle: \"{title}\"\ntype: {type}\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n\nbody\n",
    )
    .unwrap();
    fs::write(template_dir.join("design.md"), "# Design for {title}\n").unwrap();

    let doc_dir = root.join("docs/changes/CHANGE-001-alpha");
    fs::create_dir_all(&doc_dir).unwrap();
    fs::write(
        doc_dir.join("index.md"),
        "---\ntitle: \"Alpha\"\ntype: change\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n\nbody\n",
    )
    .unwrap();
    // design.md deliberately absent.

    fixture
}

fn run_json(root: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("failed to run lazyspec {args:?}: {e}"));
    serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap_or_else(|e| {
        panic!(
            "lazyspec {args:?} did not print JSON ({e}); stdout: {} stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn has_missing_part(warnings: &[serde_json::Value]) -> bool {
    warnings
        .iter()
        .any(|w| w["rule"] == "missing-part" && w["part"] == "design.md")
}

#[test]
fn validate_json_carries_missing_part() {
    let fixture = project_missing_a_part();

    let parsed = run_json(fixture.root(), &["validate", "--json"]);

    let warnings = parsed["warnings"].as_array().unwrap();
    assert!(has_missing_part(warnings), "got: {parsed}");
}

#[test]
fn validate_id_scopes_to_the_bundles_missing_part() {
    let fixture = project_missing_a_part();

    let parsed = run_json(
        fixture.root(),
        &["validate", "--id", "CHANGE-001", "--json"],
    );

    let warnings = parsed["warnings"].as_array().unwrap();
    assert!(has_missing_part(warnings), "got: {parsed}");
}

#[test]
fn status_json_carries_missing_part() {
    let fixture = project_missing_a_part();

    let parsed = run_json(fixture.root(), &["status", "--json"]);

    // `status --json` nests validation findings under `validation`, unlike
    // `validate --json`'s top-level `warnings` (each command owns its own
    // envelope).
    let warnings = parsed["validation"]["warnings"].as_array().unwrap();
    assert!(has_missing_part(warnings), "got: {parsed}");
}
