//! STORY-281: a `git` type clones its remote on first read and reads from the
//! clone thereafter. The remote is a plain local repository in a `TempDir`, so
//! no test here reaches the network.

use lazyspec::engine::config::{
    Config, NumberingStrategy, ReservedConfig, ReservedFormat, StoreBackend, TypeDef,
};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::git_ref::{GitCli, GitRefOps};
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
/// `next`. `main` is checked out, so it is what a clone with no `branch` gets;
/// `updateInstead` lets a push land on it anyway while its tree is clean, so
/// "commit straight into the remote" stays the way to move the remote ahead.
fn shared_repo() -> TempDir {
    let repo = TempDir::new().unwrap();
    let path = repo.path();
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "test@test.com"]);
    git(path, &["config", "user.name", "Test"]);
    git(
        path,
        &["config", "receive.denyCurrentBranch", "updateInstead"],
    );
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

// --- STORY-282: every registry-routed write commits in the clone and pushes ---

fn git_stdout(repo: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn commit_count(repo: &Path, branch: &str) -> usize {
    git_stdout(repo, &["rev-list", "--count", branch])
        .trim()
        .parse()
        .unwrap()
}

/// The clone has no `user.*` of its own, and the tests must not lean on the
/// host's global config (DICTUM-004).
fn identify_clone(root: &Path) -> std::path::PathBuf {
    let clone = root.join(".lazyspec/cache/rfc");
    git(&clone, &["config", "user.email", "test@test.com"]);
    git(&clone, &["config", "user.name", "Test"]);
    clone
}

#[test]
fn create_writes_into_the_clone_and_pushes_to_the_declared_branch() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let before = commit_count(remote.path(), "next");

    let (path, outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "rfc",
        "C",
        "tester",
        None,
        Some("Shared body."),
        |_| {},
    )
    .unwrap();

    assert!(outcome.is_synced());
    let type_def = &config.documents.types[0];
    assert!(
        path.starts_with(doc_root(root, type_def)),
        "{}",
        path.display()
    );
    assert!(path.exists());
    assert!(!root.join("docs/rfcs").exists());
    assert_eq!(commit_count(remote.path(), "next"), before + 1);
    assert!(
        git_stdout(remote.path(), &["log", "-1", "--format=%s", "next"]).contains("create RFC-003")
    );
    let filename = path.file_name().unwrap().to_string_lossy();
    let pushed = git_stdout(
        remote.path(),
        &["show", &format!("next:docs/rfcs/{filename}")],
    );
    assert_eq!(pushed, std::fs::read_to_string(&path).unwrap());
}

#[test]
fn update_tag_provenance_and_delete_each_push_one_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let fs = RealFileSystem;
    let mut expected = commit_count(remote.path(), "next");

    lazyspec::engine::ops::update::run_with_config(
        root,
        &store,
        "RFC-001",
        &[("title", "Renamed")],
        Some(&config),
        &GitCli,
    )
    .unwrap();
    expected += 1;
    assert_eq!(commit_count(remote.path(), "next"), expected, "update");
    assert!(
        git_stdout(remote.path(), &["show", "next:docs/rfcs/RFC-001-a.md"]).contains("Renamed")
    );

    lazyspec::cli::tag::tag_add_with_config(
        root,
        &store,
        "RFC-001",
        &["shared".to_string()],
        &fs,
        Some(&config),
    )
    .unwrap();
    expected += 1;
    assert_eq!(commit_count(remote.path(), "next"), expected, "tag");
    assert!(git_stdout(remote.path(), &["show", "next:docs/rfcs/RFC-001-a.md"]).contains("shared"));

    lazyspec::engine::provenance::set_provenance(
        root,
        &config,
        "rfc",
        "RFC-001",
        &["https://example.com/source".to_string()],
    )
    .unwrap();
    expected += 1;
    assert_eq!(commit_count(remote.path(), "next"), expected, "provenance");

    lazyspec::engine::ops::delete::run_with_config(root, &store, "RFC-001", Some(&config)).unwrap();
    expected += 1;
    assert_eq!(commit_count(remote.path(), "next"), expected, "delete");
    assert!(!git_stdout(
        remote.path(),
        &["ls-tree", "--name-only", "next", "docs/rfcs/"]
    )
    .contains("RFC-001-a.md"));
}

/// The remote gains `RFC-002-b.md` on `main` after the clone, so the clone's
/// `main` is behind it and any push is rejected as a non-fast-forward.
fn move_remote_ahead(remote: &Path) {
    write_rfc(remote, "RFC-002-b.md", "B on main");
    git(remote, &["add", "-A"]);
    git(remote, &["commit", "-m", "three"]);
}

// --- STORY-282 AC2: the writers that bypass the registry commit too ---

fn remote_file(remote: &Path, name: &str) -> String {
    git_stdout(remote, &["show", &format!("next:docs/rfcs/{name}")])
}

#[test]
fn link_and_unlink_each_push_one_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let fs = RealFileSystem;
    let before = commit_count(remote.path(), "next");

    lazyspec::engine::ops::link::link_with_config(
        root,
        &store,
        "RFC-001",
        "related-to",
        "RFC-002",
        &fs,
        Some(&config),
    )
    .unwrap();

    assert_eq!(commit_count(remote.path(), "next"), before + 1, "link");
    assert!(remote_file(remote.path(), "RFC-001-a.md").contains("related-to: RFC-002"));

    lazyspec::engine::ops::link::unlink_with_config(
        root,
        &store,
        "RFC-001",
        "related-to",
        "RFC-002",
        &fs,
        Some(&config),
    )
    .unwrap();

    assert_eq!(commit_count(remote.path(), "next"), before + 2, "unlink");
    assert!(!remote_file(remote.path(), "RFC-001-a.md").contains("RFC-002"));
}

#[test]
fn ignore_and_unignore_each_push_one_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let fs = RealFileSystem;
    let before = commit_count(remote.path(), "next");

    lazyspec::cli::ignore::ignore(root, &store, &config, &GitCli, "RFC-001", &fs).unwrap();

    assert_eq!(commit_count(remote.path(), "next"), before + 1, "ignore");
    assert!(remote_file(remote.path(), "RFC-001-a.md").contains("validate-ignore: true"));

    lazyspec::cli::ignore::unignore(root, &store, &config, &GitCli, "RFC-001", &fs).unwrap();

    assert_eq!(commit_count(remote.path(), "next"), before + 2, "unignore");
    assert!(!remote_file(remote.path(), "RFC-001-a.md").contains("validate-ignore"));
}

#[test]
fn pin_pushes_one_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    // `pin` stamps HEAD of the governs root, which is the project itself.
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "test@test.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["commit", "--allow-empty", "-m", "code"]);
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let before = commit_count(remote.path(), "next");

    lazyspec::cli::pin::run(&store, &config, &GitCli, &RealFileSystem, "RFC-001", true).unwrap();

    assert_eq!(commit_count(remote.path(), "next"), before + 1);
    assert!(remote_file(remote.path(), "RFC-001-a.md").contains("reviewed:"));
}

#[test]
fn fix_pushes_one_commit_for_the_document_it_repairs() {
    let remote = shared_repo();
    git(remote.path(), &["checkout", "next"]);
    std::fs::write(
        remote.path().join("docs/rfcs/RFC-003-c.md"),
        "---\ntitle: \"C\"\ntype: rfc\nstatus: draft\ndate: 2026-04-01\ntags: []\n---\n\nNo author.\n",
    )
    .unwrap();
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "three"]);
    git(remote.path(), &["checkout", "main"]);
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let before = commit_count(remote.path(), "next");
    let paths = vec![".lazyspec/cache/rfc/docs/rfcs/RFC-003-c.md".to_string()];

    let code = lazyspec::cli::fix::run(
        root,
        &store,
        &config,
        &paths,
        false,
        true,
        &GitCli,
        &RealFileSystem,
    );

    assert_eq!(code, 0);
    assert_eq!(commit_count(remote.path(), "next"), before + 1);
    assert!(remote_file(remote.path(), "RFC-003-c.md").contains("author:"));
}

#[test]
fn renumber_pushes_one_commit_for_every_rename() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let mut config = git_config(remote.path(), Some("next"));
    config.documents.sqids = Some(lazyspec::engine::config::SqidsConfig {
        salt: "shared".to_string(),
        min_length: 3,
    });
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let before = commit_count(remote.path(), "next");

    let code = lazyspec::cli::fix::run_renumber(
        root,
        &store,
        &config,
        &lazyspec::cli::RenumberFormat::Sqids,
        Some("rfc"),
        false,
        true,
        &GitCli,
        &RealFileSystem,
    );

    assert_eq!(code, 0);
    assert_eq!(commit_count(remote.path(), "next"), before + 1);
    let tree = git_stdout(
        remote.path(),
        &["ls-tree", "--name-only", "next", "docs/rfcs/"],
    );
    assert!(!tree.contains("RFC-001-a.md"), "{tree}");
    assert!(!tree.contains("RFC-002-b.md"), "{tree}");
    assert_eq!(tree.lines().count(), 2, "{tree}");
}

#[test]
fn rejected_link_errors_and_leaves_the_clone_file_unchanged() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root);
    let doc = clone.join("docs/rfcs/RFC-001-a.md");
    let bytes = std::fs::read(&doc).unwrap();
    git(remote.path(), &["checkout", "next"]);
    write_rfc(remote.path(), "RFC-003-c.md", "C");
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "three"]);
    git(remote.path(), &["checkout", "main"]);

    let Err(err) = lazyspec::engine::ops::link::link_with_config(
        root,
        &store,
        "RFC-001",
        "related-to",
        "RFC-002",
        &RealFileSystem,
        Some(&config),
    ) else {
        panic!("a push the remote rejects is an error");
    };

    assert!(format!("{err:#}").contains("lazyspec fetch"), "{err:#}");
    assert_eq!(std::fs::read(&doc).unwrap(), bytes);
}

// STORY-282 AC4, AC6: a rejected push is an error naming remote, branch and
// `lazyspec fetch`, and the clone is byte-identical to before the command.
#[test]
fn rejected_push_errors_and_rolls_the_clone_back() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("main"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root);
    let head = git_stdout(&clone, &["rev-parse", "HEAD"]);
    let docs = clone.join("docs/rfcs");
    move_remote_ahead(remote.path());

    let Err(err) = lazyspec::cli::create::run(root, &config, &store, "rfc", "C", "tester", |_| {})
    else {
        panic!("a push the remote rejects is an error");
    };

    let msg = format!("{err:#}");
    assert!(msg.contains(&*remote.path().to_string_lossy()), "{msg}");
    assert!(msg.contains("(main)"), "{msg}");
    assert!(msg.contains("lazyspec fetch"), "{msg}");
    assert_eq!(git_stdout(&clone, &["rev-parse", "HEAD"]), head);
    assert_eq!(git_stdout(&clone, &["status", "--porcelain"]), "");
    let mut names: Vec<_> = std::fs::read_dir(&docs)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["RFC-001-a.md"]);
    assert_eq!(lazyspec::engine::template::next_number(&docs, "RFC"), 2);
}

#[test]
fn rejected_push_through_the_binary_exits_nonzero_with_empty_stdout() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path());
    assert!(ids_via_binary(root).contains("RFC-001"));
    identify_clone(root);
    move_remote_ahead(remote.path());

    let output = lazyspec(root, &["create", "rfc", "C", "--json"]);

    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lazyspec fetch"), "{stderr}");
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

// --- STORY-282 AC7, AC8: reserved numbering claims against the type's remote ---

/// `[numbering.reserved].remote` names nothing: a create that consulted it
/// would fail, so a create that succeeds proves the type's remote was used.
fn reserved_config(remote: &Path, branch: Option<&str>) -> Config {
    let mut config = git_config(remote, branch);
    config.documents.types[0].numbering = NumberingStrategy::Reserved;
    config.documents.reserved = Some(ReservedConfig {
        remote: "no-such-remote".to_string(),
        format: ReservedFormat::Incremental,
        max_retries: 5,
    });
    config
}

fn seed_reservation(remote: &Path, num: u32) {
    let sha = String::from_utf8_lossy(
        &Command::new("git")
            .args(["hash-object", "-w", "-t", "blob", "--stdin"])
            .stdin(std::process::Stdio::null())
            .current_dir(remote)
            .output()
            .expect("hash-object")
            .stdout,
    )
    .trim()
    .to_string();
    git(
        remote,
        &["update-ref", &format!("refs/reservations/RFC/{num}"), &sha],
    );
}

fn reservations(remote: &Path) -> Vec<u32> {
    let mut nums: Vec<u32> = git_stdout(
        remote,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/reservations/RFC/",
        ],
    )
    .lines()
    .filter_map(|l| l.rsplit('/').next()?.parse().ok())
    .collect();
    nums.sort_unstable();
    nums
}

fn create_rfc(root: &Path, config: &Config, store: &Store, title: &str) -> anyhow::Result<String> {
    let path = lazyspec::cli::create::run(root, config, store, "rfc", title, "tester", |_| {})?;
    Ok(path.file_name().unwrap().to_string_lossy().into_owned())
}

#[test]
fn reserved_create_reserves_against_the_types_remote_and_pushes() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = reserved_config(remote.path(), Some("main"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root);
    let before = commit_count(remote.path(), "main");

    let filename = create_rfc(root, &config, &store, "Second").unwrap();

    assert!(filename.starts_with("RFC-002-"), "{filename}");
    assert!(clone.join("docs/rfcs").join(&filename).exists());
    assert_eq!(reservations(remote.path()), vec![2]);
    assert_eq!(commit_count(remote.path(), "main"), before + 1);
}

#[test]
fn reserved_create_continues_past_the_remotes_highest_reservation() {
    let remote = shared_repo();
    seed_reservation(remote.path(), 7);
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = reserved_config(remote.path(), Some("main"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);

    let filename = create_rfc(root, &config, &store, "Eighth").unwrap();

    assert!(filename.starts_with("RFC-008-"), "{filename}");
    assert_eq!(reservations(remote.path()), vec![7, 8]);
}

#[test]
fn stale_reserved_project_errors_then_fetches_and_lands_a_fresh_id() {
    let remote = shared_repo();
    let config = reserved_config(remote.path(), Some("main"));
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    let store_a = Store::load(project_a.path(), &config).unwrap();
    let store_b = Store::load(project_b.path(), &config).unwrap();
    identify_clone(project_a.path());
    let clone_b = identify_clone(project_b.path());
    let head_b = git_stdout(&clone_b, &["rev-parse", "HEAD"]);

    let a = create_rfc(project_a.path(), &config, &store_a, "From A").unwrap();
    assert!(a.starts_with("RFC-002-"), "{a}");

    let Err(err) = create_rfc(project_b.path(), &config, &store_b, "From B") else {
        panic!("B's clone is behind the remote, so its push is rejected");
    };
    assert!(format!("{err:#}").contains("lazyspec fetch"), "{err:#}");
    assert_eq!(git_stdout(&clone_b, &["rev-parse", "HEAD"]), head_b);
    assert_eq!(git_stdout(&clone_b, &["status", "--porcelain"]), "");
    let held = reservations(remote.path());

    GitCli.update_clone(&clone_b, Some("main")).unwrap();
    let b = create_rfc(project_b.path(), &config, &store_b, "From B").unwrap();

    let num: u32 = b["RFC-".len().."RFC-".len() + 3].parse().unwrap();
    assert_ne!(num, 2, "{b}");
    assert!(!held.contains(&num), "{b} collides with {held:?}");
    assert!(reservations(remote.path()).contains(&num));
    assert!(git_stdout(
        remote.path(),
        &["ls-tree", "--name-only", "main", "docs/rfcs/"]
    )
    .contains(&b));
}

// STORY-282 AC7 through the binary: fetch, then the re-run lands on the next free id.
#[test]
fn stale_project_fetches_then_creates_a_non_colliding_id() {
    let remote = shared_repo();
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    write_project_config(project_a.path(), remote.path());
    write_project_config(project_b.path(), remote.path());
    assert!(ids_via_binary(project_a.path()).contains("RFC-001"));
    assert!(ids_via_binary(project_b.path()).contains("RFC-001"));
    identify_clone(project_a.path());
    identify_clone(project_b.path());

    let a = create_json(project_a.path(), "A2");
    assert_eq!(a["id"], "RFC-002", "{a}");

    let rejected = lazyspec(project_b.path(), &["create", "rfc", "B2", "--json"]);
    assert!(!rejected.status.success());
    assert_eq!(String::from_utf8_lossy(&rejected.stdout), "");

    fetch_json(project_b.path(), &["fetch", "--json"]);
    let b = create_json(project_b.path(), "B2");

    assert_eq!(b["id"], "RFC-003", "{b}");
    assert_eq!(b["synced"], true, "{b}");
    let tree = git_stdout(
        remote.path(),
        &["ls-tree", "--name-only", "main", "docs/rfcs/"],
    );
    let mut ids: Vec<&str> = tree.lines().filter_map(|l| l.get(10..17)).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["RFC-001", "RFC-002", "RFC-003"], "{tree}");
}

fn create_json(root: &Path, title: &str) -> serde_json::Value {
    let output = lazyspec(root, &["create", "rfc", title, "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "create failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}\nstdout: {stdout}"))
}

// --- ITERATION-440 (STORY-282 AC9, AC10): create --parent on a git type ---

/// A second `git` type, `spec`, sharing `rfc`'s remote and branch, with its own
/// clone directory (`.lazyspec/cache/spec`).
fn add_spec_type(mut config: Config, remote: &Path, branch: Option<&str>) -> Config {
    config.documents.types.push(TypeDef {
        dir: "docs/specs".to_string(),
        remote: Some(remote.to_string_lossy().into_owned()),
        branch: branch.map(str::to_string),
        ..TypeDef::test_fixture("spec", StoreBackend::Git)
    });
    config
}

fn identify(clone: &Path) {
    git(clone, &["config", "user.email", "test@test.com"]);
    git(clone, &["config", "user.name", "Test"]);
}

#[test]
fn create_with_parent_promotes_the_flat_parent_and_pushes_both_in_one_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    let before = commit_count(remote.path(), "next");

    let (path, outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "rfc",
        "Child",
        "tester",
        Some("RFC-001"),
        None,
        |_| {},
    )
    .unwrap();

    assert!(outcome.is_synced());
    let type_def = &config.documents.types[0];
    let root_dir = doc_root(root, type_def);
    assert!(root_dir.join("RFC-001-a/index.md").exists());
    assert!(
        path.starts_with(root_dir.join("RFC-001-a")),
        "{}",
        path.display()
    );
    assert_eq!(
        path.file_name().unwrap().to_string_lossy(),
        "RFC-001-child.md",
        "subdir numbering is local to the promoted parent's own folder"
    );
    assert_eq!(commit_count(remote.path(), "next"), before + 1);
    let tree = git_stdout(
        remote.path(),
        &["ls-tree", "-r", "--name-only", "next", "docs/rfcs"],
    );
    assert!(tree.contains("docs/rfcs/RFC-001-a/index.md"), "{tree}");
    assert!(
        tree.contains("docs/rfcs/RFC-001-a/RFC-001-child.md"),
        "{tree}"
    );

    let reloaded = Store::load(root, &config).unwrap();
    let parent = reloaded
        .list(&Filter::default())
        .into_iter()
        .find(|d| d.id == "RFC-001")
        .unwrap();
    let child = reloaded
        .list(&Filter::default())
        .into_iter()
        .find(|d| d.id == "RFC-001-child")
        .unwrap();
    assert_eq!(reloaded.parent_of(&child.path), Some(&parent.path));
}

#[test]
fn create_with_parent_across_two_git_types_sharing_a_remote_lands_in_the_parents_clone() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = add_spec_type(
        git_config(remote.path(), Some("next")),
        remote.path(),
        Some("next"),
    );
    let store = Store::load(root, &config).unwrap();
    identify_clone(root);
    identify(&root.join(".lazyspec/cache/spec"));
    let before = commit_count(remote.path(), "next");

    let (path, outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "spec",
        "Child Spec",
        "tester",
        Some("RFC-001"),
        None,
        |_| {},
    )
    .unwrap();

    assert!(outcome.is_synced());
    assert!(
        path.starts_with(root.join(".lazyspec/cache/rfc/docs/rfcs/RFC-001-a")),
        "{}",
        path.display()
    );
    assert!(
        !path.starts_with(root.join(".lazyspec/cache/spec")),
        "the child must land in the parent's clone, not its own type's"
    );
    assert_eq!(commit_count(remote.path(), "next"), before + 1);
    let tree = git_stdout(
        remote.path(),
        &["ls-tree", "-r", "--name-only", "next", "docs/rfcs"],
    );
    assert!(
        tree.lines()
            .any(|l| l.starts_with("docs/rfcs/RFC-001-a/") && l.contains("child-spec")),
        "{tree}"
    );
}

#[test]
fn create_with_parent_across_two_git_types_different_remotes_rejected_before_any_push() {
    let remote_a = shared_repo();
    let remote_b = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = add_spec_type(
        git_config(remote_a.path(), Some("next")),
        remote_b.path(),
        Some("next"),
    );
    let store = Store::load(root, &config).unwrap();
    let rfc_clone = identify_clone(root);
    let spec_clone = root.join(".lazyspec/cache/spec");
    identify(&spec_clone);
    let before_a = commit_count(remote_a.path(), "next");
    let before_b = commit_count(remote_b.path(), "next");

    let err = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "spec",
        "Child Spec",
        "tester",
        Some("RFC-001"),
        None,
        |_| {},
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains(&*remote_a.path().to_string_lossy()), "{msg}");
    assert!(msg.contains(&*remote_b.path().to_string_lossy()), "{msg}");
    assert_eq!(commit_count(remote_a.path(), "next"), before_a);
    assert_eq!(commit_count(remote_b.path(), "next"), before_b);
    assert_eq!(git_stdout(&rfc_clone, &["status", "--porcelain"]), "");
    assert_eq!(git_stdout(&spec_clone, &["status", "--porcelain"]), "");
}

fn create_with_parent_json(root: &Path, title: &str, parent: &str) -> serde_json::Value {
    let output = lazyspec(
        root,
        &["create", "rfc", title, "--parent", parent, "--json"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "create failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}\nstdout: {stdout}"))
}

#[test]
fn create_with_parent_through_the_binary_writes_into_the_clone_and_syncs() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path());
    assert!(ids_via_binary(root).contains("RFC-001"));
    identify_clone(root);

    let child = create_with_parent_json(root, "Child", "RFC-001");

    let path = child["path"].as_str().unwrap();
    assert!(
        path.contains(".lazyspec/cache/rfc/docs/rfcs/RFC-001-a"),
        "{path}"
    );
    assert_eq!(child["synced"], true, "{child}");
}
