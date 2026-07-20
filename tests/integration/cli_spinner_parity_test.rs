use std::process::Command;

// ITERATION-334 / STORY-231 AC2: with `--json` or a non-TTY stdout, the create
// spinner emits zero animation. Running the binary through captured pipes makes
// both stdout and stderr non-terminals, so `op_spinner` returns `None` and the
// bytes match the pre-spinner behaviour exactly.

fn fixture_with_rfc_template() -> crate::common::TestFixture {
    let fixture = crate::common::TestFixture::new();
    let templates = fixture.root().join(".lazyspec/templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(
        templates.join("rfc.md"),
        "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\n",
    )
    .unwrap();
    fixture
}

fn run_create(root: &std::path::Path, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["create", "rfc", "Event Sourcing", "--author", "tester"];
    args.extend_from_slice(extra);
    Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(&args)
        .current_dir(root)
        .output()
        .expect("failed to run lazyspec create")
}

#[test]
fn create_json_stdout_has_no_escape_codes() {
    let fixture = fixture_with_rfc_template();
    let output = run_create(fixture.root(), &["--json"]);
    assert!(
        output.status.success(),
        "create --json should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.stdout.contains(&0x1b),
        "json stdout must contain no ESC bytes"
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("json stdout must parse");
    assert_eq!(parsed["type"], "rfc");
    assert_eq!(parsed["title"], "Event Sourcing");
}

#[test]
fn create_piped_stdout_is_the_bare_path_line() {
    let fixture = fixture_with_rfc_template();
    let output = run_create(fixture.root(), &[]);
    assert!(
        output.status.success(),
        "create should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.stdout.contains(&0x1b),
        "piped stdout must contain no ESC bytes"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout.trim_end();
    assert!(
        !line.contains('\n'),
        "piped stdout must be a single path line, got: {line:?}"
    );
    assert!(
        line.ends_with("docs/rfcs/RFC-001-event-sourcing.md"),
        "piped stdout must be the created path, got: {line:?}"
    );
}
