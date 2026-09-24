//! STORY-281: a `git` type clones its remote on first read and reads from the
//! clone thereafter. The remote is a plain local repository in a `TempDir`, so
//! no test here reaches the network.

use crate::common::{git, git_stdout};
use lazyspec::engine::config::{
    Config, NumberingStrategy, ReservedConfig, ReservedFormat, StoreBackend, TypeDef,
};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::git_ref::{GitCli, GitRefOps};
use lazyspec::engine::git_store::clone_root;
use lazyspec::engine::store::{doc_root, Filter, Store};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

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

    let clone = clone_root(root, &remote.path().to_string_lossy(), None);
    assert!(clone.join("docs/rfcs/RFC-001-a.md").exists());
    assert_eq!(ids(&store), vec!["RFC-001"]);
    let type_def = &config.documents.types.last().unwrap();
    let doc = &store.list(&Filter::default())[0];
    assert!(
        root.join(&doc.path)
            .starts_with(doc_root(&config, root, type_def)),
        "{} is under {}",
        doc.path.display(),
        doc_root(&config, root, type_def).display()
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
fn first_read_gitignores_the_shared_clone_root() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();

    Store::load(project.path(), &git_config(remote.path(), None)).unwrap();

    let gitignore = std::fs::read_to_string(project.path().join(".lazyspec/.gitignore")).unwrap();
    assert!(gitignore.lines().any(|l| l == "git/"), "{gitignore:?}");
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

// --- BUG-032: every registry-routed write commits in the shared clone, locally ---

fn commit_count(repo: &Path, branch: &str) -> usize {
    git_stdout(repo, &["rev-list", "--count", branch])
        .trim()
        .parse()
        .unwrap()
}

/// The clone has no `user.*` of its own, and the tests must not lean on the
/// host's global config (DICTUM-004).
fn identify_clone(root: &Path, remote: &Path, branch: Option<&str>) -> PathBuf {
    let clone = clone_root(root, &remote.to_string_lossy(), branch);
    git(&clone, &["config", "user.email", "test@test.com"]);
    git(&clone, &["config", "user.name", "Test"]);
    clone
}

#[test]
fn create_writes_into_the_clone_and_commits_locally_without_pushing() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let remote_before = commit_count(remote.path(), "next");
    let clone_before = commit_count(&clone, "HEAD");

    let (path, outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "rfc",
        "C",
        "tester",
        None,
        Some("Shared body."),
        &GitCli,
        |_| {},
    )
    .unwrap();

    assert!(!outcome.is_synced(), "a git write is local-only now");
    let type_def = &config.documents.types[0];
    assert!(
        path.starts_with(doc_root(&config, root, type_def)),
        "{}",
        path.display()
    );
    assert!(path.exists());
    assert!(!root.join("docs/rfcs").exists());
    assert_eq!(
        commit_count(remote.path(), "next"),
        remote_before,
        "the remote is untouched until `push`"
    );
    assert_eq!(commit_count(&clone, "HEAD"), clone_before + 1);
    assert!(git_stdout(&clone, &["log", "-1", "--format=%s", "HEAD"]).contains("create RFC-003"));
}

#[test]
fn update_tag_provenance_and_delete_each_commit_locally_once() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let fs = RealFileSystem;
    let remote_before = commit_count(remote.path(), "next");
    let mut expected = commit_count(&clone, "HEAD");

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
    assert_eq!(commit_count(&clone, "HEAD"), expected, "update");
    assert!(
        std::fs::read_to_string(clone.join("docs/rfcs/RFC-001-a.md"))
            .unwrap()
            .contains("Renamed")
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
    assert_eq!(commit_count(&clone, "HEAD"), expected, "tag");
    assert!(
        std::fs::read_to_string(clone.join("docs/rfcs/RFC-001-a.md"))
            .unwrap()
            .contains("shared")
    );

    lazyspec::engine::provenance::set_provenance(
        root,
        &config,
        "rfc",
        "RFC-001",
        &["https://example.com/source".to_string()],
    )
    .unwrap();
    expected += 1;
    assert_eq!(commit_count(&clone, "HEAD"), expected, "provenance");

    lazyspec::engine::ops::delete::run_with_config(root, &store, "RFC-001", Some(&config), &GitCli)
        .unwrap();
    expected += 1;
    assert_eq!(commit_count(&clone, "HEAD"), expected, "delete");
    assert!(!clone.join("docs/rfcs/RFC-001-a.md").exists());
    assert_eq!(
        commit_count(remote.path(), "next"),
        remote_before,
        "none of this reached the remote"
    );
}

// --- BUG-032 AC2: the writers that bypass the registry commit locally too ---

fn clone_file(clone: &Path, name: &str) -> String {
    std::fs::read_to_string(clone.join("docs/rfcs").join(name)).unwrap()
}

#[test]
fn link_and_unlink_each_commit_locally_once() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let fs = RealFileSystem;
    let remote_before = commit_count(remote.path(), "next");
    let before = commit_count(&clone, "HEAD");

    let link_outcome = lazyspec::engine::ops::link::link_with_config(
        root,
        &store,
        "RFC-001",
        "related-to",
        "RFC-002",
        &fs,
        Some(&config),
    )
    .unwrap();

    assert_eq!(commit_count(&clone, "HEAD"), before + 1, "link");
    assert!(clone_file(&clone, "RFC-001-a.md").contains("related-to: RFC-002"));
    assert!(
        !link_outcome.push_outcome.is_synced(),
        "a git write is local-only now"
    );

    let unlink_outcome = lazyspec::engine::ops::link::unlink_with_config(
        root,
        &store,
        "RFC-001",
        "related-to",
        "RFC-002",
        &fs,
        Some(&config),
    )
    .unwrap();

    assert_eq!(commit_count(&clone, "HEAD"), before + 2, "unlink");
    assert!(!clone_file(&clone, "RFC-001-a.md").contains("RFC-002"));
    assert!(
        !unlink_outcome.push_outcome.is_synced(),
        "a git write is local-only now"
    );
    assert_eq!(
        commit_count(remote.path(), "next"),
        remote_before,
        "neither write reached the remote"
    );
}

#[test]
fn ignore_and_unignore_each_commit_locally_once() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let fs = RealFileSystem;
    let before = commit_count(&clone, "HEAD");

    let ignore_outcome =
        lazyspec::cli::ignore::ignore(root, &store, &config, &GitCli, "RFC-001", &fs).unwrap();

    assert_eq!(commit_count(&clone, "HEAD"), before + 1, "ignore");
    assert!(clone_file(&clone, "RFC-001-a.md").contains("validate-ignore: true"));
    assert!(!ignore_outcome.is_synced(), "a git write is local-only now");

    let unignore_outcome =
        lazyspec::cli::ignore::unignore(root, &store, &config, &GitCli, "RFC-001", &fs).unwrap();

    assert_eq!(commit_count(&clone, "HEAD"), before + 2, "unignore");
    assert!(!clone_file(&clone, "RFC-001-a.md").contains("validate-ignore"));
    assert!(
        !unignore_outcome.is_synced(),
        "a git write is local-only now"
    );
}

// F6: a rewrite that lands byte-identical content (the doc was already
// ignored) leaves the clone clean, so the outcome is `Synced` with no
// warning -- not another `LocalOnly` claiming there is something to push.
#[test]
fn ignore_on_an_already_ignored_git_doc_is_synced_and_commits_nothing() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let fs = RealFileSystem;

    lazyspec::cli::ignore::ignore(root, &store, &config, &GitCli, "RFC-001", &fs).unwrap();
    let before = commit_count(&clone, "HEAD");

    let outcome =
        lazyspec::cli::ignore::ignore(root, &store, &config, &GitCli, "RFC-001", &fs).unwrap();

    assert_eq!(
        commit_count(&clone, "HEAD"),
        before,
        "an already-ignored doc has nothing left to commit"
    );
    assert!(outcome.is_synced(), "{outcome:?}");
    assert!(outcome.warning().is_none(), "{outcome:?}");
}

#[test]
fn govern_add_and_remove_each_commit_locally_once() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let fs = RealFileSystem;
    let before = commit_count(&clone, "HEAD");

    let (governs, add_outcome) = lazyspec::cli::govern::run_add(
        &store,
        &config,
        &GitCli,
        &fs,
        "RFC-001",
        &["src/engine/**".to_string()],
    )
    .unwrap();

    assert_eq!(governs, vec!["src/engine/**".to_string()]);
    assert_eq!(commit_count(&clone, "HEAD"), before + 1, "govern add");
    assert!(clone_file(&clone, "RFC-001-a.md").contains("src/engine/**"));
    assert!(!add_outcome.is_synced(), "a git write is local-only now");

    let store = Store::load(root, &config).unwrap();
    let (governs, remove_outcome) = lazyspec::cli::govern::run_remove(
        &store,
        &config,
        &GitCli,
        &fs,
        "RFC-001",
        &["src/engine/**".to_string()],
    )
    .unwrap();

    assert!(governs.is_empty());
    assert_eq!(commit_count(&clone, "HEAD"), before + 2, "govern remove");
    assert!(!clone_file(&clone, "RFC-001-a.md").contains("governs"));
    assert!(!remove_outcome.is_synced(), "a git write is local-only now");
}

#[test]
fn pin_commits_locally_once() {
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
    let clone = identify_clone(root, remote.path(), Some("next"));
    let before = commit_count(&clone, "HEAD");

    let pin_outcome =
        lazyspec::cli::pin::run(&store, &config, &GitCli, &RealFileSystem, "RFC-001", true)
            .unwrap();

    assert_eq!(commit_count(&clone, "HEAD"), before + 1);
    assert!(clone_file(&clone, "RFC-001-a.md").contains("reviewed:"));
    assert!(!pin_outcome.is_synced(), "a git write is local-only now");
}

// This test runs the compiled binary rather than calling `fix::run` in
// process, so the `--json` output it prints (including `synced`) can be
// asserted rather than discarded.
#[test]
fn fix_commits_locally_once_for_the_document_it_repairs() {
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
    write_project_config(root, remote.path(), Some("next"));
    // RFC-003 has no `author` and fails to parse, so it never appears in
    // `list`; this just forces the first-read clone into existence.
    assert!(lazyspec(root, &["list", "--json"]).status.success());
    let clone = identify_clone(root, remote.path(), Some("next"));
    let before = commit_count(&clone, "HEAD");
    let relative = clone
        .strip_prefix(root)
        .unwrap()
        .join("docs/rfcs/RFC-003-c.md");

    let json = fetch_json(root, &["fix", &relative.to_string_lossy(), "--json"]);

    assert_eq!(json["synced"], serde_json::json!(false), "{json}");
    assert_eq!(commit_count(&clone, "HEAD"), before + 1);
    assert!(clone_file(&clone, "RFC-003-c.md").contains("author:"));
}

#[test]
fn renumber_commits_locally_once_for_every_rename() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let mut config = git_config(remote.path(), Some("next"));
    config.documents.sqids = Some(lazyspec::engine::config::SqidsConfig {
        salt: "shared".to_string(),
        min_length: 3,
    });
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let before = commit_count(&clone, "HEAD");

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
    assert_eq!(commit_count(&clone, "HEAD"), before + 1);
    let tree = git_stdout(&clone, &["ls-tree", "--name-only", "HEAD", "docs/rfcs/"]);
    assert!(!tree.contains("RFC-001-a.md"), "{tree}");
    assert!(!tree.contains("RFC-002-b.md"), "{tree}");
    assert_eq!(tree.lines().count(), 2, "{tree}");
}

/// `teammate` (checking out `branch`, writing `filename`, committing, and
/// returning to whatever branch was checked out) simulates a push that landed
/// on the remote independently of the clone under test -- a sibling type's
/// write, or another writer entirely.
fn teammate_commits_on(remote: &Path, branch: &str, filename: &str, title: &str) {
    let previous = git_stdout(remote, &["symbolic-ref", "--short", "HEAD"])
        .trim()
        .to_string();
    git(remote, &["checkout", branch]);
    write_rfc(remote, filename, title);
    git(remote, &["add", "-A"]);
    git(remote, &["commit", "-m", "teammate"]);
    git(remote, &["checkout", &previous]);
}

// BUG-032: the original bug -- two git types sharing one remote landed in
// separate clones, so the second type's push always non-fast-forwarded once
// the first had pushed. A write is local-only now, so there is nothing left
// to reject: both creates land in the one shared clone, and the remote is
// untouched until `push`.
#[test]
fn two_git_types_on_one_remote_can_both_create_without_colliding() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = add_spec_type(
        git_config(remote.path(), Some("next")),
        remote.path(),
        Some("next"),
    );
    let store = Store::load(root, &config).unwrap();
    let clone = clone_root(root, &remote.path().to_string_lossy(), Some("next"));
    let before = commit_count(&clone, "HEAD");

    let (rfc_path, rfc_outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "rfc",
        "A2",
        "tester",
        None,
        None,
        &GitCli,
        |_| {},
    )
    .unwrap();
    let (spec_path, spec_outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "spec",
        "B1",
        "tester",
        None,
        None,
        &GitCli,
        |_| {},
    )
    .unwrap();

    assert!(!rfc_outcome.is_synced());
    assert!(!spec_outcome.is_synced());
    assert!(rfc_path.exists());
    assert!(spec_path.exists());
    assert!(rfc_path.starts_with(&clone));
    assert!(spec_path.starts_with(&clone));
    assert_eq!(
        commit_count(&clone, "HEAD"),
        before + 2,
        "both creates landed as local commits in the one shared clone"
    );
}

// BUG-032: a write commits locally regardless of what the remote has done
// meanwhile -- there is no push to reject.
#[test]
fn create_succeeds_locally_even_while_the_remote_has_moved_ahead() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let before = commit_count(&clone, "HEAD");

    teammate_commits_on(remote.path(), "next", "RFC-003-teammate.md", "Teammate");

    let path = lazyspec::cli::create::run(
        root,
        &config,
        &store,
        "rfc",
        "Mine",
        "tester",
        &GitCli,
        |_| {},
    )
    .expect("a local commit never needs the remote to agree");

    assert!(path.exists());
    assert_eq!(commit_count(&clone, "HEAD"), before + 1);
}

// BUG-032 AC3/AC4: `rebase_onto_remote` fetches and rebases the local commit
// onto the teammate's, then `push` lands both on the remote branch.
#[test]
fn rebase_onto_remote_then_push_rebases_over_a_teammate_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));

    lazyspec::cli::create::run(
        root,
        &config,
        &store,
        "rfc",
        "Mine",
        "tester",
        &GitCli,
        |_| {},
    )
    .unwrap();
    teammate_commits_on(remote.path(), "next", "RFC-003-teammate.md", "Teammate");

    GitCli.rebase_onto_remote(&clone, Some("next")).unwrap();
    let pushed = GitCli.push(&clone, Some("next")).unwrap();

    assert_eq!(pushed, 1);
    let tree = git_stdout(
        remote.path(),
        &["ls-tree", "--name-only", "next", "docs/rfcs/"],
    );
    assert!(tree.contains("RFC-003-teammate.md"), "{tree}");
    assert!(tree.lines().any(|l| l.contains("mine")), "{tree}");
    assert_eq!(
        commit_count(remote.path(), "next"),
        commit_count(&clone, "HEAD"),
        "the clone and the remote agree once pushed"
    );
}

// BUG-032: nothing ahead of the remote-tracking branch -- `push` pushes
// nothing and reports zero.
#[test]
fn push_reports_zero_when_nothing_is_ahead() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let _ = &store;

    let pushed = GitCli.push(&clone, Some("next")).unwrap();

    assert_eq!(pushed, 0);
}

// BUG-032 AC4: a rebase conflict is left for the human -- the rebase is
// aborted, the local commit is intact, and the error names the clone and the
// conflicted file.
#[test]
fn rebase_onto_remote_conflict_aborts_and_keeps_the_local_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));

    lazyspec::engine::ops::update::run_with_config(
        root,
        &store,
        "RFC-001",
        &[("title", "Mine")],
        Some(&config),
        &GitCli,
    )
    .unwrap();
    let local_head = git_stdout(&clone, &["rev-parse", "HEAD"]);

    let previous = git_stdout(remote.path(), &["symbolic-ref", "--short", "HEAD"])
        .trim()
        .to_string();
    git(remote.path(), &["checkout", "next"]);
    let remote_doc = remote.path().join("docs/rfcs/RFC-001-a.md");
    let content = std::fs::read_to_string(&remote_doc)
        .unwrap()
        .replace("title: \"A\"", "title: \"Theirs\"");
    std::fs::write(&remote_doc, content).unwrap();
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "theirs"]);
    git(remote.path(), &["checkout", &previous]);

    let err = GitCli.rebase_onto_remote(&clone, Some("next")).unwrap_err();

    let conflict = err
        .downcast_ref::<lazyspec::engine::git_ref::RebaseConflict>()
        .unwrap_or_else(|| panic!("a content conflict is a RebaseConflict: {err:#}"));
    assert_eq!(conflict.clone, clone);
    assert!(
        conflict.files.iter().any(|f| f.contains("RFC-001-a.md")),
        "{:?}",
        conflict.files
    );
    assert_eq!(
        git_stdout(&clone, &["rev-parse", "HEAD"]),
        local_head,
        "the local commit is kept"
    );
    assert_eq!(git_stdout(&clone, &["status", "--porcelain"]), "");
}

// BUG-032 AC5: `update_clone` rebases onto the fetched head instead of
// `reset --hard`, so a local, unpushed commit survives a refresh.
#[test]
fn update_clone_rebases_and_keeps_the_unpushed_local_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));

    lazyspec::cli::create::run(
        root,
        &config,
        &store,
        "rfc",
        "Mine",
        "tester",
        &GitCli,
        |_| {},
    )
    .unwrap();
    let local_subject = git_stdout(&clone, &["log", "-1", "--format=%s"]);
    teammate_commits_on(remote.path(), "next", "RFC-003-teammate.md", "Teammate");

    GitCli.update_clone(&clone, Some("next")).unwrap();

    assert!(clone.join("docs/rfcs/RFC-003-teammate.md").exists());
    assert!(
        clone.join("docs/rfcs").read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("mine")),
        "the local file is kept"
    );
    assert_eq!(
        git_stdout(&clone, &["log", "-1", "--format=%s"]),
        local_subject,
        "the local commit is still HEAD, rebased on top"
    );
    assert_eq!(
        GitCli.unpushed(&clone, Some("next")).unwrap(),
        1,
        "the local commit is still unpushed"
    );
}

// --- STORY-281 AC5: `fetch` brings the clone current ---

/// A project whose only remote type is `git`, written as the binary reads it so
/// `fetch --json` is exercised end to end, stdout included. `branch` pins the
/// clone to a non-default branch, matching [`git_config`].
fn write_project_config(root: &Path, remote: &Path, branch: Option<&str>) {
    let branch_line = branch
        .map(|b| format!("branch = \"{b}\"\n"))
        .unwrap_or_default();
    let toml = format!(
        r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "git"
remote = "{}"
{branch_line}
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
    write_project_config(root, remote.path(), None);
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
    write_project_config(root, remote.path(), None);
    assert!(ids_via_binary(root).contains("RFC-001"));

    git(remote.path(), &["rm", "-q", "docs/rfcs/RFC-001-a.md"]);
    git(remote.path(), &["commit", "-m", "drop"]);

    let outcomes = fetch_json(root, &["fetch", "--json"]);

    assert_eq!(outcomes[0]["removed"], 1, "{outcomes}");
    assert_eq!(outcomes[0]["fetched"], 0, "{outcomes}");
    let clone = clone_root(root, &remote.path().to_string_lossy(), None);
    assert!(!clone.join("docs/rfcs/RFC-001-a.md").exists());
}

#[test]
fn fetch_type_filter_accepts_a_git_type_and_names_git_when_refusing_another() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);

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

// F1: two types sharing one clone must each see their own teammate-pushed
// doc as `new: 1` -- not the second type reporting `new: 0` because the
// first type's fetch (against the one shared clone) already ran and moved
// what the second type's own "before" snapshot would have compared against.
#[test]
fn fetch_reports_new_one_for_both_types_sharing_a_clone() {
    let remote = shared_repo();
    write_rfc(remote.path(), "SPEC-001-a.md", "Spec A"); // seeds docs/rfcs; moved below
    std::fs::create_dir_all(remote.path().join("docs/specs")).unwrap();
    std::fs::rename(
        remote.path().join("docs/rfcs/SPEC-001-a.md"),
        remote.path().join("docs/specs/SPEC-001-a.md"),
    )
    .unwrap();
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "seed specs"]);

    let project = TempDir::new().unwrap();
    let root = project.path();
    write_two_git_types_config(root, remote.path(), None);
    lazyspec(root, &["fetch", "--json"]);

    write_rfc(remote.path(), "RFC-002-b.md", "B");
    write_rfc(remote.path(), "SPEC-002-b.md", "Spec B");
    std::fs::rename(
        remote.path().join("docs/rfcs/SPEC-002-b.md"),
        remote.path().join("docs/specs/SPEC-002-b.md"),
    )
    .unwrap();
    git(remote.path(), &["add", "-A"]);
    git(
        remote.path(),
        &["commit", "-m", "teammate adds one of each"],
    );

    let outcomes = fetch_json(root, &["fetch", "--json"]);

    let by_type = |name: &str| {
        outcomes
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["type"] == name)
            .unwrap_or_else(|| panic!("no outcome for '{name}': {outcomes}"))
    };
    assert_eq!(by_type("rfc")["new"], 1, "{outcomes}");
    assert_eq!(by_type("spec")["new"], 1, "{outcomes}");
}

// F3: a file hand-edited directly in the clone (not through lazyspec) must
// block both `push` and `fetch` before either touches the rebase -- naming
// the clone and how to clear it -- rather than rebasing over it and either
// dragging the edit into a replayed commit or losing it to a conflict abort.
#[test]
fn push_and_fetch_refuse_a_hand_edited_file_in_the_clone() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);
    assert!(ids_via_binary(root).contains("RFC-001"));

    let clone = clone_root(root, &remote.path().to_string_lossy(), None);
    let doc = clone.join("docs/rfcs/RFC-001-a.md");
    let before = std::fs::read_to_string(&doc).unwrap();
    std::fs::write(&doc, format!("{before}\nhand-edited, never committed\n")).unwrap();

    let push_output = push_output(root);
    assert!(!push_output.status.success(), "push must refuse");
    let push_value = push_json(&push_output);
    let clone_entry = &push_value["clones"][0];
    assert_eq!(
        clone_entry["error"]["kind"], "uncommitted_changes",
        "{push_value}"
    );
    let push_message = clone_entry["error"]["message"].as_str().unwrap();
    assert!(
        push_message.contains(&clone.display().to_string()),
        "{push_message}"
    );
    assert!(push_message.contains("stash"), "{push_message}");
    assert_eq!(
        std::fs::read_to_string(&doc).unwrap(),
        format!("{before}\nhand-edited, never committed\n"),
        "the hand edit must survive untouched"
    );

    let fetch_output = lazyspec(root, &["fetch", "--json"]);
    assert!(!fetch_output.status.success(), "fetch must refuse too");
    let fetch_stdout = String::from_utf8_lossy(&fetch_output.stdout);
    let fetch_value: serde_json::Value = serde_json::from_str(&fetch_stdout)
        .unwrap_or_else(|e| panic!("{e}\nstdout: {fetch_stdout}"));
    let fetch_message = fetch_value[0]["error"].as_str().unwrap();
    assert!(
        fetch_message.contains(&clone.display().to_string()),
        "{fetch_message}"
    );
    assert!(fetch_message.contains("stash"), "{fetch_message}");
}

// --- BUG-032 AC8: `fetch` migrates old per-type clones ---

/// A pre-BUG-032 per-type clone at `.lazyspec/cache/<type>/` -- what `fetch`
/// looks for and, when clean, deletes in favour of the shared clone at
/// `.lazyspec/git/<slug>`.
fn write_legacy_clone(root: &Path, remote: &Path, type_name: &str) {
    let legacy = root.join(".lazyspec/cache").join(type_name);
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    Command::new("git")
        .args([
            "clone",
            &remote.to_string_lossy(),
            &legacy.to_string_lossy(),
        ])
        .output()
        .expect("git clone");
}

#[test]
fn fetch_removes_a_clean_legacy_clone_and_reports_it() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);
    let legacy = root.join(".lazyspec/cache/rfc");
    write_legacy_clone(root, remote.path(), "rfc");
    assert!(legacy.join(".git").exists());
    // The subprocess reports its own (canonicalized) view of the path; `root`
    // here may still be a symlinked tmp dir (e.g. macOS's `/tmp` ->
    // `/private/tmp`).
    let legacy = legacy.canonicalize().unwrap();

    let output = lazyspec(root, &["fetch", "--json"]);
    assert!(output.status.success(), "{:?}", output);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains(&format!(
            "note: removed legacy clone for type `rfc`: {}",
            legacy.display()
        )),
        "{stderr}"
    );
    assert!(!legacy.exists());
}

#[test]
fn fetch_keeps_a_legacy_clone_with_an_unpushed_commit_and_warns() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);
    let legacy = root.join(".lazyspec/cache/rfc");
    write_legacy_clone(root, remote.path(), "rfc");
    git(&legacy, &["config", "user.email", "test@test.com"]);
    git(&legacy, &["config", "user.name", "Test"]);
    write_rfc(&legacy, "RFC-099-unpushed.md", "Unpushed");
    git(&legacy, &["add", "-A"]);
    git(&legacy, &["commit", "-m", "not pushed yet"]);
    let legacy = legacy.canonicalize().unwrap();

    let output = lazyspec(root, &["fetch", "--json"]);
    assert!(output.status.success(), "{:?}", output);

    assert!(legacy.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning:"), "{stderr}");
    assert!(stderr.contains(&legacy.display().to_string()), "{stderr}");
    assert!(stderr.contains("unpushed commit"), "{stderr}");
    assert!(
        !stderr.contains("note: removed legacy clone"),
        "nothing was removed: {stderr}"
    );
}

// A non-`git` type's `.lazyspec/cache/<name>` is its own legitimate cache
// (`github-issues` et al. materialize there), never a candidate for
// legacy-clone migration -- even when it happens to be a git repo itself, and
// even though it shares the same parent directory a `git` type's old clone
// would. `write_project_config`'s `note` type declares no `store`, so it is
// `filesystem`, not `git`.
#[test]
fn fetch_leaves_a_non_git_types_cache_dir_untouched() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);
    let notes_cache = root.join(".lazyspec/cache/note");
    write_legacy_clone(root, remote.path(), "note");
    assert!(notes_cache.join(".git").exists());

    let output = lazyspec(root, &["fetch", "--json"]);
    assert!(output.status.success(), "{:?}", output);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("note: removed legacy clone"),
        "a non-git type's cache dir is never a migration candidate: {stderr}"
    );
    assert!(
        notes_cache.exists(),
        "a non-git type's cache dir must survive fetch"
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
    let path =
        lazyspec::cli::create::run(root, config, store, "rfc", title, "tester", &GitCli, |_| {})?;
    Ok(path.file_name().unwrap().to_string_lossy().into_owned())
}

#[test]
fn reserved_create_reserves_against_the_types_remote_and_commits_locally() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = reserved_config(remote.path(), Some("main"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("main"));
    let remote_before = commit_count(remote.path(), "main");
    let clone_before = commit_count(&clone, "HEAD");

    let filename = create_rfc(root, &config, &store, "Second").unwrap();

    assert!(filename.starts_with("RFC-002-"), "{filename}");
    assert!(clone.join("docs/rfcs").join(&filename).exists());
    assert_eq!(reservations(remote.path()), vec![2]);
    assert_eq!(commit_count(&clone, "HEAD"), clone_before + 1);
    assert_eq!(
        commit_count(remote.path(), "main"),
        remote_before,
        "the doc commit is local only; only the reservation ref reached the remote"
    );
}

#[test]
fn reserved_create_continues_past_the_remotes_highest_reservation() {
    let remote = shared_repo();
    seed_reservation(remote.path(), 7);
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = reserved_config(remote.path(), Some("main"));
    let store = Store::load(root, &config).unwrap();
    identify_clone(root, remote.path(), Some("main"));

    let filename = create_rfc(root, &config, &store, "Eighth").unwrap();

    assert!(filename.starts_with("RFC-008-"), "{filename}");
    assert_eq!(reservations(remote.path()), vec![7, 8]);
}

// BUG-032: a clone that never saw a sibling project's write is no longer a
// reason to reject -- the doc commits locally regardless, and reserved
// numbering (queried straight against the live remote, not the clone) still
// lands the second project on a fresh, non-colliding number.
#[test]
fn stale_project_still_creates_locally_and_reserves_a_fresh_id() {
    let remote = shared_repo();
    let config = reserved_config(remote.path(), Some("main"));
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    let store_a = Store::load(project_a.path(), &config).unwrap();
    let store_b = Store::load(project_b.path(), &config).unwrap();
    identify_clone(project_a.path(), remote.path(), Some("main"));
    let clone_b = identify_clone(project_b.path(), remote.path(), Some("main"));
    let clone_b_before = commit_count(&clone_b, "HEAD");
    let remote_before = commit_count(remote.path(), "main");

    let a = create_rfc(project_a.path(), &config, &store_a, "From A").unwrap();
    assert!(a.starts_with("RFC-002-"), "{a}");

    let b = create_rfc(project_b.path(), &config, &store_b, "From B").unwrap();

    let num: u32 = b["RFC-".len().."RFC-".len() + 3].parse().unwrap();
    assert_ne!(num, 2, "{b}");
    assert!(reservations(remote.path()).contains(&num));
    assert!(clone_b.join("docs/rfcs").join(&b).exists());
    assert_eq!(commit_count(&clone_b, "HEAD"), clone_b_before + 1);
    assert_eq!(
        commit_count(remote.path(), "main"),
        remote_before,
        "neither project's doc commit reached the remote"
    );
}

// --- ITERATION-440 (STORY-282 AC9, AC10): create --parent on a git type ---

/// A second `git` type, `spec`. When `remote`/`branch` agree with `rfc`'s own,
/// the two share one clone (BUG-032 AC1); a different remote is a different
/// clone.
fn add_spec_type(mut config: Config, remote: &Path, branch: Option<&str>) -> Config {
    config.documents.types.push(TypeDef {
        dir: "docs/specs".to_string(),
        remote: Some(remote.to_string_lossy().into_owned()),
        branch: branch.map(str::to_string),
        ..TypeDef::test_fixture("spec", StoreBackend::Git)
    });
    config
}

#[test]
fn create_with_parent_promotes_the_flat_parent_and_commits_both_in_one_local_commit() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), Some("next"));
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let remote_before = commit_count(remote.path(), "next");
    let before = commit_count(&clone, "HEAD");

    let (path, outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "rfc",
        "Child",
        "tester",
        Some("RFC-001"),
        None,
        &GitCli,
        |_| {},
    )
    .unwrap();

    assert!(!outcome.is_synced());
    let type_def = &config.documents.types[0];
    let root_dir = doc_root(&config, root, type_def);
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
    assert_eq!(commit_count(&clone, "HEAD"), before + 1);
    assert_eq!(
        commit_count(remote.path(), "next"),
        remote_before,
        "the remote is untouched until `push`"
    );
    let tree = git_stdout(
        &clone,
        &["ls-tree", "-r", "--name-only", "HEAD", "docs/rfcs"],
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

// BUG-032 AC1: `rfc` and `spec` declare the same remote and branch, so they
// resolve to the one shared clone -- there is no separate "the child's own
// clone" for the child to land in instead.
#[test]
fn create_with_parent_across_two_git_types_sharing_a_remote_lands_in_the_shared_clone() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = add_spec_type(
        git_config(remote.path(), Some("next")),
        remote.path(),
        Some("next"),
    );
    let store = Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), Some("next"));
    let before = commit_count(&clone, "HEAD");

    let (path, outcome) = lazyspec::cli::create::run_with_body(
        root,
        &config,
        &store,
        "spec",
        "Child Spec",
        "tester",
        Some("RFC-001"),
        None,
        &GitCli,
        |_| {},
    )
    .unwrap();

    assert!(!outcome.is_synced());
    assert!(
        path.starts_with(clone.join("docs/rfcs/RFC-001-a")),
        "{}",
        path.display()
    );
    assert_eq!(commit_count(&clone, "HEAD"), before + 1);
    let tree = git_stdout(
        &clone,
        &["ls-tree", "-r", "--name-only", "HEAD", "docs/rfcs"],
    );
    assert!(
        tree.lines()
            .any(|l| l.starts_with("docs/rfcs/RFC-001-a/") && l.contains("child-spec")),
        "{tree}"
    );
}

#[test]
fn create_with_parent_across_two_git_types_different_remotes_rejected_before_any_mutation() {
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
    let rfc_clone = identify_clone(root, remote_a.path(), Some("next"));
    let spec_clone = identify_clone(root, remote_b.path(), Some("next"));
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
        &GitCli,
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
fn create_with_parent_through_the_binary_writes_into_the_clone_and_commits_locally() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);
    assert!(ids_via_binary(root).contains("RFC-001"));
    let clone = identify_clone(root, remote.path(), None);

    let child = create_with_parent_json(root, "Child", "RFC-001");

    let path = child["path"].as_str().unwrap();
    assert!(
        Path::new(path).starts_with(
            clone
                .strip_prefix(root)
                .unwrap()
                .join("docs/rfcs/RFC-001-a")
        ),
        "{path}"
    );
    assert_eq!(child["synced"], false, "{child}");
}

// --- BUG-032 batch 2: `lazyspec push` through the binary ---

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

fn push_output(root: &Path) -> std::process::Output {
    lazyspec(root, &["push", "--json"])
}

fn push_json(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "{e}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn status_json(root: &Path) -> serde_json::Value {
    let output = lazyspec(root, &["status", "--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "status failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}\nstdout: {stdout}"))
}

/// Two `git` types, `rfc` and `spec`, declaring the same remote and branch --
/// they share one clone (BUG-032 AC1), so one `push` publishes both.
fn write_two_git_types_config(root: &Path, remote: &Path, branch: Option<&str>) {
    let branch_line = branch
        .map(|b| format!("branch = \"{b}\"\n"))
        .unwrap_or_default();
    let toml = format!(
        r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "git"
remote = "{remote}"
{branch_line}
[[types]]
name = "spec"
plural = "specs"
dir = "docs/specs"
prefix = "SPEC"
store = "git"
remote = "{remote}"
{branch_line}
[[relationships]]
name = "related-to"
"#,
        remote = remote.display(),
    );
    std::fs::write(root.join(".lazyspec.toml"), toml).unwrap();
}

// (a) Two git types on one remote: `push` publishes both types' local
// commits in one shared-clone push.
#[test]
fn push_lands_both_git_types_sharing_one_clone_on_the_remote() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_two_git_types_config(root, remote.path(), Some("next"));
    assert!(ids_via_binary(root).contains("RFC-001"));

    assert!(lazyspec(root, &["create", "rfc", "A2", "--json"])
        .status
        .success());
    assert!(lazyspec(root, &["create", "spec", "B1", "--json"])
        .status
        .success());

    let output = push_output(root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = push_json(&output);
    let clones = value["clones"].as_array().unwrap();
    assert_eq!(clones.len(), 1, "{value}");
    assert_eq!(clones[0]["pushed"], 2, "{value}");
    assert!(clones[0]["error"].is_null(), "{value}");
    let mut types = clones[0]["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect::<Vec<_>>();
    types.sort();
    assert_eq!(types, vec!["rfc", "spec"], "{value}");

    let tree = git_stdout(remote.path(), &["ls-tree", "-r", "--name-only", "next"]);
    assert!(tree.contains("docs/rfcs/RFC-002"), "{tree}");
    assert!(tree.contains("docs/specs/SPEC-001"), "{tree}");
}

// (e, combined with b) `branch` unset resolves to the remote's default
// branch. Two independent clones of that remote each number their next
// incremental id RFC-002 without ever seeing each other; the first push
// lands cleanly, and the second is blocked with a `duplicate_ids` error
// naming both colliding paths, the local commit kept, the remote untouched.
#[test]
fn push_reports_duplicate_ids_when_two_clones_land_the_same_incremental_id() {
    let remote = shared_repo();
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    write_project_config(project_a.path(), remote.path(), None);
    write_project_config(project_b.path(), remote.path(), None);
    assert!(ids_via_binary(project_a.path()).contains("RFC-001"));
    assert!(ids_via_binary(project_b.path()).contains("RFC-001"));

    let a = create_json(project_a.path(), "A2");
    assert_eq!(a["id"], "RFC-002", "{a}");
    let b = create_json(project_b.path(), "B2");
    assert_eq!(
        b["id"], "RFC-002",
        "both clones independently number their next doc: {b}"
    );

    let a_push = push_output(project_a.path());
    assert!(
        a_push.status.success(),
        "{}",
        String::from_utf8_lossy(&a_push.stderr)
    );

    let remote_before = commit_count(remote.path(), "main");
    let b_push = push_output(project_b.path());
    assert!(!b_push.status.success(), "B's push must exit non-zero");
    let value = push_json(&b_push);
    let clone = &value["clones"][0];
    assert_eq!(clone["error"]["kind"], "duplicate_ids", "{value}");
    let collisions = clone["error"]["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 1, "{value}");
    assert_eq!(collisions[0]["id"], "RFC-002", "{value}");
    assert_eq!(
        collisions[0]["paths"].as_array().unwrap().len(),
        2,
        "{value}"
    );

    assert_eq!(
        commit_count(remote.path(), "main"),
        remote_before,
        "B's push never reached the remote"
    );
    let clone_b = identify_clone(project_b.path(), remote.path(), None);
    assert!(
        clone_b.join("docs/rfcs").read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("b2")),
        "B's local commit is kept"
    );
}

// (c) Two projects edit the same doc: `push` rebases cleanly onto the first
// project's push, but the second edit conflicts on the same line. The
// rebase aborts, the local commit is kept, and the clone path names itself
// in the error.
#[test]
fn push_reports_a_rebase_conflict_naming_the_clone_when_two_projects_edit_the_same_doc() {
    let remote = shared_repo();
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    write_project_config(project_a.path(), remote.path(), Some("next"));
    write_project_config(project_b.path(), remote.path(), Some("next"));
    assert!(ids_via_binary(project_a.path()).contains("RFC-001"));
    assert!(ids_via_binary(project_b.path()).contains("RFC-001"));

    let update_a = lazyspec(
        project_a.path(),
        &["update", "RFC-001", "--title", "Mine", "--json"],
    );
    assert!(
        update_a.status.success(),
        "{}",
        String::from_utf8_lossy(&update_a.stderr)
    );
    let push_a = push_output(project_a.path());
    assert!(
        push_a.status.success(),
        "{}",
        String::from_utf8_lossy(&push_a.stderr)
    );

    let update_b = lazyspec(
        project_b.path(),
        &["update", "RFC-001", "--title", "Theirs", "--json"],
    );
    assert!(
        update_b.status.success(),
        "{}",
        String::from_utf8_lossy(&update_b.stderr)
    );

    let clone_b = identify_clone(project_b.path(), remote.path(), Some("next"));
    let local_head = git_stdout(&clone_b, &["rev-parse", "HEAD"]);
    let push_b = push_output(project_b.path());
    assert!(!push_b.status.success(), "B's push must exit non-zero");
    let value = push_json(&push_b);
    let clone = &value["clones"][0];
    assert_eq!(clone["error"]["kind"], "rebase_conflict", "{value}");
    // Canonicalized: the subprocess and this test may resolve `TMPDIR`
    // through a different symlink prefix (`/tmp` vs `/private/tmp` on
    // macOS) for the same clone.
    assert_eq!(
        Path::new(clone["path"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        clone_b.canonicalize().unwrap(),
        "{value}"
    );
    assert!(
        clone["error"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str().unwrap().contains("RFC-001-a.md")),
        "{value}"
    );
    assert_eq!(
        git_stdout(&clone_b, &["rev-parse", "HEAD"]),
        local_head,
        "B's local commit is kept"
    );
}

// (d) `status --json` reports one unpushed commit after a create, and zero
// once `push` has published it.
#[test]
fn status_json_reports_unpushed_then_zero_after_push() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), Some("next"));
    assert!(ids_via_binary(root).contains("RFC-001"));

    assert!(lazyspec(root, &["create", "rfc", "Mine", "--json"])
        .status
        .success());

    let status = status_json(root);
    let git_stores = status["git_stores"].as_array().unwrap();
    assert_eq!(git_stores.len(), 1, "{status}");
    assert_eq!(git_stores[0]["unpushed"], 1, "{status}");

    let push_out = push_output(root);
    assert!(
        push_out.status.success(),
        "{}",
        String::from_utf8_lossy(&push_out.stderr)
    );

    let status_after = status_json(root);
    assert_eq!(
        status_after["git_stores"][0]["unpushed"], 0,
        "{status_after}"
    );
}

// STORY-282 AC7 equivalent, re-added for the local-commit model: a project
// that fetches before creating sees the sibling's already-pushed doc and
// numbers its own past it, landing a non-colliding id. Unlike the rejected
// push this used to test, the create here never risked a collision at all --
// it commits locally, after the fetch, into a clone that already holds
// RFC-002.
#[test]
fn project_b_fetches_after_a_pushes_and_creates_a_non_colliding_id() {
    let remote = shared_repo();
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    write_project_config(project_a.path(), remote.path(), None);
    write_project_config(project_b.path(), remote.path(), None);
    assert!(ids_via_binary(project_a.path()).contains("RFC-001"));
    assert!(ids_via_binary(project_b.path()).contains("RFC-001"));

    let a = create_json(project_a.path(), "A2");
    assert_eq!(a["id"], "RFC-002", "{a}");
    let push_a = push_output(project_a.path());
    assert!(
        push_a.status.success(),
        "{}",
        String::from_utf8_lossy(&push_a.stderr)
    );

    fetch_json(project_b.path(), &["fetch", "--json"]);
    let b = create_json(project_b.path(), "B2");

    assert_eq!(b["id"], "RFC-003", "{b}");
    let tree = git_stdout(
        remote.path(),
        &["ls-tree", "--name-only", "main", "docs/rfcs/"],
    );
    assert!(tree.contains("RFC-002"), "{tree}");
    assert!(
        !tree.contains("RFC-003"),
        "B's create is local-only until push: {tree}"
    );
}

// --- Batch 2 fix pass: `added_files` must not read a delete+similar-create
// pair as a rename, and a rebase already in progress must never be aborted ---

// A doc deleted and a similarly-worded new doc created in the same unpushed
// commit are, by git's own default rename heuristic, one `R` row, not an `A`
// and a `D` -- so without `--no-renames`, `added_files` (the duplicate-id
// guard's input) would report nothing added at all.
#[test]
fn added_files_reports_the_new_path_when_a_delete_and_a_similar_create_look_like_a_rename() {
    let remote = shared_repo();
    let project = TempDir::new().unwrap();
    let root = project.path();
    let config = git_config(remote.path(), None);
    Store::load(root, &config).unwrap();
    let clone = identify_clone(root, remote.path(), None);

    std::fs::remove_file(clone.join("docs/rfcs/RFC-001-a.md")).unwrap();
    write_rfc(&clone, "RFC-003-a.md", "A");
    git(&clone, &["add", "-A"]);
    git(
        &clone,
        &[
            "commit",
            "-m",
            "delete RFC-001, add a similarly-worded RFC-003",
        ],
    );

    let added = GitCli.added_files(&clone, None).unwrap();
    assert_eq!(
        added,
        vec!["docs/rfcs/RFC-003-a.md".to_string()],
        "the new path is reported as added, not hidden inside a rename pair"
    );
}

// The same rename-shaped pair, but the new doc's id collides with a sibling
// already in the clone -- end to end through `lazyspec push`, proving the
// duplicate-id guard actually sees the added path rather than missing it
// behind a rename.
#[test]
fn push_catches_a_duplicate_id_hidden_behind_a_delete_and_similar_create_rename() {
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-b", "main"]);
    git(remote.path(), &["config", "user.email", "test@test.com"]);
    git(remote.path(), &["config", "user.name", "Test"]);
    write_rfc(remote.path(), "RFC-002-a.md", "A");
    write_rfc(remote.path(), "RFC-005-b.md", "Shared wording");
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "one"]);

    let project = TempDir::new().unwrap();
    let root = project.path();
    write_project_config(root, remote.path(), None);
    assert!(ids_via_binary(root).contains("RFC-005"));
    let clone = identify_clone(root, remote.path(), None);

    std::fs::remove_file(clone.join("docs/rfcs/RFC-005-b.md")).unwrap();
    write_rfc(&clone, "RFC-002-c.md", "Shared wording");
    git(&clone, &["add", "-A"]);
    git(
        &clone,
        &[
            "commit",
            "-m",
            "delete RFC-005, add a similarly-worded RFC-002",
        ],
    );

    let output = push_output(root);
    assert!(
        !output.status.success(),
        "the collision must block the push"
    );
    let value = push_json(&output);
    let clone_result = &value["clones"][0];
    assert_eq!(clone_result["error"]["kind"], "duplicate_ids", "{value}");
    let collisions = clone_result["error"]["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 1, "{value}");
    assert_eq!(collisions[0]["id"], "RFC-002", "{value}");
}

// A conflicted rebase the user is mid-resolving must never be silently
// aborted by a later `push`/`fetch`: it errors, names the clone, and leaves
// both the in-progress rebase and the resolved-but-not-yet-committed content
// exactly as the user left them.
#[test]
fn rebase_onto_remote_does_not_abort_a_rebase_already_in_progress() {
    let remote = TempDir::new().unwrap();
    git(remote.path(), &["init", "-b", "main"]);
    git(remote.path(), &["config", "user.email", "test@test.com"]);
    git(remote.path(), &["config", "user.name", "Test"]);
    write_rfc(remote.path(), "RFC-001-a.md", "A");
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "one"]);

    let clone_parent = TempDir::new().unwrap();
    let clone = clone_parent.path().join("clone");
    git(
        clone_parent.path(),
        &["clone", &remote.path().to_string_lossy(), "clone"],
    );
    git(&clone, &["config", "user.email", "test@test.com"]);
    git(&clone, &["config", "user.name", "Test"]);

    // A local, unpushed edit...
    write_rfc(&clone, "RFC-001-a.md", "Local");
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-m", "local edit"]);

    // ...conflicting with an edit that landed on the remote in the meantime.
    write_rfc(remote.path(), "RFC-001-a.md", "Remote");
    git(remote.path(), &["add", "-A"]);
    git(remote.path(), &["commit", "-m", "remote edit"]);

    git(&clone, &["fetch", "origin", "main"]);
    // Started directly, not through `rebase_onto`, standing in for a rebase
    // the user began themselves; asserting success here would fail, since a
    // conflict is the point.
    let _ = Command::new("git")
        .args(["rebase", "origin/main"])
        .current_dir(&clone)
        .output()
        .expect("git runs");
    assert!(
        clone.join(".git/rebase-merge").exists(),
        "the rebase started"
    );

    // The user resolves the conflict and stages it, but has not yet run
    // `rebase --continue`.
    write_rfc(&clone, "RFC-001-a.md", "Resolved");
    git(&clone, &["add", "-A"]);
    let staged_before = git_stdout(&clone, &["diff", "--cached"]);

    let result = GitCli.rebase_onto_remote(&clone, Some("main"));

    assert!(
        result.is_err(),
        "an in-progress rebase must not be silently continued or aborted"
    );
    let message = format!("{:#}", result.unwrap_err());
    assert!(message.contains("rebase"), "{message}");
    assert!(
        message.contains(&clone.canonicalize().unwrap().to_string_lossy().into_owned())
            || message.contains(&clone.to_string_lossy().into_owned()),
        "{message}"
    );
    assert!(
        clone.join(".git/rebase-merge").exists(),
        "the rebase is still in progress -- nothing aborted it"
    );
    assert_eq!(
        git_stdout(&clone, &["diff", "--cached"]),
        staged_before,
        "the resolved, staged content is untouched"
    );
    assert!(
        std::fs::read_to_string(clone.join("docs/rfcs/RFC-001-a.md"))
            .unwrap()
            .contains("Resolved"),
        "the working tree still holds the user's resolution"
    );
}
