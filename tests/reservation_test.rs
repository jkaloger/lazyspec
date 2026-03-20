mod common;

use common::TestFixture;
use lazyspec::engine::config::{ReservedConfig, ReservedFormat};
use lazyspec::engine::reservation::{list_reservations, reserve_next};
use lazyspec::engine::store::Store;
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

    let result = reserve_next(fixture.root(), "origin", "RFC", 5, fixture.root(), |_| {}).unwrap();
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

    let result = reserve_next(fixture.root(), "origin", "RFC", 5, fixture.root(), |_| {}).unwrap();
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

    let result = reserve_next(fixture.root(), "origin", "RFC", 5, fixture.root(), |_| {}).unwrap();

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

    let result = reserve_next(fixture.root(), "origin", "RFC", 3, fixture.root(), |_| {});
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

    let result = reserve_next(fixture.root(), "nonexistent_remote", "RFC", 5, fixture.root(), |_| {});
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

    let _ = reserve_next(fixture.root(), "origin", "RFC", 3, fixture.root(), |_| {});

    // After exhausting retries, no dangling local refs should remain
    // Candidates tried: 1, 2, 3
    for n in 1..=3 {
        assert!(
            !local_ref_exists(fixture.root(), "RFC", n),
            "local ref for candidate {n} should have been cleaned up"
        );
    }
}

fn config_with_reserved() -> lazyspec::engine::config::Config {
    let mut config = lazyspec::engine::config::Config::default();
    config.reserved = Some(ReservedConfig {
        remote: "origin".to_string(),
        format: ReservedFormat::Incremental,
        max_retries: 5,
    });
    config
}

// AC-1: list displays all reservation refs with correct type, number, and ref path
#[test]
fn list_shows_all_reservations() {
    let (fixture, bare) = TestFixture::with_git_remote();

    seed_ref_on_bare(bare.path(), "RFC", 1);
    seed_ref_on_bare(bare.path(), "RFC", 3);
    seed_ref_on_bare(bare.path(), "STORY", 5);

    let reservations = list_reservations(fixture.root(), "origin", |_| {}).unwrap();

    assert_eq!(reservations.len(), 3, "should return all 3 refs");

    let has = |prefix: &str, number: u32| {
        reservations.iter().any(|r| {
            r.prefix == prefix
                && r.number == number
                && r.ref_path == format!("refs/reservations/{prefix}/{number}")
        })
    };

    assert!(has("RFC", 1), "should contain RFC/1");
    assert!(has("RFC", 3), "should contain RFC/3");
    assert!(has("STORY", 5), "should contain STORY/5");
}

// AC-2: list --json outputs structured JSON with prefix, number, ref_path keys
#[test]
fn list_json_output_is_structured() {
    let (fixture, bare) = TestFixture::with_git_remote();

    seed_ref_on_bare(bare.path(), "RFC", 1);
    seed_ref_on_bare(bare.path(), "RFC", 3);
    seed_ref_on_bare(bare.path(), "STORY", 5);

    let reservations = list_reservations(fixture.root(), "origin", |_| {}).unwrap();
    let json_str = serde_json::to_string_pretty(&reservations).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed.len(), 3);
    for entry in &parsed {
        assert!(entry.get("prefix").is_some(), "entry missing 'prefix' key");
        assert!(entry.get("number").is_some(), "entry missing 'number' key");
        assert!(
            entry.get("ref_path").is_some(),
            "entry missing 'ref_path' key"
        );
    }
}

// AC-3: prune deletes refs when matching documents exist locally
#[test]
fn prune_deletes_refs_with_matching_documents() {
    let (fixture, bare) = TestFixture::with_git_remote();
    let config = config_with_reserved();

    seed_ref_on_bare(bare.path(), "RFC", 42);
    fixture.write_rfc("RFC-042-some-title.md", "Some Title", "draft");

    let store = Store::load(fixture.root(), &config).unwrap();
    lazyspec::cli::reservations::run_prune(fixture.root(), &config, &store, false, false, |_| {}).unwrap();

    assert!(
        !ref_exists_on_remote(fixture.root(), "origin", "refs/reservations/RFC/42"),
        "ref should be deleted after prune"
    );
}

// AC-4: prune flags orphans (no matching doc) without deleting
#[test]
fn prune_flags_orphans_without_deleting() {
    let (fixture, bare) = TestFixture::with_git_remote();
    let config = config_with_reserved();

    seed_ref_on_bare(bare.path(), "RFC", 99);

    let store = Store::load(fixture.root(), &config).unwrap();
    lazyspec::cli::reservations::run_prune(fixture.root(), &config, &store, false, false, |_| {}).unwrap();

    assert!(
        ref_exists_on_remote(fixture.root(), "origin", "refs/reservations/RFC/99"),
        "orphan ref should not be deleted"
    );
}

// AC-5: prune --dry-run does not delete refs
#[test]
fn prune_dry_run_does_not_delete() {
    let (fixture, bare) = TestFixture::with_git_remote();
    let config = config_with_reserved();

    seed_ref_on_bare(bare.path(), "RFC", 42);
    fixture.write_rfc("RFC-042-some-title.md", "Some Title", "draft");

    let store = Store::load(fixture.root(), &config).unwrap();
    lazyspec::cli::reservations::run_prune(fixture.root(), &config, &store, true, false, |_| {}).unwrap();

    assert!(
        ref_exists_on_remote(fixture.root(), "origin", "refs/reservations/RFC/42"),
        "ref should still exist after dry-run prune"
    );
}

// AC-6: prune --json outputs structured JSON with pruned and orphaned arrays
#[test]
fn prune_json_output_is_structured() {
    let (fixture, bare) = TestFixture::with_git_remote();

    seed_ref_on_bare(bare.path(), "RFC", 42);
    seed_ref_on_bare(bare.path(), "RFC", 99);
    fixture.write_rfc("RFC-042-some-title.md", "Some Title", "draft");

    // Write .lazyspec.toml with reserved numbering so the binary can load config
    let toml_content = r#"
[numbering.reserved]
remote = "origin"
format = "incremental"
max_retries = 5
"#;
    std::fs::write(fixture.root().join(".lazyspec.toml"), toml_content).unwrap();

    let binary = env!("CARGO_BIN_EXE_lazyspec");
    let output = Command::new(binary)
        .args(["reservations", "prune", "--json"])
        .current_dir(fixture.root())
        .output()
        .expect("failed to run lazyspec");

    assert!(output.status.success(), "prune --json should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("output should be valid JSON: {e}\nstdout: {stdout}"));

    let pruned = parsed.get("pruned").expect("missing 'pruned' key");
    let orphaned = parsed.get("orphaned").expect("missing 'orphaned' key");
    let errors = parsed.get("errors").expect("missing 'errors' key");

    let pruned_arr = pruned.as_array().expect("'pruned' should be an array");
    let orphaned_arr = orphaned.as_array().expect("'orphaned' should be an array");
    let errors_arr = errors.as_array().expect("'errors' should be an array");

    assert_eq!(pruned_arr.len(), 1, "should have 1 pruned ref");
    assert_eq!(orphaned_arr.len(), 1, "should have 1 orphaned ref");
    assert_eq!(errors_arr.len(), 0, "should have no errors");

    assert_eq!(pruned_arr[0]["number"], 42);
    assert_eq!(orphaned_arr[0]["number"], 99);
}

#[test]
fn local_files_seed_higher_candidate() {
    let (fixture, bare) = TestFixture::with_git_remote();

    // Remote has RFC/3
    seed_ref_on_bare(bare.path(), "RFC", 3);

    // Local dir has RFC-010-something.md (local max = 10)
    let docs_dir = fixture.root().join("docs_rfcs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(docs_dir.join("RFC-010-something.md"), "").unwrap();

    let result = reserve_next(fixture.root(), "origin", "RFC", 5, &docs_dir, |_| {}).unwrap();
    assert_eq!(result, 11, "should start from local max (10) + 1, not remote max (3) + 1");
}

#[test]
fn remote_wins_when_higher_than_local() {
    let (fixture, bare) = TestFixture::with_git_remote();

    // Remote has RFC/20
    seed_ref_on_bare(bare.path(), "RFC", 20);

    // Local dir has RFC-005-something.md (local max = 5)
    let docs_dir = fixture.root().join("docs_rfcs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(docs_dir.join("RFC-005-something.md"), "").unwrap();

    let result = reserve_next(fixture.root(), "origin", "RFC", 5, &docs_dir, |_| {}).unwrap();
    assert_eq!(result, 21, "should start from remote max (20) + 1, not local max (5) + 1");
}
