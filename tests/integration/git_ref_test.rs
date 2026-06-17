use crate::common::TestFixture;
use lazyspec::engine::git_ref::{GitCli, GitRefOps};

#[test]
fn create_ref_commit_and_resolve() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/doc1";

    let sha = git
        .create_ref_commit(fixture.root(), refname, &[("hello.txt", "hello world")])
        .unwrap();

    assert!(!sha.is_empty());

    let resolved = git.resolve_ref(fixture.root(), refname).unwrap();
    assert_eq!(resolved, Some(sha));
}

#[test]
fn read_ref_blob_returns_written_content() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/blob";

    let sha = git
        .create_ref_commit(
            fixture.root(),
            refname,
            &[("data.json", "{\"key\":\"value\"}")],
        )
        .unwrap();

    let content = git
        .read_ref_blob(fixture.root(), &sha, "data.json")
        .unwrap();
    assert_eq!(content, "{\"key\":\"value\"}");
}

#[test]
fn create_ref_commit_with_multiple_files() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/multi";

    let sha = git
        .create_ref_commit(
            fixture.root(),
            refname,
            &[("a.txt", "aaa"), ("b.txt", "bbb")],
        )
        .unwrap();

    let a = git.read_ref_blob(fixture.root(), &sha, "a.txt").unwrap();
    let b = git.read_ref_blob(fixture.root(), &sha, "b.txt").unwrap();
    assert_eq!(a, "aaa");
    assert_eq!(b, "bbb");
}

#[test]
fn update_ref_cas_succeeds_with_correct_old_sha() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/cas";

    let old_sha = git
        .create_ref_commit(fixture.root(), refname, &[("v1.txt", "version 1")])
        .unwrap();

    let new_sha = git
        .create_ref_commit(
            fixture.root(),
            "refs/lazyspec/test/cas-tmp",
            &[("v2.txt", "version 2")],
        )
        .unwrap();

    git.update_ref(fixture.root(), refname, &new_sha, &old_sha)
        .unwrap();

    let resolved = git.resolve_ref(fixture.root(), refname).unwrap();
    assert_eq!(resolved, Some(new_sha));
}

#[test]
fn update_ref_cas_fails_with_wrong_old_sha() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/cas-fail";

    let _sha = git
        .create_ref_commit(fixture.root(), refname, &[("v1.txt", "version 1")])
        .unwrap();

    let new_sha = git
        .create_ref_commit(
            fixture.root(),
            "refs/lazyspec/test/cas-fail-tmp",
            &[("v2.txt", "v2")],
        )
        .unwrap();

    let result = git.update_ref(
        fixture.root(),
        refname,
        &new_sha,
        "0000000000000000000000000000000000000000",
    );
    assert!(result.is_err(), "CAS should fail with wrong old SHA");
}

#[test]
fn list_refs_returns_created_refs() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;

    git.create_ref_commit(fixture.root(), "refs/lazyspec/list/a", &[("a.txt", "a")])
        .unwrap();
    git.create_ref_commit(fixture.root(), "refs/lazyspec/list/b", &[("b.txt", "b")])
        .unwrap();
    git.create_ref_commit(fixture.root(), "refs/lazyspec/other/c", &[("c.txt", "c")])
        .unwrap();

    let refs = git
        .list_refs(fixture.root(), "refs/lazyspec/list/")
        .unwrap();

    let names: Vec<&str> = refs.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"refs/lazyspec/list/a"));
    assert!(names.contains(&"refs/lazyspec/list/b"));
    assert!(!names.contains(&"refs/lazyspec/other/c"));
}

#[test]
fn delete_ref_removes_ref() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/delete";

    git.create_ref_commit(fixture.root(), refname, &[("f.txt", "data")])
        .unwrap();

    let before = git.resolve_ref(fixture.root(), refname).unwrap();
    assert!(before.is_some());

    git.delete_ref(fixture.root(), refname).unwrap();

    let after = git.resolve_ref(fixture.root(), refname).unwrap();
    assert_eq!(after, None);
}

#[test]
fn resolve_ref_returns_none_for_missing_ref() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;

    let result = git
        .resolve_ref(fixture.root(), "refs/lazyspec/nonexistent")
        .unwrap();
    assert_eq!(result, None);
}

#[test]
fn push_and_fetch_round_trip() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/push";

    let sha = git
        .create_ref_commit(fixture.root(), refname, &[("pushed.txt", "remote data")])
        .unwrap();

    git.push_ref(fixture.root(), "origin", refname).unwrap();

    // Delete the local ref to simulate fetching from scratch
    git.delete_ref(fixture.root(), refname).unwrap();
    let gone = git.resolve_ref(fixture.root(), refname).unwrap();
    assert_eq!(gone, None);

    // Fetch it back
    git.fetch_refs(fixture.root(), "origin", refname).unwrap();

    let fetched = git.resolve_ref(fixture.root(), refname).unwrap();
    assert_eq!(fetched, Some(sha));
}

#[test]
fn delete_remote_ref_removes_from_remote() {
    let (fixture, bare_dir) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/delremote";

    git.create_ref_commit(fixture.root(), refname, &[("f.txt", "data")])
        .unwrap();
    git.push_ref(fixture.root(), "origin", refname).unwrap();

    git.delete_remote_ref(fixture.root(), "origin", refname, None)
        .unwrap();

    // Verify ref is gone from remote via ls-remote on the bare repo
    git.delete_ref(fixture.root(), refname).unwrap();
    let output = std::process::Command::new("git")
        .args([
            "ls-remote",
            "--refs",
            bare_dir.path().to_str().unwrap(),
            refname,
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(refname),
        "ref should not exist on remote after deletion"
    );
}

#[test]
fn update_creates_chained_commit() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/chain";

    let original_sha = git
        .create_ref_commit(fixture.root(), refname, &[("doc.md", "version 1")])
        .unwrap();

    let updated_sha = git
        .create_commit(
            fixture.root(),
            refname,
            &[("doc.md", "version 2")],
            Some(&original_sha),
        )
        .unwrap();

    let output = std::process::Command::new("git")
        .args(["cat-file", "-p", &updated_sha])
        .current_dir(fixture.root())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains(&format!("parent {}", original_sha)),
        "new commit should have parent pointing to original SHA, got: {}",
        stdout
    );
}

#[test]
fn push_ref_with_lease_succeeds_when_remote_matches() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/lease-push";

    let sha = git
        .create_ref_commit(fixture.root(), refname, &[("v1.txt", "version 1")])
        .unwrap();

    // Push normally first to establish remote ref
    git.push_ref(fixture.root(), "origin", refname).unwrap();

    // Create a new commit and update local ref
    let new_sha = git
        .create_commit(
            fixture.root(),
            refname,
            &[("v2.txt", "version 2")],
            Some(&sha),
        )
        .unwrap();
    git.update_ref(fixture.root(), refname, &new_sha, &sha)
        .unwrap();

    // Push with lease using the correct expected old SHA
    git.push_ref_with_lease(fixture.root(), "origin", refname, &new_sha, Some(&sha))
        .unwrap();

    // Verify remote has the new SHA by fetching
    git.delete_ref(fixture.root(), refname).unwrap();
    git.fetch_refs(fixture.root(), "origin", refname).unwrap();
    let fetched = git.resolve_ref(fixture.root(), refname).unwrap();
    assert_eq!(fetched, Some(new_sha));
}

#[test]
fn push_ref_with_lease_pushes_dangling_commit_without_local_ref() {
    // Regression: LeaseEngine::acquire and ::heartbeat mint a dangling commit
    // (no local ref) and push it. Earlier impl pushed by refname, which failed
    // with "src refspec ... does not match any" for acquire and silently no-op'd
    // for heartbeat (local ref still at old sha). Push must use <sha>:<refname>.
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/leases/iteration/ITERATION-REG";

    let new_sha = git
        .create_commit(fixture.root(), refname, &[("lease.json", "{}")], None)
        .unwrap();

    // Local ref must NOT exist at this point -- this is the precondition of acquire.
    assert_eq!(
        git.resolve_ref(fixture.root(), refname).unwrap(),
        None,
        "test precondition: local ref must be absent"
    );

    let zero = "0000000000000000000000000000000000000000";
    git.push_ref_with_lease(fixture.root(), "origin", refname, &new_sha, Some(zero))
        .unwrap();

    // Verify the dangling commit landed on the remote.
    git.fetch_refs(fixture.root(), "origin", refname).unwrap();
    assert_eq!(
        git.resolve_ref(fixture.root(), refname).unwrap(),
        Some(new_sha)
    );
}

#[test]
fn push_ref_with_lease_pushes_new_sha_not_local_ref() {
    // Regression for heartbeat: local ref still points at old sha when push fires.
    // Earlier impl pushed by refname (i.e. old sha), so remote stayed stale even
    // though local later advanced via update_ref. Push must carry new_sha.
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/leases/iteration/ITERATION-HB";

    let old_sha = git
        .create_ref_commit(fixture.root(), refname, &[("lease.json", "{\"v\":1}")])
        .unwrap();
    git.push_ref(fixture.root(), "origin", refname).unwrap();

    // Mint a dangling new commit. Local ref still at old_sha.
    let new_sha = git
        .create_commit(
            fixture.root(),
            refname,
            &[("lease.json", "{\"v\":2}")],
            Some(&old_sha),
        )
        .unwrap();
    assert_eq!(
        git.resolve_ref(fixture.root(), refname).unwrap(),
        Some(old_sha.clone()),
        "test precondition: local ref must still be at old_sha"
    );

    git.push_ref_with_lease(fixture.root(), "origin", refname, &new_sha, Some(&old_sha))
        .unwrap();

    // Remote must now hold new_sha (not old_sha).
    git.delete_ref(fixture.root(), refname).unwrap();
    git.fetch_refs(fixture.root(), "origin", refname).unwrap();
    assert_eq!(
        git.resolve_ref(fixture.root(), refname).unwrap(),
        Some(new_sha)
    );
}

#[test]
fn push_ref_with_lease_fails_when_remote_changed() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/lease-fail";

    let sha = git
        .create_ref_commit(fixture.root(), refname, &[("v1.txt", "version 1")])
        .unwrap();

    // Push to establish remote ref
    git.push_ref(fixture.root(), "origin", refname).unwrap();

    // Simulate another agent: create a different commit and push it (changing the remote)
    let interloper_sha = git
        .create_commit(fixture.root(), refname, &[("v3.txt", "sneaky")], Some(&sha))
        .unwrap();
    git.update_ref(fixture.root(), refname, &interloper_sha, &sha)
        .unwrap();
    git.push_ref(fixture.root(), "origin", refname).unwrap();

    // Now create our own commit based on the original sha (simulating stale local state)
    let new_sha = git
        .create_commit(
            fixture.root(),
            refname,
            &[("v2.txt", "version 2")],
            Some(&sha),
        )
        .unwrap();
    git.update_ref(fixture.root(), refname, &new_sha, &interloper_sha)
        .unwrap();

    // Push with lease using the stale expected old SHA -- should fail because
    // remote is at interloper_sha, not sha
    let result = git.push_ref_with_lease(fixture.root(), "origin", refname, &new_sha, Some(&sha));
    assert!(
        result.is_err(),
        "push_ref_with_lease should fail when remote ref was changed by another agent"
    );
}

#[test]
fn read_commit_timestamp_returns_correct_time() {
    use chrono::Utc;

    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/timestamp";

    let before = Utc::now();
    let sha = git
        .create_ref_commit(fixture.root(), refname, &[("f.txt", "data")])
        .unwrap();
    let after = Utc::now();

    let ts = git.read_commit_timestamp(fixture.root(), &sha).unwrap();

    assert!(
        ts.timestamp() >= before.timestamp() - 2 && ts.timestamp() <= after.timestamp() + 2,
        "commit timestamp {} should be close to now (between {} and {})",
        ts,
        before,
        after
    );
}

#[test]
fn create_ref_commit_fails_if_ref_exists() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let refname = "refs/lazyspec/test/cas-create";

    git.create_ref_commit(fixture.root(), refname, &[("v1.txt", "first")])
        .unwrap();

    let result = git.create_ref_commit(fixture.root(), refname, &[("v2.txt", "second")]);
    assert!(
        result.is_err(),
        "second create_ref_commit on same refname should fail due to CAS"
    );
}

#[test]
fn heartbeat_succeeds_and_extends_expiry() {
    use chrono::{DateTime, Duration, Utc};
    use lazyspec::engine::config::CoordinationConfig;
    use lazyspec::engine::lease::LeaseEngine;

    let (fixture, _bare) = TestFixture::with_git_remote();
    let git = GitCli;
    let config = CoordinationConfig {
        remote: "origin".to_string(),
        lease_duration: "60m".to_string(),
        grace_period: "2m".to_string(),
        max_push_retries: 5,
        max_clock_skew: "5m".to_string(),
    };

    let engine = LeaseEngine::new(git, config.clone());
    // Truncate to seconds to match serde ts_seconds serialization
    let now = DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap();
    let lease = engine
        .acquire(fixture.root(), "story", "STORY-001", "agent-a", now)
        .unwrap();

    let heartbeat_time = now + Duration::minutes(30);
    let git2 = GitCli;
    let engine2 = LeaseEngine::new(git2, config);

    // Fetch the ref before heartbeat (simulating a fresh client)
    let updated = engine2
        .heartbeat(
            fixture.root(),
            "story",
            "STORY-001",
            "agent-a",
            heartbeat_time,
        )
        .unwrap();

    assert_eq!(updated.agent, "agent-a");
    assert_eq!(updated.acquired, lease.acquired);
    assert!(
        updated.expires > lease.expires,
        "heartbeat should extend expiry: old={}, new={}",
        lease.expires,
        updated.expires
    );
    assert_eq!(updated.expires, heartbeat_time + Duration::minutes(60));

    // Verify the ref was actually updated (no CAS error occurred)
    let git3 = GitCli;
    let refname = "refs/lazyspec/leases/story/STORY-001";
    let sha = git3.resolve_ref(fixture.root(), refname).unwrap();
    assert!(sha.is_some(), "ref should still exist after heartbeat");

    // Read the lease back and verify content
    let blob = git3
        .read_ref_blob(fixture.root(), &sha.unwrap(), "lease.json")
        .unwrap();
    let stored_lease: lazyspec::engine::lease::Lease = serde_json::from_str(&blob).unwrap();
    assert_eq!(stored_lease.expires, updated.expires);
}
