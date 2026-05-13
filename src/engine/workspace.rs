use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Workspace {
    pub path: PathBuf,
    pub branch: String,
}

pub fn provision_workspace(
    repo_root: &Path,
    workspace_root: &Path,
    base_branch: &str,
    branch: &str,
    claim_id: &str,
) -> Result<Workspace> {
    let worktree_path = workspace_root.join(claim_id);

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
