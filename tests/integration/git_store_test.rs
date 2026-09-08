//! STORY-281: a `git` type clones its remote on first read and reads from the
//! clone thereafter. The remote is a plain local repository in a `TempDir`, so
//! no test here reaches the network.

use lazyspec::engine::config::{Config, StoreBackend, TypeDef};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::store::{doc_root, Filter, Store};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_rfc(repo: &Path, filename: &str, title: &str) {
    let dir = repo.join("docs/rfcs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(filename),
        format!(
            "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: tester\ndate: 2026-04-01\ntags: []\n---\n\n## Summary\n{title}.\n"
        ),
    )
    .unwrap();
}

/// A repository with `RFC-001-a.md` on `main` and `RFC-002-b.md` added on
/// `next`. `main` is checked out, so it is what a clone with no `branch` gets.
fn shared_repo() -> TempDir {
    let repo = TempDir::new().unwrap();
    let path = repo.path();
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "test@test.com"]);
    git(path, &["config", "user.name", "Test"]);
    write_rfc(path, "RFC-001-a.md", "A");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-m", "one"]);
    git(path, &["checkout", "-b", "next"]);
    write_rfc(path, "RFC-002-b.md", "B");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-m", "two"]);
    git(path, &["checkout", "main"]);
    repo
}

fn git_config(remote: &Path, branch: Option<&str>) -> Config {
    let mut config = Config::default();
    // Replace the starter types: the default `rfc` is filesystem-backed and
    // `type_by_name` would resolve to it ahead of the git one.
    config.documents.types = vec![TypeDef {
        dir: "docs/rfcs".to_string(),
        remote: Some(remote.to_string_lossy().into_owned()),
        branch: branch.map(str::to_string),
        ..TypeDef::test_fixture("rfc", StoreBackend::Git)
    }];
    config
}

fn ids(store: &Store) -> Vec<String> {
    let mut ids: Vec<String> = store
        .list(&Filter::default())
        .into_iter()
        .map(|d| d.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn first_read_clones_remote_and_lists_docs_under_doc_root() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), None);

    let store = Store::load(root, &config).unwrap();

    assert!(root
        .join(".lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md")
        .exists());
    assert_eq!(ids(&store), vec!["RFC-001"]);
    let type_def = &config.documents.types.last().unwrap();
    let doc = &store.list(&Filter::default())[0];
    assert!(
        root.join(&doc.path).starts_with(doc_root(root, type_def)),
        "{} is under {}",
        doc.path.display(),
        doc_root(root, type_def).display()
    );
}

#[test]
fn branch_selects_what_the_clone_checks_out() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();

    let store = Store::load(project.path(), &git_config(remote.path(), Some("next"))).unwrap();

    assert_eq!(ids(&store), vec!["RFC-001", "RFC-002"]);
}

#[test]
fn existing_clone_is_read_when_the_remote_is_gone() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let config = git_config(remote.path(), None);
    Store::load(project.path(), &config).unwrap();

    remote.close().unwrap();
    let store = Store::load(project.path(), &config).unwrap();

    assert_eq!(ids(&store), vec!["RFC-001"]);
}

#[test]
fn first_read_gitignores_the_cache() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();

    Store::load(project.path(), &git_config(remote.path(), None)).unwrap();

    let gitignore = std::fs::read_to_string(project.path().join(".lazyspec/.gitignore")).unwrap();
    assert!(gitignore.lines().any(|l| l == "cache/"), "{gitignore:?}");
}

#[test]
fn unclonable_remote_error_names_remote_and_default_branch() {
    let not_a_repo = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();

    let Err(err) = Store::load(project.path(), &git_config(not_a_repo.path(), None)) else {
        panic!("cloning a directory that is not a repository fails");
    };

    let msg = format!("{err:#}");
    assert!(msg.contains(&*not_a_repo.path().to_string_lossy()), "{msg}");
    assert!(msg.contains("default branch"), "{msg}");
}

fn assert_refused<T>(command: &str, result: anyhow::Result<T>) {
    let Err(err) = result else {
        panic!("{command} on a git type is refused");
    };
    let msg = format!("{err:#}");
    assert!(msg.contains("git"), "{command}: {msg}");
    assert!(msg.contains("not yet supported"), "{command}: {msg}");
}

#[test]
fn every_write_to_a_git_type_is_refused_before_the_clone_is_touched() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone_doc = root.join(".lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md");
    let before = std::fs::read(&clone_doc).unwrap();
    let fs = RealFileSystem;

    assert_refused(
        "create",
        lazyspec::cli::create::run(root, &config, &store, "rfc", "C", "tester", |_| {}),
    );
    assert_refused(
        "update",
        lazyspec::engine::ops::update::run_with_config(
            root,
            &store,
            "RFC-001",
            &[("title", "Renamed")],
            Some(&config),
            &GitCli,
        ),
    );
    assert_refused(
        "link",
        lazyspec::engine::ops::link::link_with_config(
            root,
            &store,
            "RFC-001",
            "related-to",
            "RFC-002",
            &fs,
            Some(&config),
        ),
    );
    assert_refused(
        "tag",
        lazyspec::cli::tag::tag_add_with_config(
            root,
            &store,
            "RFC-001",
            &["shared".to_string()],
            &fs,
            Some(&config),
        ),
    );
    assert_refused(
        "delete",
        lazyspec::engine::ops::delete::run_with_config(root, &store, "RFC-001", Some(&config)),
    );

    assert_eq!(std::fs::read(&clone_doc).unwrap(), before);
    assert!(!root.join("docs/rfcs").exists());
}

// --- STORY-281 AC5: `fetch` brings the clone current ---

/// A project whose only remote type is `git`, written as the binary reads it so
/// `fetch --json` is exercised end to end, stdout included.
fn write_project_config(root: &Path, remote: &Path) {
    let toml = format!(
        r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "git"
remote = "{}"

[[types]]
name = "note"
plural = "notes"
dir = "docs/notes"
prefix = "NOTE"

[[relationships]]
name = "related-to"
"#,
        remote.display()
    );
    std::fs::write(root.join(".lazyspec.toml"), toml).unwrap();
}

fn lazyspec(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("lazyspec runs")
}

fn fetch_json(root: &Path, args: &[&str]) -> serde_json::Value {
    let output = lazyspec(root, args);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "fetch failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !stdout.contains("no fetchable types"),
        "a git type is fetchable: {stdout}"
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}\nstdout: {stdout}"))
}

fn ids_via_binary(root: &Path) -> String {
    let output = lazyspec(root, &["list", "--json"]);
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn fetch_brings_the_clone_current_and_reports_the_git_type() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path());
    assert!(!ids_via_binary(root).contains("RFC-002"));

    write_rfc(remote.path(), "RFC-002-b.md", "B on main");
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "three"]);

    let outcomes = fetch_json(root, &["fetch", "--json"]);

    assert_eq!(
        outcomes,
        serde_json::json!([{"type": "rfc", "fetched": 2, "new": 1, "removed": 0}])
    );
    assert!(ids_via_binary(root).contains("RFC-002"));
}

#[test]
fn fetch_reports_a_document_removed_upstream_and_drops_it_from_the_clone() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path());
    assert!(ids_via_binary(root).contains("RFC-001"));

    git(remote.path(), &["rm", "-q", "docs/rfcs/RFC-001-a.md"]);
    git(remote.path(), &["commit", "-m", "drop"]);

    let outcomes = fetch_json(root, &["fetch", "--json"]);

    assert_eq!(outcomes[0]["removed"], 1, "{outcomes}");
    assert_eq!(outcomes[0]["fetched"], 0, "{outcomes}");
    assert!(!root
        .join(".lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md")
        .exists());
}

#[test]
fn fetch_type_filter_accepts_a_git_type_and_names_git_when_refusing_another() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path());

    let outcomes = fetch_json(root, &["fetch", "--type", "rfc", "--json"]);
    assert_eq!(outcomes[0]["type"], "rfc", "{outcomes}");

    let refused = lazyspec(root, &["fetch", "--type", "note"]);
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("git-ref, git, or clickup-tasks"),
        "the refusal lists git among the fetchable backends: {stderr}"
    );
}

#[test]
fn missing_branch_error_names_the_branch() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();

    let Err(err) = Store::load(project.path(), &git_config(remote.path(), Some("nope"))) else {
        panic!("cloning a branch the remote lacks fails");
    };

    assert!(format!("{err:#}").contains("nope"), "{err:#}");
}
