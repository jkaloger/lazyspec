mod common;

use common::TestFixture;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// Write `.lazyspec.toml` with `[orchestration]` agent_users at fixture root.
fn write_orchestration_config(root: &Path, agent_users: &[&str]) {
    let list = agent_users
        .iter()
        .map(|u| format!("\"{}\"", u))
        .collect::<Vec<_>>()
        .join(", ");
    let toml = format!(
        "[orchestration]\nagent_users = [{}]\nclaim_type = \"story\"\n",
        list
    );
    std::fs::write(root.join(".lazyspec.toml"), toml).unwrap();
}

/// Build a fixture with one story doc and `.lazyspec.toml` orchestration block.
/// Returns the fixture and the doc id (e.g. `STORY-001`).
fn assign_project(agent_users: &[&str]) -> (TestFixture, String) {
    let fixture = TestFixture::new();
    write_orchestration_config(fixture.root(), agent_users);
    fixture.write_story("STORY-001-test.md", "Test Story", "draft", None);
    (fixture, "STORY-001".to_string())
}

fn lazyspec_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lazyspec"))
}

/// Read the `assignees` list from the YAML frontmatter of a doc file.
fn read_assignees(path: &Path) -> Vec<String> {
    let content = std::fs::read_to_string(path).unwrap();
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    assert!(parts.len() >= 3, "doc missing frontmatter: {:?}", path);
    let yaml: serde_yaml::Value = serde_yaml::from_str(parts[1]).unwrap();
    yaml.get("assignees")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn assign_appends_default_user() {
    let (fixture, doc_id) = assign_project(&["claude-bot"]);

    let output = lazyspec_bin()
        .args(["assign", &doc_id])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec assign");

    assert!(
        output.status.success(),
        "assign should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let doc_path = fixture.root().join("docs/stories/STORY-001-test.md");
    assert_eq!(read_assignees(&doc_path), vec!["claude-bot".to_string()]);
}

#[test]
fn assign_with_user_flag() {
    let (fixture, doc_id) = assign_project(&[]);

    let output = lazyspec_bin()
        .args(["assign", &doc_id, "--user", "alice"])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec assign");

    assert!(
        output.status.success(),
        "assign --user should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let doc_path = fixture.root().join("docs/stories/STORY-001-test.md");
    assert_eq!(read_assignees(&doc_path), vec!["alice".to_string()]);
}

#[test]
fn assign_json_output() {
    let (fixture, doc_id) = assign_project(&[]);

    let output = lazyspec_bin()
        .args(["assign", &doc_id, "--user", "alice", "--json"])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec assign --json");

    assert!(
        output.status.success(),
        "assign --json should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout should be valid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });

    assert_eq!(parsed["id"], "STORY-001");
    assert_eq!(parsed["assignee_added"], "alice");
    assert_eq!(
        parsed["assignees"],
        serde_json::json!(["alice"]),
        "assignees field should be [alice]"
    );
}

#[test]
fn assign_kicks_listening_socket() {
    let (fixture, doc_id) = assign_project(&[]);

    let sock_dir = fixture.root().join(".lazyspec");
    std::fs::create_dir_all(&sock_dir).unwrap();
    let sock_path = sock_dir.join("daemon.sock");
    let listener = UnixListener::bind(&sock_path).expect("bind daemon.sock");

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let handle = std::thread::spawn(move || {
        let (mut stream, _addr) = listener.accept().expect("accept");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).expect("read kick bytes");
        tx.send(buf).expect("send kick bytes");
    });

    let output = lazyspec_bin()
        .args(["assign", &doc_id, "--user", "alice"])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec assign");

    assert!(
        output.status.success(),
        "assign should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("expected kick bytes on socket within 2s");
    let s = std::str::from_utf8(&bytes).expect("kick payload must be utf-8");
    assert!(s.ends_with('\n'), "wire form must be newline-framed");
    let payload: serde_json::Value =
        serde_json::from_str(s.trim_end()).expect("kick payload must be JSON");
    assert_eq!(payload["type"], "kick", "expected kick payload, got: {s:?}");

    handle.join().expect("listener thread joined");
}

#[test]
fn assign_succeeds_without_socket() {
    let (fixture, doc_id) = assign_project(&[]);

    // No `.lazyspec/daemon.sock` exists. Confirm precondition.
    assert!(!fixture.root().join(".lazyspec/daemon.sock").exists());

    let output = lazyspec_bin()
        .args(["assign", &doc_id, "--user", "alice"])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec assign");

    assert!(
        output.status.success(),
        "assign without daemon socket should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let doc_path = fixture.root().join("docs/stories/STORY-001-test.md");
    assert_eq!(read_assignees(&doc_path), vec!["alice".to_string()]);
}

#[test]
fn filesystem_round_trip_assignees() {
    let (fixture, doc_id) = assign_project(&["claude-bot"]);

    let out1 = lazyspec_bin()
        .args(["assign", &doc_id])
        .current_dir(fixture.root())
        .output()
        .expect("first assign");
    assert!(
        out1.status.success(),
        "first assign should succeed; stderr: {}",
        String::from_utf8_lossy(&out1.stderr)
    );

    let out2 = lazyspec_bin()
        .args(["assign", &doc_id, "--user", "alice"])
        .current_dir(fixture.root())
        .output()
        .expect("second assign");
    assert!(
        out2.status.success(),
        "second assign should succeed; stderr: {}",
        String::from_utf8_lossy(&out2.stderr)
    );

    let doc_path = fixture.root().join("docs/stories/STORY-001-test.md");
    assert_eq!(
        read_assignees(&doc_path),
        vec!["claude-bot".to_string(), "alice".to_string()],
        "both assignees should be present in order"
    );
}
