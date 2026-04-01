mod common;

use common::TestFixture;
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
        .create_ref_commit(fixture.root(), refname, &[("data.json", "{\"key\":\"value\"}")])
        .unwrap();

    let content = git.read_ref_blob(fixture.root(), &sha, "data.json").unwrap();
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
        .create_ref_commit(fixture.root(), "refs/lazyspec/test/cas-tmp", &[("v2.txt", "version 2")])
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
        .create_ref_commit(fixture.root(), "refs/lazyspec/test/cas-fail-tmp", &[("v2.txt", "v2")])
        .unwrap();

    let result = git.update_ref(fixture.root(), refname, &new_sha, "0000000000000000000000000000000000000000");
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

    git.delete_remote_ref(fixture.root(), "origin", refname)
        .unwrap();

    // Verify ref is gone from remote via ls-remote on the bare repo
    git.delete_ref(fixture.root(), refname).unwrap();
    let output = std::process::Command::new("git")
        .args(["ls-remote", "--refs", bare_dir.path().to_str().unwrap(), refname])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(refname),
        "ref should not exist on remote after deletion"
    );
}
