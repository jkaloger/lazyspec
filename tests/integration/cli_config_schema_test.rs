use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_lazyspec")
}

// The schema describes the shape of any .lazyspec.toml and is a property of the
// binary, so `config schema` must succeed in a directory with no config and emit
// parseable JSON on stdout.
#[test]
fn config_schema_works_without_a_project() {
    let tmp = TempDir::new().unwrap();

    let output = Command::new(binary())
        .args(["config", "schema"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run lazyspec");

    assert!(
        output.status.success(),
        "config schema should exit zero with no config, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert!(parsed.is_object(), "schema should be a JSON object");
}

// AC1: the `--json` flag is accepted and produces identical output (the schema is
// JSON either way).
#[test]
fn config_schema_json_flag_matches_default() {
    let tmp = TempDir::new().unwrap();

    let plain = Command::new(binary())
        .args(["config", "schema"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run lazyspec");
    assert!(plain.status.success());

    let with_json = Command::new(binary())
        .args(["config", "schema", "--json"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run lazyspec");
    assert!(
        with_json.status.success(),
        "config schema --json should exit zero, stderr: {}",
        String::from_utf8_lossy(&with_json.stderr)
    );

    assert_eq!(plain.stdout, with_json.stdout, "--json output should match");
}
