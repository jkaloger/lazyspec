use std::process::Command;

// ITERATION-335 / STORY-231 AC5 + dictum 2: the init wizard's talking-face
// greeting must never reach machine-readable or piped output. Running the binary
// through captured pipes makes stdout a non-terminal, so `should_greet` gates the
// animation off and no ESC bytes (nor the face box) appear on stdout.

fn run_init(root: &std::path::Path, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["init"];
    args.extend_from_slice(extra);
    Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(&args)
        .current_dir(root)
        .output()
        .expect("failed to run lazyspec init")
}

#[test]
fn init_json_stdout_has_no_greeting_or_escape_codes() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_init(dir.path(), &["--json"]);
    assert!(
        output.status.success(),
        "init --json should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.stdout.contains(&0x1b),
        "json stdout must contain no ESC bytes"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains('╭') && !stdout.contains('╰'),
        "json stdout must not render the greeting face box, got: {stdout:?}"
    );
}

#[test]
fn init_piped_stdout_has_no_greeting_or_escape_codes() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_init(dir.path(), &[]);
    assert!(
        output.status.success(),
        "init should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.stdout.contains(&0x1b),
        "piped stdout must contain no ESC bytes"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains('╭') && !stdout.contains('╰'),
        "piped stdout must not render the greeting face box, got: {stdout:?}"
    );
}
