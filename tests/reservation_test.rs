mod common;

use common::TestFixture;
use lazyspec::engine::reservation::reserve_next;
use std::process::Command;

fn seed_ref_on_bare(bare_path: &std::path::Path, prefix: &str, num: u32) {
    let sha_output = Command::new("git")
        .args(["hash-object", "-w", "-t", "blob", "--stdin"])
        .stdin(std::process::Stdio::null())
        .current_dir(bare_path)
        .output()
        .expect("hash-object in bare repo");
    let sha = String::from_utf8_lossy(&sha_output.stdout).trim().to_string();

    let refname = format!("refs/reservations/{prefix}/{num}");
    Command::new("git")
        .args(["update-ref", &refname, &sha])
        .current_dir(bare_path)
        .output()
        .expect("update-ref in bare repo");
}

fn install_pre_receive_hook(bare_path: &std::path::Path, script: &str) {
    let hooks_dir = bare_path.join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    let hook_path = hooks_dir.join("pre-receive");
    std::fs::write(&hook_path, script).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn ref_exists_on_remote(repo_root: &std::path::Path, remote: &str, refname: &str) -> bool {
    let output = Command::new("git")
        .args(["ls-remote", "--refs", remote, refname])
        .current_dir(repo_root)
        .output()
        .expect("ls-remote check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.contains(refname)
}

fn local_ref_exists(repo_root: &std::path::Path, prefix: &str, num: u32) -> bool {
    let ref_path = repo_root
        .join(".git/refs/reservations")
        .join(prefix)
        .join(num.to_string());
    ref_path.exists()
}

#[test]
fn successful_reservation_returns_1_and_pushes_ref() {
    let (fixture, _bare) = TestFixture::with_git_remote();

    let result = reserve_next(fixture.root(), "origin", "RFC", 5).unwrap();
    assert_eq!(result, 1);

    assert!(
        ref_exists_on_remote(fixture.root(), "origin", "refs/reservations/RFC/1"),
        "ref should exist on remote after reservation"
    );
}

#[test]
fn increments_from_existing_refs() {
    let (fixture, bare) = TestFixture::with_git_remote();

    seed_ref_on_bare(bare.path(), "RFC", 3);

    let result = reserve_next(fixture.root(), "origin", "RFC", 5).unwrap();
    assert_eq!(result, 4);

    assert!(ref_exists_on_remote(
        fixture.root(),
        "origin",
        "refs/reservations/RFC/4"
    ));
}

#[test]
fn retry_on_conflict_succeeds_on_second_attempt() {
    let (fixture, bare) = TestFixture::with_git_remote();

    // Hook rejects the first push, accepts subsequent ones.
    // Uses a counter file to track attempts.
    let counter_path = bare.path().join("push_count");
    std::fs::write(&counter_path, "0").unwrap();

    let script = format!(
        r#"#!/bin/sh
COUNTER_FILE="{counter}"
COUNT=$(cat "$COUNTER_FILE")
COUNT=$((COUNT + 1))
echo "$COUNT" > "$COUNTER_FILE"
if [ "$COUNT" -le 1 ]; then
    echo "rejected: simulated conflict" >&2
    exit 1
fi
exit 0
"#,
        counter = counter_path.display()
    );
    install_pre_receive_hook(bare.path(), &script);

    let result = reserve_next(fixture.root(), "origin", "RFC", 5).unwrap();

    // First attempt (candidate=1) rejected, cleanup + increment to 2, second attempt succeeds
    assert_eq!(result, 2);
}

#[test]
fn exhausted_retries_returns_error() {
    let (fixture, bare) = TestFixture::with_git_remote();

    // Hook always rejects
    let script = r#"#!/bin/sh
echo "rejected: always fail" >&2
exit 1
"#;
    install_pre_receive_hook(bare.path(), script);

    let result = reserve_next(fixture.root(), "origin", "RFC", 3);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Failed to reserve") && err_msg.contains("3 attempts"),
        "error should mention exhausted retries with attempt count, got: {err_msg}"
    );
}

#[test]
fn unreachable_remote_fails_with_hint() {
    let (fixture, _bare) = TestFixture::with_git_remote();

    let result = reserve_next(fixture.root(), "nonexistent_remote", "RFC", 5);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("incremental") || err_msg.contains("sqids"),
        "error should suggest --numbering incremental or --numbering sqids, got: {err_msg}"
    );
}

#[test]
fn pre_computed_id_passthrough() {
    use lazyspec::engine::template::resolve_filename;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let filename = resolve_filename(
        "{type}-{n:03}-{title}.md",
        "rfc",
        "My Feature",
        dir.path(),
        None,
        Some("042"),
    );

    assert_eq!(filename, "RFC-042-my-feature.md");
}

#[test]
fn cleanup_after_failed_push_removes_local_ref() {
    let (fixture, bare) = TestFixture::with_git_remote();

    // Hook always rejects
    let script = r#"#!/bin/sh
echo "rejected: always fail" >&2
exit 1
"#;
    install_pre_receive_hook(bare.path(), script);

    let _ = reserve_next(fixture.root(), "origin", "RFC", 3);

    // After exhausting retries, no dangling local refs should remain
    // Candidates tried: 1, 2, 3
    for n in 1..=3 {
        assert!(
            !local_ref_exists(fixture.root(), "RFC", n),
            "local ref for candidate {n} should have been cleaned up"
        );
    }
}
