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

    lazyspec::engine::ops::delete::run_with_config(root, &store, "RFC-001", Some(&config)).unwrap();
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

    let path = lazyspec::cli::create::run(root, &config, &store, "rfc", "Mine", "tester", |_| {})
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

    lazyspec::cli::create::run(root, &config, &store, "rfc", "Mine", "tester", |_| {}).unwrap();
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

    lazyspec::cli::create::run(root, &config, &store, "rfc", "Mine", "tester", |_| {}).unwrap();
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
    assert!(!root
        .join(".lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md")
        .exists());
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
