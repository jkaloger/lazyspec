mod common;

use common::TestFixture;
use lazyspec::engine::git_status::{parse_porcelain_line, GitFileStatus, GitStatusCache};
use lazyspec::tui::app::App;
use std::path::PathBuf;
use std::process::Command;

fn git(fixture: &TestFixture, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(fixture.root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn make_app(fixture: &TestFixture) -> App {
    let store = fixture.store();
    App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    )
}

/// Commit the initial docs directory structure so that individual
/// new files appear as separate entries in `git status --porcelain`
/// (without `-uall`, untracked dirs collapse into a single entry).
fn commit_docs_skeleton(fixture: &TestFixture) {
    for dir in &["docs/rfcs", "docs/adrs", "docs/stories", "docs/iterations"] {
        std::fs::write(fixture.root().join(dir).join(".gitkeep"), "").unwrap();
    }
    git(fixture, &["add", "docs"]);
    git(fixture, &["commit", "-m", "scaffold docs dirs"]);
}

#[test]
fn test_new_file_shows_green() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    commit_docs_skeleton(&fixture);
    fixture.write_rfc("RFC-100-new.md", "New RFC", "draft");

    let app = make_app(&fixture);
    let path = fixture.root().join("docs/rfcs/RFC-100-new.md");
    assert_eq!(
        app.git_status_cache.get(&path),
        Some(&GitFileStatus::New),
    );
}

#[test]
fn test_modified_file_shows_yellow() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    fixture.write_rfc("RFC-200-mod.md", "Mod RFC", "draft");

    git(&fixture, &["add", "docs/rfcs/RFC-200-mod.md"]);
    git(&fixture, &["commit", "-m", "add rfc"]);

    // Modify the committed file
    fixture.write_doc(
        "docs/rfcs/RFC-200-mod.md",
        "---\ntitle: \"Mod RFC Updated\"\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2026-01-01\ntags: []\n---\nChanged body\n",
    );

    let app = make_app(&fixture);
    let path = fixture.root().join("docs/rfcs/RFC-200-mod.md");
    assert_eq!(
        app.git_status_cache.get(&path),
        Some(&GitFileStatus::Modified),
    );
}

#[test]
fn test_unchanged_file_no_indicator() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    fixture.write_rfc("RFC-300-unchanged.md", "Unchanged RFC", "draft");

    git(&fixture, &["add", "docs/rfcs/RFC-300-unchanged.md"]);
    git(&fixture, &["commit", "-m", "add rfc"]);

    let app = make_app(&fixture);
    let path = fixture.root().join("docs/rfcs/RFC-300-unchanged.md");
    assert_eq!(app.git_status_cache.get(&path), None);
}

#[test]
fn test_cache_invalidation() {
    let (fixture, _bare) = TestFixture::with_git_remote();
    fixture.write_rfc("RFC-400-cache.md", "Cache RFC", "draft");

    git(&fixture, &["add", "docs/rfcs/RFC-400-cache.md"]);
    git(&fixture, &["commit", "-m", "add rfc"]);

    let mut cache = GitStatusCache::new(fixture.root());
    let path = fixture.root().join("docs/rfcs/RFC-400-cache.md");
    assert_eq!(cache.get(&path), None);

    // Modify the file after cache was built
    fixture.write_doc(
        "docs/rfcs/RFC-400-cache.md",
        "---\ntitle: \"Cache RFC Changed\"\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2026-01-01\ntags: []\n---\nChanged\n",
    );

    // Cache still shows old state
    assert_eq!(cache.get(&path), None);

    cache.invalidate();
    cache.refresh();
    assert_eq!(cache.get(&path), Some(&GitFileStatus::Modified));
}

#[test]
fn test_non_git_repo() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-500-nogit.md", "No Git RFC", "draft");

    let app = make_app(&fixture);
    let path = fixture.root().join("docs/rfcs/RFC-500-nogit.md");
    assert_eq!(app.git_status_cache.get(&path), None);
}

#[test]
fn test_porcelain_parsing() {
    // Untracked
    let (path, status) = parse_porcelain_line("?? docs/rfcs/new.md").unwrap();
    assert_eq!(status, GitFileStatus::New);
    assert_eq!(path, PathBuf::from("docs/rfcs/new.md"));

    // Added (staged)
    let (path, status) = parse_porcelain_line("A  docs/rfcs/added.md").unwrap();
    assert_eq!(status, GitFileStatus::New);
    assert_eq!(path, PathBuf::from("docs/rfcs/added.md"));

    // Added then modified in worktree (partially staged new file)
    let (path, status) = parse_porcelain_line("AM docs/rfcs/added.md").unwrap();
    assert_eq!(status, GitFileStatus::New);
    assert_eq!(path, PathBuf::from("docs/rfcs/added.md"));

    // Modified in worktree only
    let (path, status) = parse_porcelain_line(" M docs/rfcs/changed.md").unwrap();
    assert_eq!(status, GitFileStatus::Modified);
    assert_eq!(path, PathBuf::from("docs/rfcs/changed.md"));

    // Modified and staged
    let (path, status) = parse_porcelain_line("M  docs/rfcs/staged.md").unwrap();
    assert_eq!(status, GitFileStatus::Modified);
    assert_eq!(path, PathBuf::from("docs/rfcs/staged.md"));

    // Partially staged modification (AC-4)
    let (path, status) = parse_porcelain_line("MM docs/rfcs/partial.md").unwrap();
    assert_eq!(status, GitFileStatus::Modified);
    assert_eq!(path, PathBuf::from("docs/rfcs/partial.md"));

    // Renamed (AC-8) - destination shown as Modified
    let (path, status) = parse_porcelain_line("R  old.md -> new.md").unwrap();
    assert_eq!(status, GitFileStatus::Modified);
    assert_eq!(path, PathBuf::from("new.md"));

    // Renamed and modified in worktree
    let (path, status) = parse_porcelain_line("RM old.md -> new.md").unwrap();
    assert_eq!(status, GitFileStatus::Modified);
    assert_eq!(path, PathBuf::from("new.md"));

    // Deleted
    let (path, status) = parse_porcelain_line("D  docs/rfcs/removed.md").unwrap();
    assert_eq!(status, GitFileStatus::Modified);
    assert_eq!(path, PathBuf::from("docs/rfcs/removed.md"));

    // Short/invalid lines return None
    assert!(parse_porcelain_line("??").is_none());
    assert!(parse_porcelain_line("").is_none());
    assert!(parse_porcelain_line("M ").is_none());
}
