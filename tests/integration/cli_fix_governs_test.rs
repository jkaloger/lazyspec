//! `fix --governs` through the CLI surface: the `{"governs": [...]}` envelope
//! agents parse, the lines a human reads, and the exit code a repair script
//! branches on.

use std::path::Path;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use lazyspec::engine::fs::{FileSystem, RealFileSystem};
use lazyspec::engine::git_ref::GitRefOps;
use lazyspec::engine::staleness::Drift;

use crate::common::TestFixture;

const REVIEWED: &str = "0123456789abcdef0123456789abcdef01234567";
const DOC: &str = "docs/rfcs/RFC-001-engine.md";

/// Git as the zero-match rule uses it: renames between the document's
/// `reviewed` commit and `HEAD`, and nothing else. Every other operation is
/// unreachable from `fix --governs`, so reaching one is a test failure rather
/// than a value to stub.
struct RenamingGit(Vec<(String, String)>);

impl RenamingGit {
    fn new(pairs: &[(&str, &str)]) -> Self {
        Self(
            pairs
                .iter()
                .map(|(f, t)| (f.to_string(), t.to_string()))
                .collect(),
        )
    }
}

impl GitRefOps for RenamingGit {
    fn renames(&self, _root: &Path, _from: &str, _to: &str) -> Result<Vec<(String, String)>> {
        Ok(self.0.clone())
    }
    fn diff_stat(&self, _root: &Path, _from: &str, _to: &str, _paths: &[String]) -> Result<Drift> {
        unreachable!("fix --governs diffs nothing")
    }
    fn resolve_ref(&self, _root: &Path, _refname: &str) -> Result<Option<String>> {
        unreachable!("fix --governs resolves no refs")
    }
    fn list_refs(&self, _root: &Path, _pattern: &str) -> Result<Vec<(String, String)>> {
        unreachable!("fix --governs lists no refs")
    }
    fn read_ref_blob(&self, _root: &Path, _sha: &str, _path: &str) -> Result<String> {
        unreachable!("fix --governs reads no blobs")
    }
    fn create_commit(
        &self,
        _root: &Path,
        _refname: &str,
        _files: &[(&str, &str)],
        _parent: Option<&str>,
    ) -> Result<String> {
        unreachable!("fix --governs commits nothing")
    }
    fn create_ref_commit(
        &self,
        _root: &Path,
        _refname: &str,
        _files: &[(&str, &str)],
    ) -> Result<String> {
        unreachable!("fix --governs commits nothing")
    }
    fn update_ref(&self, _root: &Path, _r: &str, _new: &str, _old: &str) -> Result<()> {
        unreachable!("fix --governs updates no refs")
    }
    fn delete_ref(&self, _root: &Path, _refname: &str) -> Result<()> {
        unreachable!("fix --governs deletes no refs")
    }
    fn fetch_refs(&self, _root: &Path, _remote: &str, _pattern: &str) -> Result<()> {
        unreachable!("fix --governs fetches nothing")
    }
    fn update_clone(&self, _clone: &Path, _branch: Option<&str>) -> Result<()> {
        unreachable!("fix --governs updates no clones")
    }
    fn clone_repo(&self, _remote: &str, _branch: Option<&str>, _dest: &Path) -> Result<()> {
        unreachable!("fix --governs clones nothing")
    }
    fn commit_and_push(&self, _clone: &Path, _branch: Option<&str>, _message: &str) -> Result<()> {
        unreachable!("fix --governs commits nothing")
    }
    fn push_ref(&self, _root: &Path, _remote: &str, _refname: &str) -> Result<()> {
        unreachable!("fix --governs pushes nothing")
    }
    fn push_new_ref(&self, _root: &Path, _rem: &str, _r: &str, _sha: &str) -> Result<()> {
        unreachable!("fix --governs pushes nothing")
    }
    fn delete_remote_ref(
        &self,
        _root: &Path,
        _remote: &str,
        _refname: &str,
        _expected_old: Option<&str>,
    ) -> Result<()> {
        unreachable!("fix --governs touches no remote")
    }
    fn push_ref_with_lease(
        &self,
        _root: &Path,
        _remote: &str,
        _refname: &str,
        _new_sha: &str,
        _expected_old: Option<&str>,
    ) -> Result<()> {
        unreachable!("fix --governs touches no remote")
    }
    fn read_commit_timestamp(&self, _root: &Path, _sha: &str) -> Result<DateTime<Utc>> {
        unreachable!("fix --governs reads no timestamps")
    }
    fn head(&self, _root: &Path) -> Result<String> {
        Ok(REVIEWED.to_string())
    }
}

/// A filesystem whose writes always fail: a read-only checkout, a permission
/// the process does not have. Reads pass through, so the document still loads
/// and still reports its rotted pin.
struct UnwritableFs;

impl FileSystem for UnwritableFs {
    fn write(&self, path: &Path, _contents: &str) -> Result<()> {
        Err(anyhow!("permission denied: {}", path.display()))
    }
    fn read_to_string(&self, path: &Path) -> Result<String> {
        RealFileSystem.read_to_string(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        RealFileSystem.rename(from, to)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<std::path::PathBuf>> {
        RealFileSystem.read_dir(path)
    }
    fn exists(&self, path: &Path) -> bool {
        RealFileSystem.exists(path)
    }
    fn create_dir_all(&self, path: &Path) -> Result<()> {
        RealFileSystem.create_dir_all(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        RealFileSystem.is_dir(path)
    }
}

/// One rfc pinning `src/old/**` beside a pin that still matches, over a tree
/// where `src/old` is gone and `src/context` is where git says it went.
fn rotted_project() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_doc(
        DOC,
        &format!(
            "---\ntitle: \"Engine\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\ngoverns:\n- \"src/old/**\"\n- \"src/engine/**\"\nreviewed: {REVIEWED}\nrelated: []\n---\n\nbody\n"
        ),
    );
    std::fs::create_dir_all(fixture.root().join("src/engine")).unwrap();
    std::fs::create_dir_all(fixture.root().join("src/context")).unwrap();
    std::fs::write(fixture.root().join("src/engine/store.rs"), "fn a() {}\n").unwrap();
    std::fs::write(fixture.root().join("src/context/resolve.rs"), "fn b() {}\n").unwrap();
    fixture
}

fn moved_git() -> RenamingGit {
    RenamingGit::new(&[("src/old/resolve.rs", "src/context/resolve.rs")])
}

#[test]
fn json_wraps_every_rewrite_in_a_governs_array() {
    let fixture = rotted_project();

    let json = lazyspec::cli::fix::run_governs_json(
        fixture.root(),
        &fixture.store(),
        &moved_git(),
        false,
        &RealFileSystem,
    );

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({
            "governs": [{
                "path": DOC,
                "old_glob": "src/old/**",
                "new_glob": "src/context/**",
                "written": true,
                "error": null,
            }]
        })
    );
}

#[test]
fn a_repo_with_no_rotted_pin_reports_an_empty_array_and_succeeds() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-engine.md", "Engine", "draft");

    let json = lazyspec::cli::fix::run_governs_json(
        fixture.root(),
        &fixture.store(),
        &moved_git(),
        false,
        &RealFileSystem,
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({ "governs": [] })
    );

    let code = lazyspec::cli::fix::run_governs(
        fixture.root(),
        &fixture.store(),
        &fixture.config(),
        &moved_git(),
        false,
        false,
        &RealFileSystem,
    );
    assert_eq!(
        code, 0,
        "nothing to repair is a healthy repo, not a failure"
    );
}

#[test]
fn the_human_line_names_the_document_and_both_globs() {
    let fixture = rotted_project();

    let output = lazyspec::cli::fix::run_governs_human(
        fixture.root(),
        &fixture.store(),
        &moved_git(),
        false,
        &RealFileSystem,
    );

    assert_eq!(
        output,
        format!("Repinned {DOC}: src/old/** -> src/context/**\n")
    );
}

#[test]
fn a_dry_run_says_would_repin() {
    let fixture = rotted_project();

    let output = lazyspec::cli::fix::run_governs_human(
        fixture.root(),
        &fixture.store(),
        &moved_git(),
        true,
        &RealFileSystem,
    );

    assert_eq!(
        output,
        format!("Would repin {DOC}: src/old/** -> src/context/**\n")
    );
}

#[test]
fn a_failed_write_is_reported_as_an_error_not_as_a_repin() {
    let fixture = rotted_project();

    let output = lazyspec::cli::fix::run_governs_human(
        fixture.root(),
        &fixture.store(),
        &moved_git(),
        false,
        &UnwritableFs,
    );

    assert!(
        output.starts_with(&format!("error: could not repin {DOC}: permission denied")),
        "the failure must say why, got: {output}"
    );
    assert!(
        !output.contains("Repinned"),
        "nothing was written, so nothing was repinned, got: {output}"
    );
}

#[test]
fn a_failed_write_carries_its_reason_into_json() {
    let fixture = rotted_project();

    let json = lazyspec::cli::fix::run_governs_json(
        fixture.root(),
        &fixture.store(),
        &moved_git(),
        false,
        &UnwritableFs,
    );

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entry = &value["governs"][0];
    assert_eq!(entry["written"], serde_json::json!(false));
    assert!(
        entry["error"]
            .as_str()
            .unwrap()
            .contains("permission denied"),
        "the error belongs in the payload agents read, got: {entry}"
    );
}

#[test]
fn a_failed_write_exits_nonzero() {
    let fixture = rotted_project();

    let code = lazyspec::cli::fix::run_governs(
        fixture.root(),
        &fixture.store(),
        &fixture.config(),
        &moved_git(),
        false,
        false,
        &UnwritableFs,
    );

    assert_eq!(code, 1, "the caller asked for a repair and did not get one");
}
