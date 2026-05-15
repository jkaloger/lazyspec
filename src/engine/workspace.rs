use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct Workspace {
    pub path: PathBuf,
    pub branch: String,
}

#[derive(Debug)]
struct WorktreeEntry {
    path: PathBuf,
    /// `Some(name)` for a checked-out branch, `None` for detached HEAD.
    branch: Option<String>,
}

pub fn provision_workspace(
    repo_root: &Path,
    workspace_root: &Path,
    base_branch: &str,
    branch: &str,
    claim_id: &str,
) -> Result<Workspace> {
    let worktree_path = workspace_root.join(claim_id);

    match precheck_existing_worktree(repo_root, &worktree_path, branch)? {
        PrecheckOutcome::Reuse => {
            return Ok(Workspace {
                path: worktree_path,
                branch: branch.to_string(),
            });
        }
        PrecheckOutcome::Proceed => {}
    }

    let ref_exists = local_branch_exists(repo_root, branch)?;

    let output = if ref_exists {
        Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["worktree", "add"])
            .arg(&worktree_path)
            .arg(branch)
            .output()
            .context("failed to spawn git worktree add")?
    } else {
        Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["worktree", "add", "-b", branch])
            .arg(&worktree_path)
            .arg(base_branch)
            .output()
            .context("failed to spawn git worktree add")?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree add failed: {}", stderr.trim());
    }

    Ok(Workspace {
        path: worktree_path,
        branch: branch.to_string(),
    })
}

pub fn remove(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "remove", "--force"])
        .arg(workspace_path)
        .output()
        .context("failed to spawn git worktree remove")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree remove failed: {}", stderr.trim());
    }
    Ok(())
}

enum PrecheckOutcome {
    /// Matching registered worktree on the requested branch: reuse as-is.
    Reuse,
    /// No conflict: caller should continue with `git worktree add`.
    Proceed,
}

fn precheck_existing_worktree(
    repo_root: &Path,
    worktree_path: &Path,
    requested_branch: &str,
) -> Result<PrecheckOutcome> {
    let entries = list_worktrees(repo_root)?;
    let target = canonicalize_for_compare(worktree_path);

    let registered = entries
        .iter()
        .find(|e| canonicalize_for_compare(&e.path) == target);

    match registered {
        Some(entry) => match &entry.branch {
            Some(name) if name == requested_branch => Ok(PrecheckOutcome::Reuse),
            Some(name) => bail!(
                "worktree at {} is registered on branch {} but {} was requested; \
                 resolve by removing the existing worktree or re-running with the matching branch",
                worktree_path.display(),
                name,
                requested_branch,
            ),
            None => bail!(
                "worktree at {} is registered with detached HEAD but branch {} was requested; \
                 resolve by removing the existing worktree or re-running with the matching branch",
                worktree_path.display(),
                requested_branch,
            ),
        },
        None => {
            if worktree_path.exists() {
                bail!(
                    "path {} exists on disk but is not a registered git worktree; \
                     remove the directory or run `git worktree prune` before retrying",
                    worktree_path.display(),
                );
            }
            Ok(PrecheckOutcome::Proceed)
        }
    }
}

fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeEntry>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("failed to spawn git worktree list")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git worktree list failed: {}", stderr.trim());
    }
    Ok(parse_worktree_porcelain(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_worktree_porcelain(input: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;
    let mut detached = false;

    let flush = |entries: &mut Vec<WorktreeEntry>,
                 path: &mut Option<PathBuf>,
                 branch: &mut Option<String>,
                 detached: &mut bool| {
        if let Some(p) = path.take() {
            let b = if *detached { None } else { branch.take() };
            entries.push(WorktreeEntry { path: p, branch: b });
        }
        *branch = None;
        *detached = false;
    };

    for line in input.lines() {
        if line.is_empty() {
            flush(
                &mut entries,
                &mut current_path,
                &mut current_branch,
                &mut detached,
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("worktree ") {
            // New record; flush any in-progress entry that had no trailing blank.
            flush(
                &mut entries,
                &mut current_path,
                &mut current_branch,
                &mut detached,
            );
            current_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            current_branch = Some(rest.strip_prefix("refs/heads/").unwrap_or(rest).to_string());
        } else if line == "detached" {
            detached = true;
        }
    }
    flush(
        &mut entries,
        &mut current_path,
        &mut current_branch,
        &mut detached,
    );
    entries
}

/// Canonicalize for path equality (resolves macOS `/var` → `/private/var`).
/// Falls back to the original path if canonicalization fails (e.g. path missing).
fn canonicalize_for_compare(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn local_branch_exists(repo_root: &Path, branch: &str) -> Result<bool> {
    let refname = format!("refs/heads/{}", branch);
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--verify", "--quiet", &refname])
        .output()
        .context("failed to spawn git rev-parse")?;
    Ok(output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(repo: &Path, args: &[&str]) -> std::process::Output {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git command failed to spawn");
        if !output.status.success() {
            panic!(
                "git {:?} failed: stdout={} stderr={}",
                args,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        output
    }

    fn setup_repo() -> (TempDir, PathBuf) {
        let td = tempfile::tempdir().unwrap();
        let repo = td.path().join("repo");
        fs::create_dir(&repo).unwrap();
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["config", "user.email", "test@example.com"]);
        run_git(&repo, &["config", "user.name", "Test"]);
        run_git(&repo, &["config", "commit.gpgsign", "false"]);
        fs::write(repo.join("README.md"), "base\n").unwrap();
        run_git(&repo, &["add", "README.md"]);
        run_git(&repo, &["commit", "-m", "base commit"]);
        (td, repo)
    }

    fn head_sha(repo_or_worktree: &Path) -> String {
        let out = run_git(repo_or_worktree, &["rev-parse", "HEAD"]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn branch_sha(repo: &Path, branch: &str) -> String {
        let out = run_git(repo, &["rev-parse", branch]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn provision_workspace_first_claim_creates_worktree_from_base() {
        // AC7: no pre-existing branch -> create worktree from base
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();
        let base_tip = branch_sha(&repo, "main");

        let ws = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .unwrap();

        assert_eq!(ws.path, workspace_root.join("claim-1"));
        assert_eq!(ws.branch, "agents/STORY-127");
        assert!(ws.path.exists(), "worktree dir should exist");
        assert_eq!(head_sha(&ws.path), base_tip);

        let list = run_git(&repo, &["worktree", "list", "--porcelain"]);
        let list_str = String::from_utf8_lossy(&list.stdout);
        assert!(
            list_str.contains(&ws.path.to_string_lossy().to_string()),
            "worktree should be registered: {}",
            list_str
        );
    }

    #[test]
    fn provision_workspace_reuses_existing_branch_without_rewind() {
        // AC8: existing local branch w/ commit ahead of base -> attach, no rewind
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();

        // Create branch ahead of main with an extra commit.
        run_git(&repo, &["checkout", "-b", "agents/STORY-127"]);
        fs::write(repo.join("extra.txt"), "ahead\n").unwrap();
        run_git(&repo, &["add", "extra.txt"]);
        run_git(&repo, &["commit", "-m", "ahead commit"]);
        let branch_tip = branch_sha(&repo, "agents/STORY-127");
        let base_tip = branch_sha(&repo, "main");
        assert_ne!(branch_tip, base_tip, "branch must be ahead of base");

        // Switch primary repo back to main so worktree can attach branch.
        run_git(&repo, &["checkout", "main"]);

        let ws = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .unwrap();

        assert_eq!(head_sha(&ws.path), branch_tip, "no rewind to base");
        assert_eq!(
            branch_sha(&repo, "agents/STORY-127"),
            branch_tip,
            "branch tip unchanged"
        );
        assert!(
            ws.path.join("extra.txt").exists(),
            "extra commit content present"
        );
    }

    #[test]
    fn remove_workspace_unregisters_worktree() {
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();
        let ws = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .unwrap();
        assert!(ws.path.exists());

        remove(&repo, &ws.path).unwrap();
        assert!(!ws.path.exists(), "worktree dir should be gone");
        let list = run_git(&repo, &["worktree", "list", "--porcelain"]);
        let list_str = String::from_utf8_lossy(&list.stdout);
        assert!(
            !list_str.contains(&ws.path.to_string_lossy().to_string()),
            "worktree should be unregistered: {}",
            list_str
        );
    }

    #[test]
    fn provision_workspace_reentry_with_matching_worktree_is_idempotent() {
        // AC1: re-provision w/ existing registered worktree on matching branch
        // returns Workspace without invoking `git worktree add` again.
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();

        let first = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .unwrap();
        let first_head = head_sha(&first.path);

        let second = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .expect("re-provision should be idempotent");

        assert_eq!(second.path, first.path);
        assert_eq!(second.branch, first.branch);
        assert_eq!(
            head_sha(&second.path),
            first_head,
            "worktree HEAD unchanged"
        );

        let list = run_git(&repo, &["worktree", "list", "--porcelain"]);
        let list_str = String::from_utf8_lossy(&list.stdout);
        let count = list_str
            .lines()
            .filter(|l| l.starts_with("worktree "))
            .count();
        assert_eq!(count, 2, "main repo + one claim worktree: {}", list_str);
    }

    #[test]
    fn provision_workspace_orphan_dir_errors_with_guidance() {
        // AC2: directory exists at worktree path but is not registered.
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();
        let orphan = workspace_root.join("claim-1");
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("stale.txt"), "leftover\n").unwrap();

        let err = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .expect_err("orphan dir should error");

        let msg = format!("{err:#}");
        assert!(
            msg.contains(&orphan.to_string_lossy().to_string()),
            "error should name path: {msg}"
        );
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("prune") || lower.contains("remove"),
            "error should mention prune or remove: {msg}"
        );
    }

    #[test]
    fn provision_workspace_branch_mismatch_errors_naming_both_branches() {
        // AC3: registered worktree on a different branch than requested.
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();
        let worktree_path = workspace_root.join("claim-1");

        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "agents/OTHER",
                worktree_path.to_str().unwrap(),
                "main",
            ],
        );

        let err = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .expect_err("branch mismatch should error");

        let msg = format!("{err:#}");
        assert!(
            msg.contains(&worktree_path.to_string_lossy().to_string()),
            "error should name path: {msg}"
        );
        assert!(
            msg.contains("agents/OTHER"),
            "error should name registered branch: {msg}"
        );
        assert!(
            msg.contains("agents/STORY-127"),
            "error should name requested branch: {msg}"
        );
    }

    #[test]
    fn provision_workspace_missing_branch_recreates_from_base() {
        // AC9: branch was deleted -> behave like first claim
        let (td, repo) = setup_repo();
        let workspace_root = td.path().join("workspaces");
        fs::create_dir(&workspace_root).unwrap();

        // Create and delete the branch.
        run_git(&repo, &["branch", "agents/STORY-127", "main"]);
        run_git(&repo, &["branch", "-D", "agents/STORY-127"]);
        let base_tip = branch_sha(&repo, "main");

        let ws = provision_workspace(
            &repo,
            &workspace_root,
            "main",
            "agents/STORY-127",
            "claim-1",
        )
        .unwrap();

        assert_eq!(head_sha(&ws.path), base_tip, "recreated from base tip");
        assert_eq!(
            branch_sha(&repo, "agents/STORY-127"),
            base_tip,
            "branch ref recreated at base"
        );
    }
}
