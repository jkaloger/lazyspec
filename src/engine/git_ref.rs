use crate::engine::staleness::Drift;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// Wall-clock cap for the network `git fetch` in the sync path, so a slow or
// auth-prompting remote can't wedge the background poll thread (BUG-001).
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// A rebase (`update_clone`, `rebase_onto_remote`) hit a conflict: the rebase
/// was aborted, local commits are intact, and `files` names what collided
/// (BUG-032 AC4/AC5) -- structured so a later `push`/`fetch` JSON surface can
/// report it rather than parsing prose out of the error chain.
#[derive(Debug)]
pub struct RebaseConflict {
    pub clone: PathBuf,
    pub files: Vec<String>,
}

impl std::fmt::Display for RebaseConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "rebase conflict in {}: {} -- resolve with `git -C {} pull --rebase`, then `lazyspec push`",
            self.clone.display(),
            self.files.join(", "),
            self.clone.display()
        )
    }
}

impl std::error::Error for RebaseConflict {}

/// A rebase was already in progress in `clone` (started by an earlier, unrelated
/// `git rebase` -- possibly one the user is mid-resolving) when `rebase_onto`
/// went to start its own. Nothing here touches it: aborting would destroy
/// work the user may have already resolved and `git add`ed.
#[derive(Debug)]
pub struct RebaseInProgress {
    pub clone: PathBuf,
}

impl std::fmt::Display for RebaseInProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "a rebase is already in progress in {} -- finish it with `git -C {} rebase --continue` (or `--abort`), then re-run this command",
            self.clone.display(),
            self.clone.display()
        )
    }
}

impl std::error::Error for RebaseInProgress {}

/// `clone` had an uncommitted change (`git status --porcelain` non-empty) when
/// a rebase (`update_clone`/`rebase_onto_remote`/`push`'s pre-rebase step)
/// went to run there. Rebasing over a dirty tree either drags the
/// uncommitted content into every replayed commit or refuses outright, and
/// either way it is the human's to resolve there, not lazyspec's to guess
/// at -- so this bails before touching the rebase, structured the same way
/// as [`RebaseInProgress`] and [`RebaseConflict`] so a `--json` surface can
/// report it rather than parsing prose out of the error chain.
#[derive(Debug)]
pub struct UncommittedChanges {
    pub clone: PathBuf,
}

impl std::fmt::Display for UncommittedChanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} has uncommitted changes -- commit them there (`git -C {} commit -am ...`) or stash them (`git -C {} stash`), then re-run this command",
            self.clone.display(),
            self.clone.display(),
            self.clone.display(),
        )
    }
}

impl std::error::Error for UncommittedChanges {}

pub trait GitRefOps {
    fn resolve_ref(&self, root: &Path, refname: &str) -> Result<Option<String>>;
    fn list_refs(&self, root: &Path, pattern: &str) -> Result<Vec<(String, String)>>;
    fn read_ref_blob(&self, root: &Path, sha: &str, path: &str) -> Result<String>;
    fn create_commit(
        &self,
        root: &Path,
        refname: &str,
        files: &[(&str, &str)],
        parent: Option<&str>,
    ) -> Result<String>;
    fn create_ref_commit(
        &self,
        root: &Path,
        refname: &str,
        files: &[(&str, &str)],
    ) -> Result<String>;
    fn update_ref(&self, root: &Path, refname: &str, new_sha: &str, old_sha: &str) -> Result<()>;
    fn delete_ref(&self, root: &Path, refname: &str) -> Result<()>;
    fn fetch_refs(&self, root: &Path, remote: &str, pattern: &str) -> Result<()>;
    /// Clone `remote` into `dest` as a single-branch checkout of `branch`, or
    /// of the remote's default branch when `None` (RFC-072 "The git store").
    fn clone_repo(&self, remote: &str, branch: Option<&str>, dest: &Path) -> Result<()>;
    /// Bring an existing clone to the tip of `branch` (or the remote's default
    /// branch when `None`): fetch, then rebase local commits onto the fetched
    /// head (BUG-032 AC5) -- never `reset --hard`, which would discard commits
    /// a write made locally and never pushed. A conflicted rebase is aborted
    /// and returned as [`RebaseConflict`]; local commits are intact either way.
    /// A rebase already in progress is left alone and returned as
    /// [`RebaseInProgress`] rather than aborted.
    /// Built on [`rebase_onto_remote`](GitRefOps::rebase_onto_remote).
    fn update_clone(&self, clone: &Path, branch: Option<&str>) -> Result<()>;
    /// Stage everything in `clone` and commit it as `message`. Never pushes
    /// (BUG-032 AC2): a write lands locally, and `push` is the separate,
    /// explicit step that publishes it. A clean tree commits nothing and
    /// returns `Ok`. A commit failure resets the clone to its pre-call `HEAD`
    /// before erroring, so a rejected write leaves no orphan file for
    /// `next_number` to count (STORY-282 AC6).
    fn commit(&self, clone: &Path, message: &str) -> Result<()>;
    /// Fetch `branch` on `origin` (or the branch `HEAD` tracks when `None`,
    /// updating the remote-tracking ref) and rebase local commits onto it
    /// (BUG-032 AC3/AC5) -- shared by `update_clone` and, ahead of `push`, a
    /// caller that needs the rebase and a check (e.g. a duplicate-id guard)
    /// to run before anything is pushed. A rebase conflict this call caused
    /// is aborted and returned as [`RebaseConflict`]; local commits are
    /// intact either way. A rebase already in progress before this call --
    /// started elsewhere, maybe mid-resolution -- is never touched and is
    /// returned as [`RebaseInProgress`] instead.
    fn rebase_onto_remote(&self, clone: &Path, branch: Option<&str>) -> Result<()>;
    /// Push `HEAD` to `branch` on `origin` (or the branch `HEAD` tracks when
    /// `None`) and report how many commits went -- 0 when nothing was ahead of
    /// the remote-tracking branch, which pushes nothing (BUG-032 AC3). Callers
    /// rebase first (`rebase_onto_remote`); this never fetches or rebases
    /// itself, so a caller can run its own check between the two.
    fn push(&self, clone: &Path, branch: Option<&str>) -> Result<usize>;
    /// How many local commits in `clone` are ahead of `branch`'s remote-tracking
    /// ref (or the branch `HEAD` tracks when `None`) -- what `push` would push,
    /// without fetching or pushing anything itself.
    fn unpushed(&self, clone: &Path, branch: Option<&str>) -> Result<usize>;
    /// The paths git considers added (`--diff-filter=A`) between `branch`'s
    /// remote-tracking ref (or the branch `HEAD` tracks when `None`) and
    /// `HEAD`, relative to `clone` -- what `push`'s duplicate-id check
    /// (BUG-032 AC6) scans for a doc whose id now collides with a sibling.
    /// Never fetches; call after `rebase_onto_remote` so the range reads the
    /// rebased tree.
    fn added_files(&self, clone: &Path, branch: Option<&str>) -> Result<Vec<String>>;
    /// Whether `clone`'s working tree has anything uncommitted -- staged,
    /// unstaged, or untracked (`git status --porcelain`). What the BUG-032
    /// AC8 legacy-clone migration checks alongside [`unpushed`](GitRefOps::unpushed)
    /// before deleting an old per-type clone: either one is a reason to keep it.
    fn has_uncommitted_changes(&self, clone: &Path) -> Result<bool>;
    fn push_ref(&self, root: &Path, remote: &str, refname: &str) -> Result<()>;
    fn push_new_ref(&self, root: &Path, remote: &str, refname: &str, new_sha: &str) -> Result<()>;
    fn delete_remote_ref(
        &self,
        root: &Path,
        remote: &str,
        refname: &str,
        expected_old: Option<&str>,
    ) -> Result<()>;
    fn push_ref_with_lease(
        &self,
        root: &Path,
        remote: &str,
        refname: &str,
        new_sha: &str,
        expected_old: Option<&str>,
    ) -> Result<()>;
    fn read_commit_timestamp(&self, root: &Path, sha: &str) -> Result<DateTime<Utc>>;
    /// The commit `HEAD` points at. Errors where `HEAD` cannot be read at all --
    /// a repository with no commits, or a directory that is not one.
    fn head(&self, root: &Path) -> Result<String>;
    /// The `(from, to)` path pairs git detected as renames between two commits,
    /// relative to `root` (RFC-068). Paths that moved in or out of `root` are
    /// not pairs there and are omitted.
    fn renames(&self, root: &Path, from: &str, to: &str) -> Result<Vec<(String, String)>>;
    /// How much changed between two commits under `paths`, as file, insertion
    /// and deletion counts (RFC-069). `paths` are `governs` globs relative to
    /// `root`, matched with `globset` exactly as [`Store::governing`] matches
    /// them, so the counted set is the governed set; an empty list counts the
    /// whole tree.
    ///
    /// [`Store::governing`]: crate::engine::store::Store::governing
    fn diff_stat(&self, root: &Path, from: &str, to: &str, paths: &[String]) -> Result<Drift>;
}

/// The git-ref client seam as an object-safe trait for `GitRefStore`'s boxed
/// client. Blanket-implemented for anything that satisfies [`GitRefOps`].
pub trait GitRefClient: GitRefOps + crate::engine::gh::AsAny {}

impl<T: GitRefOps + crate::engine::gh::AsAny> GitRefClient for T {}

pub struct GitCli;

impl GitCli {
    fn run_git(&self, root: &Path, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new("git").args(args).current_dir(root).output()?;
        Ok(output)
    }

    fn run_git_with_stdin(
        &self,
        root: &Path,
        args: &[&str],
        stdin_data: &[u8],
    ) -> Result<std::process::Output> {
        use std::io::Write;
        let mut child = Command::new("git")
            .args(args)
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_data)?;
        }

        let output = child.wait_with_output()?;
        Ok(output)
    }

    /// `branch`, or the clone's current branch when `None` -- what
    /// `update_clone`/`rebase_onto_remote`/`push`/`unpushed` resolve a caller's `Option<&str>`
    /// against so a type with no declared `branch` still fetches, rebases and
    /// pushes the branch its clone actually tracks.
    fn resolve_branch(&self, clone: &Path, branch: Option<&str>) -> Result<String> {
        if let Some(branch) = branch {
            return Ok(branch.to_string());
        }
        let output = self.run_git(clone, &["symbolic-ref", "--short", "HEAD"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "could not resolve the clone's current branch: {}",
                stderr.trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Fetch `branch` from `origin`, updating both `FETCH_HEAD` and the
    /// remote-tracking ref `origin/<branch>` (a plain `git fetch origin
    /// <branch>` opportunistically updates the tracking ref when it matches
    /// the clone's configured fetch refspec), so `unpushed`'s count reads
    /// current.
    fn fetch_branch(&self, clone: &Path, branch: &str) -> Result<()> {
        let mut fetch = Command::new("git");
        fetch
            .args(["fetch", "origin", branch])
            .current_dir(clone)
            .env("GIT_TERMINAL_PROMPT", "0");
        let output = crate::engine::subprocess::output_with_timeout(fetch, FETCH_TIMEOUT)
            .context("git fetch")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git fetch failed: {}", stderr.trim());
        }
        Ok(())
    }

    /// Rebase the clone's current branch onto `onto` (`FETCH_HEAD` or an
    /// `origin/<branch>` tracking ref). A clean rebase replays local commits on
    /// top; a conflicted one aborts -- local commits stay exactly as they were
    /// -- and reports [`RebaseConflict`] with the files that collided. A rebase
    /// already in progress (started outside this call, maybe mid-resolution)
    /// is left untouched and reported as [`RebaseInProgress`]; this never
    /// aborts a rebase it did not itself start.
    fn rebase_onto(&self, clone: &Path, onto: &str) -> Result<()> {
        // Checked in this order deliberately: a rebase already in progress
        // (started outside this call) typically leaves conflict markers
        // uncommitted, and that dirt is not the human's to resolve by
        // stashing -- `rebase --continue`/`--abort` is. Only a tree that is
        // dirty *and not* already mid-rebase is `UncommittedChanges`.
        if self.rebase_in_progress(clone)?.is_some() {
            return Err(RebaseInProgress {
                clone: clone.to_path_buf(),
            }
            .into());
        }
        if self.has_uncommitted_changes(clone)? {
            return Err(UncommittedChanges {
                clone: clone.to_path_buf(),
            }
            .into());
        }
        let output = self.run_git(clone, &["rebase", onto])?;
        if output.status.success() {
            return Ok(());
        }
        let files = self.conflicted_files(clone).unwrap_or_default();
        let _ = self.run_git(clone, &["rebase", "--abort"]);
        if files.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!("git rebase onto {onto} failed: {stderr}");
        }
        Err(RebaseConflict {
            clone: clone.to_path_buf(),
            files,
        }
        .into())
    }

    /// The path of `rebase-merge` or `rebase-apply` under `clone`'s git dir,
    /// when either exists -- git's own marker for a rebase already in
    /// progress there, checked before `rebase_onto` starts one of its own.
    fn rebase_in_progress(&self, clone: &Path) -> Result<Option<PathBuf>> {
        for git_path in ["rebase-merge", "rebase-apply"] {
            let output = self.run_git(clone, &["rev-parse", "--git-path", git_path])?;
            if !output.status.success() {
                continue;
            }
            let reported = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let resolved = if Path::new(&reported).is_absolute() {
                PathBuf::from(reported)
            } else {
                clone.join(&reported)
            };
            if resolved.exists() {
                return Ok(Some(resolved));
            }
        }
        Ok(None)
    }

    fn conflicted_files(&self, clone: &Path) -> Result<Vec<String>> {
        let output = self.run_git(clone, &["diff", "--name-only", "--diff-filter=U"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git diff --diff-filter=U failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    fn count_ahead(&self, clone: &Path, upstream: &str) -> Result<usize> {
        let range = format!("{upstream}..HEAD");
        let output = self.run_git(clone, &["rev-list", "--count", &range])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git rev-list --count failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(0))
    }
}

impl GitRefOps for GitCli {
    fn resolve_ref(&self, root: &Path, refname: &str) -> Result<Option<String>> {
        let output = self.run_git(root, &["rev-parse", "--verify", refname])?;
        if !output.status.success() {
            return Ok(None);
        }
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Some(sha))
    }

    fn list_refs(&self, root: &Path, pattern: &str) -> Result<Vec<(String, String)>> {
        let format_arg = "%(refname)\t%(objectname)";
        let output = self.run_git(
            root,
            &["for-each-ref", &format!("--format={}", format_arg), pattern],
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git for-each-ref failed: {}", stderr.trim());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let refs = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|line| {
                let (refname, sha) = line.split_once('\t')?;
                Some((refname.to_string(), sha.to_string()))
            })
            .collect();
        Ok(refs)
    }

    fn read_ref_blob(&self, root: &Path, sha: &str, path: &str) -> Result<String> {
        let spec = format!("{}:{}", sha, path);
        let output = self.run_git(root, &["show", &spec])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git show failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn create_commit(
        &self,
        root: &Path,
        _refname: &str,
        files: &[(&str, &str)],
        parent: Option<&str>,
    ) -> Result<String> {
        let mut tree_entries = Vec::new();

        for (path, content) in files {
            let output = self.run_git_with_stdin(
                root,
                &["hash-object", "-w", "--stdin"],
                content.as_bytes(),
            )?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("git hash-object failed: {}", stderr.trim());
            }
            let blob_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            tree_entries.push(format!("100644 blob {}\t{}", blob_sha, path));
        }

        let tree_input = tree_entries.join("\n") + "\n";
        let output = self.run_git_with_stdin(root, &["mktree"], tree_input.as_bytes())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git mktree failed: {}", stderr.trim());
        }
        let tree_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let output = if let Some(parent_sha) = parent {
            self.run_git(
                root,
                &[
                    "commit-tree",
                    &tree_sha,
                    "-p",
                    parent_sha,
                    "-m",
                    "ref commit",
                ],
            )?
        } else {
            self.run_git(root, &["commit-tree", &tree_sha, "-m", "ref commit"])?
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git commit-tree failed: {}", stderr.trim());
        }
        let commit_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(commit_sha)
    }

    fn create_ref_commit(
        &self,
        root: &Path,
        refname: &str,
        files: &[(&str, &str)],
    ) -> Result<String> {
        let commit_sha = self.create_commit(root, refname, files, None)?;

        let null_sha = "0000000000000000000000000000000000000000";
        let output = self.run_git(root, &["update-ref", refname, &commit_sha, null_sha])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git update-ref failed: {}", stderr.trim());
        }

        Ok(commit_sha)
    }

    fn update_ref(&self, root: &Path, refname: &str, new_sha: &str, old_sha: &str) -> Result<()> {
        let output = self.run_git(root, &["update-ref", refname, new_sha, old_sha])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git update-ref CAS failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn delete_ref(&self, root: &Path, refname: &str) -> Result<()> {
        let output = self.run_git(root, &["update-ref", "-d", refname])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git update-ref -d failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn fetch_refs(&self, root: &Path, remote: &str, pattern: &str) -> Result<()> {
        let refspec = format!("+{}:{}", pattern, pattern);
        // Build the command directly (not via `run_git`) so the fetch runs under a
        // timeout and with credential prompts disabled -- a prompting remote would
        // otherwise block forever on a headless poll thread.
        let mut cmd = Command::new("git");
        cmd.args(["fetch", "--prune", remote, &refspec])
            .current_dir(root)
            .env("GIT_TERMINAL_PROMPT", "0");
        let output = crate::engine::subprocess::output_with_timeout(cmd, FETCH_TIMEOUT)
            .context("git fetch")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git fetch failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn clone_repo(&self, remote: &str, branch: Option<&str>, dest: &Path) -> Result<()> {
        let mut cmd = Command::new("git");
        cmd.args(["clone", "--single-branch"]);
        if let Some(branch) = branch {
            cmd.args(["--branch", branch]);
        }
        cmd.arg(remote).arg(dest).env("GIT_TERMINAL_PROMPT", "0");
        let output = crate::engine::subprocess::output_with_timeout(cmd, FETCH_TIMEOUT)
            .context("git clone")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git clone failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn update_clone(&self, clone: &Path, branch: Option<&str>) -> Result<()> {
        self.rebase_onto_remote(clone, branch)
    }

    fn commit(&self, clone: &Path, message: &str) -> Result<()> {
        let head = self.head(clone)?;
        let output = self.run_git(clone, &["add", "-A"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git add failed: {}", stderr.trim());
        }
        let staged = self.run_git(clone, &["diff", "--cached", "--quiet"])?;
        if staged.status.success() {
            return Ok(());
        }
        let output = self.run_git(clone, &["commit", "-q", "-m", message])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            self.run_git(clone, &["reset", "--hard", &head])?;
            bail!("git commit failed: {}", stderr);
        }
        Ok(())
    }

    fn rebase_onto_remote(&self, clone: &Path, branch: Option<&str>) -> Result<()> {
        let branch = self.resolve_branch(clone, branch)?;
        self.fetch_branch(clone, &branch)?;
        self.rebase_onto(clone, &format!("origin/{branch}"))
    }

    fn push(&self, clone: &Path, branch: Option<&str>) -> Result<usize> {
        let branch = self.resolve_branch(clone, branch)?;
        let tracking = format!("origin/{branch}");
        let ahead = self.count_ahead(clone, &tracking)?;
        if ahead == 0 {
            return Ok(0);
        }
        let refspec = format!("HEAD:{branch}");
        let mut push = Command::new("git");
        push.args(["push", "origin", &refspec])
            .current_dir(clone)
            .env("GIT_TERMINAL_PROMPT", "0");
        let output = crate::engine::subprocess::output_with_timeout(push, FETCH_TIMEOUT)
            .context("git push")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git push failed: {}", stderr.trim());
        }
        Ok(ahead)
    }

    fn unpushed(&self, clone: &Path, branch: Option<&str>) -> Result<usize> {
        let branch = self.resolve_branch(clone, branch)?;
        self.count_ahead(clone, &format!("origin/{branch}"))
    }

    fn added_files(&self, clone: &Path, branch: Option<&str>) -> Result<Vec<String>> {
        let branch = self.resolve_branch(clone, branch)?;
        let range = format!("origin/{branch}..HEAD");
        // `--no-renames`: without it, deleting an already-pushed doc and adding a
        // similar new one in the same range reads as a rename (`R`), not an add,
        // and the new path never reaches `--diff-filter=A`.
        let output = self.run_git(
            clone,
            &[
                "diff",
                "--name-only",
                "--diff-filter=A",
                "--no-renames",
                &range,
            ],
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git diff --diff-filter=A failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    fn has_uncommitted_changes(&self, clone: &Path) -> Result<bool> {
        let output = self.run_git(clone, &["status", "--porcelain"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git status failed: {}", stderr.trim());
        }
        Ok(!output.stdout.is_empty())
    }

    fn push_ref(&self, root: &Path, remote: &str, refname: &str) -> Result<()> {
        let output = self.run_git(root, &["push", remote, refname])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git push failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn push_new_ref(&self, root: &Path, remote: &str, refname: &str, new_sha: &str) -> Result<()> {
        // An empty lease expectation ("<refname>:") requires the ref to be
        // absent on the remote, so a clone that concurrently claimed the same
        // number gets its push rejected rather than silently overwritten.
        let lease_arg = format!("--force-with-lease={}:", refname);
        let refspec = format!("{}:{}", new_sha, refname);
        let output = self.run_git(root, &["push", &lease_arg, remote, &refspec])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git push (new ref) failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn delete_remote_ref(
        &self,
        root: &Path,
        remote: &str,
        refname: &str,
        expected_old: Option<&str>,
    ) -> Result<()> {
        let delete_spec = format!(":{}", refname);
        let output = match expected_old {
            Some(sha) => {
                let lease_arg = format!("--force-with-lease={}:{}", refname, sha);
                self.run_git(root, &["push", &lease_arg, remote, &delete_spec])?
            }
            None => self.run_git(root, &["push", remote, &delete_spec])?,
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git push --delete failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn push_ref_with_lease(
        &self,
        root: &Path,
        remote: &str,
        refname: &str,
        new_sha: &str,
        expected_old: Option<&str>,
    ) -> Result<()> {
        let lease_arg = match expected_old {
            Some(sha) => format!("--force-with-lease={}:{}", refname, sha),
            None => format!("--force-with-lease={}", refname),
        };
        // Push by SHA so dangling commit objects without a local ref can be pushed.
        // The caller advances the local ref only after this returns Ok.
        let refspec = format!("{}:{}", new_sha, refname);
        let output = self.run_git(root, &["push", &lease_arg, remote, &refspec])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git push --force-with-lease failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn read_commit_timestamp(&self, root: &Path, sha: &str) -> Result<DateTime<Utc>> {
        let output = self.run_git(root, &["cat-file", "-p", sha])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git cat-file failed: {}", stderr.trim());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("committer ") {
                let parts: Vec<&str> = rest.rsplitn(3, ' ').collect();
                if parts.len() < 3 {
                    bail!("unexpected committer line format: {}", line);
                }
                let timestamp: i64 = parts[1]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("invalid committer timestamp: {}", parts[1]))?;
                return DateTime::from_timestamp(timestamp, 0)
                    .ok_or_else(|| anyhow::anyhow!("invalid unix timestamp: {}", timestamp));
            }
        }
        bail!("no committer line found in commit {}", sha)
    }

    fn head(&self, root: &Path) -> Result<String> {
        let Some(sha) = self.resolve_ref(root, "HEAD")? else {
            bail!("could not read HEAD in {}", root.display());
        };
        Ok(sha)
    }

    fn renames(&self, root: &Path, from: &str, to: &str) -> Result<Vec<(String, String)>> {
        // `--relative` reports paths relative to the working directory, which is
        // `root`, so the pairs read in the same terms as the `governs` globs they
        // are matched against.
        let range = format!("{}..{}", from, to);
        let output = self.run_git(root, &["diff", "-M", "--name-status", "--relative", &range])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git diff --name-status failed: {}", stderr.trim());
        }
        Ok(parse_renames(&String::from_utf8_lossy(&output.stdout)))
    }

    fn diff_stat(&self, root: &Path, from: &str, to: &str, paths: &[String]) -> Result<Drift> {
        // The globs are never handed to git. A pathspec is matched in git's own
        // wildmatch dialect, which agrees with `globset` on `*` and `**` but not
        // on brace alternates (`src/{cli,tui}/**` matches nothing there) nor on
        // a bare directory (`src` is its whole subtree there, one missing file
        // here) -- so git only names the files that changed, and the same
        // `globset` matchers `Store::governing` uses decide which are governed.
        //
        // `--numstat` is the per-file form of `--shortstat`; `--relative`
        // reports paths relative to `root`, as `governs` globs read; and
        // `--no-renames` keeps every row a single path.
        let range = format!("{}..{}", from, to);
        let governed = governed_matcher(paths)?;
        let args = ["diff", "--numstat", "--relative", "--no-renames", &range];
        let output = self.run_git(root, &args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git diff --numstat failed: {}", stderr.trim());
        }
        Ok(sum_numstat(
            &String::from_utf8_lossy(&output.stdout),
            governed.as_ref(),
        ))
    }
}

/// The `governs` globs as one matcher, or `None` for an empty list, which counts
/// every changed file rather than none. Compiled the same way the loader
/// compiles them (`store::loader::compile_governs`), so a glob that governs a
/// file here governs it there.
fn governed_matcher(paths: &[String]) -> Result<Option<GlobSet>> {
    if paths.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for glob in paths {
        builder.add(Glob::new(glob).with_context(|| format!("invalid governs glob '{glob}'"))?);
    }
    Ok(Some(builder.build()?))
}

/// The rows of `git diff --numstat`: insertions, deletions and path, tab
/// separated, with `-` for both counts of a binary file -- which still changed,
/// so it counts as a file and contributes no lines, exactly as `--shortstat`
/// reports it.
fn sum_numstat(stdout: &str, governed: Option<&GlobSet>) -> Drift {
    let mut drift = Drift::default();
    for line in stdout.lines() {
        let mut fields = line.split('\t');
        let (Some(insertions), Some(deletions), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if governed.is_some_and(|set| !set.is_match(path)) {
            continue;
        }
        drift.files += 1;
        drift.insertions += insertions.parse().unwrap_or(0);
        drift.deletions += deletions.parse().unwrap_or(0);
    }
    drift
}

/// The rename rows of `git diff -M --name-status` output: status `R<score>`
/// followed by the old and the new path, tab separated. Every other status
/// (added, modified, deleted) carries one path and is not a rename.
fn parse_renames(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter(|line| line.starts_with('R'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let _status = fields.next()?;
            let from = fields.next()?;
            let to = fields.next()?;
            Some((from.to_string(), to.to_string()))
        })
        .collect()
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::*;
    use chrono::{DateTime, Utc};
    use std::cell::RefCell;

    type RefList = Vec<(String, String)>;

    /// What the fake's `head` returns when no result was queued.
    pub const FAKE_HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

    pub struct MockGitRefClient {
        pub resolve_results: RefCell<Vec<Result<Option<String>>>>,
        pub list_results: RefCell<Vec<Result<RefList>>>,
        pub read_blob_results: RefCell<Vec<Result<String>>>,
        pub create_commit_results: RefCell<Vec<Result<String>>>,
        pub create_ref_commit_results: RefCell<Vec<Result<String>>>,
        pub update_ref_results: RefCell<Vec<Result<()>>>,
        pub delete_ref_results: RefCell<Vec<Result<()>>>,
        pub fetch_results: RefCell<Vec<Result<()>>>,
        pub clone_results: RefCell<Vec<Result<()>>>,
        pub update_clone_results: RefCell<Vec<Result<()>>>,
        pub commit_results: RefCell<Vec<Result<()>>>,
        pub rebase_onto_remote_results: RefCell<Vec<Result<()>>>,
        pub push_commits_results: RefCell<Vec<Result<usize>>>,
        pub unpushed_results: RefCell<Vec<Result<usize>>>,
        pub added_files_results: RefCell<Vec<Result<Vec<String>>>>,
        pub has_uncommitted_changes_results: RefCell<Vec<Result<bool>>>,
        pub push_results: RefCell<Vec<Result<()>>>,
        pub push_new_ref_results: RefCell<Vec<Result<()>>>,
        pub delete_remote_results: RefCell<Vec<Result<()>>>,
        pub push_with_lease_results: RefCell<Vec<Result<()>>>,
        pub read_commit_timestamp_results: RefCell<Vec<Result<DateTime<Utc>>>>,
        pub head_results: RefCell<Vec<Result<String>>>,
        /// What `renames` reports, on every call. Not a queue: a caller asking
        /// once per rotted glob would otherwise see the fixture only the first
        /// time.
        pub renames_pairs: RefCell<RefList>,
        /// Makes `renames` fail on every call, standing in for a `reviewed` sha
        /// the repository cannot resolve.
        pub renames_error: RefCell<Option<String>>,
        /// What `diff_stat` reports, on every call -- not a queue, for the same
        /// reason `renames_pairs` is not one.
        pub diff_stat_drift: RefCell<Drift>,
        /// Makes `diff_stat` fail on every call, standing in for a range git
        /// cannot resolve.
        pub diff_stat_error: RefCell<Option<String>>,
        /// Shared, so a test that hands the mock to something owning it -- a
        /// `Box<dyn GitRefOps>` on a validation rule -- can still read back what
        /// was asked of git. See [`MockGitRefClient::call_log`].
        calls: std::rc::Rc<RefCell<Vec<String>>>,
        /// The `doc.md` blob content passed to each `create_commit`, in call
        /// order. Lets tests assert what was serialized into the ref, which the
        /// `calls` string log (parent SHA only) does not capture.
        pub committed_blobs: RefCell<Vec<String>>,
    }

    impl Default for MockGitRefClient {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockGitRefClient {
        pub fn new() -> Self {
            Self {
                resolve_results: RefCell::new(vec![]),
                list_results: RefCell::new(vec![]),
                read_blob_results: RefCell::new(vec![]),
                create_commit_results: RefCell::new(vec![]),
                create_ref_commit_results: RefCell::new(vec![]),
                update_ref_results: RefCell::new(vec![]),
                delete_ref_results: RefCell::new(vec![]),
                fetch_results: RefCell::new(vec![]),
                clone_results: RefCell::new(vec![]),
                update_clone_results: RefCell::new(vec![]),
                commit_results: RefCell::new(vec![]),
                rebase_onto_remote_results: RefCell::new(vec![]),
                push_commits_results: RefCell::new(vec![]),
                unpushed_results: RefCell::new(vec![]),
                added_files_results: RefCell::new(vec![]),
                has_uncommitted_changes_results: RefCell::new(vec![]),
                push_results: RefCell::new(vec![]),
                push_new_ref_results: RefCell::new(vec![]),
                delete_remote_results: RefCell::new(vec![]),
                push_with_lease_results: RefCell::new(vec![]),
                read_commit_timestamp_results: RefCell::new(vec![]),
                head_results: RefCell::new(vec![]),
                renames_pairs: RefCell::new(vec![]),
                renames_error: RefCell::new(None),
                diff_stat_drift: RefCell::new(Drift::default()),
                diff_stat_error: RefCell::new(None),
                calls: std::rc::Rc::new(RefCell::new(vec![])),
                committed_blobs: RefCell::new(vec![]),
            }
        }

        /// A handle on the call log that outlives handing the mock away.
        pub fn call_log(&self) -> std::rc::Rc<RefCell<Vec<String>>> {
            std::rc::Rc::clone(&self.calls)
        }

        pub fn with_resolve_result(self, result: Result<Option<String>>) -> Self {
            self.resolve_results.borrow_mut().push(result);
            self
        }

        pub fn with_list_result(self, result: Result<Vec<(String, String)>>) -> Self {
            self.list_results.borrow_mut().push(result);
            self
        }

        pub fn with_read_blob_result(self, result: Result<String>) -> Self {
            self.read_blob_results.borrow_mut().push(result);
            self
        }

        pub fn with_create_commit_result(self, result: Result<String>) -> Self {
            self.create_commit_results.borrow_mut().push(result);
            self
        }

        pub fn with_create_ref_commit_result(self, result: Result<String>) -> Self {
            self.create_ref_commit_results.borrow_mut().push(result);
            self
        }

        pub fn with_update_ref_result(self, result: Result<()>) -> Self {
            self.update_ref_results.borrow_mut().push(result);
            self
        }

        pub fn with_delete_ref_result(self, result: Result<()>) -> Self {
            self.delete_ref_results.borrow_mut().push(result);
            self
        }

        pub fn with_fetch_result(self, result: Result<()>) -> Self {
            self.fetch_results.borrow_mut().push(result);
            self
        }

        pub fn with_clone_result(self, result: Result<()>) -> Self {
            self.clone_results.borrow_mut().push(result);
            self
        }

        pub fn with_update_clone_result(self, result: Result<()>) -> Self {
            self.update_clone_results.borrow_mut().push(result);
            self
        }

        pub fn with_commit_result(self, result: Result<()>) -> Self {
            self.commit_results.borrow_mut().push(result);
            self
        }

        pub fn with_rebase_onto_remote_result(self, result: Result<()>) -> Self {
            self.rebase_onto_remote_results.borrow_mut().push(result);
            self
        }

        pub fn with_push_commits_result(self, result: Result<usize>) -> Self {
            self.push_commits_results.borrow_mut().push(result);
            self
        }

        pub fn with_unpushed_result(self, result: Result<usize>) -> Self {
            self.unpushed_results.borrow_mut().push(result);
            self
        }

        pub fn with_added_files_result(self, result: Result<Vec<String>>) -> Self {
            self.added_files_results.borrow_mut().push(result);
            self
        }

        pub fn with_has_uncommitted_changes_result(self, result: Result<bool>) -> Self {
            self.has_uncommitted_changes_results
                .borrow_mut()
                .push(result);
            self
        }

        pub fn with_push_result(self, result: Result<()>) -> Self {
            self.push_results.borrow_mut().push(result);
            self
        }

        pub fn with_push_new_ref_result(self, result: Result<()>) -> Self {
            self.push_new_ref_results.borrow_mut().push(result);
            self
        }

        pub fn with_delete_remote_result(self, result: Result<()>) -> Self {
            self.delete_remote_results.borrow_mut().push(result);
            self
        }

        pub fn with_push_with_lease_result(self, result: Result<()>) -> Self {
            self.push_with_lease_results.borrow_mut().push(result);
            self
        }

        pub fn with_read_commit_timestamp_result(self, result: Result<DateTime<Utc>>) -> Self {
            self.read_commit_timestamp_results.borrow_mut().push(result);
            self
        }

        pub fn with_head_result(self, result: Result<String>) -> Self {
            self.head_results.borrow_mut().push(result);
            self
        }

        pub fn with_renames(self, pairs: &[(&str, &str)]) -> Self {
            *self.renames_pairs.borrow_mut() = pairs
                .iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect();
            self
        }

        pub fn with_renames_error(self, message: &str) -> Self {
            *self.renames_error.borrow_mut() = Some(message.to_string());
            self
        }

        pub fn with_diff_stat(self, drift: Drift) -> Self {
            *self.diff_stat_drift.borrow_mut() = drift;
            self
        }

        pub fn with_diff_stat_error(self, message: &str) -> Self {
            *self.diff_stat_error.borrow_mut() = Some(message.to_string());
            self
        }

        fn pop_or_default<T: Default>(queue: &RefCell<Vec<Result<T>>>) -> Result<T> {
            let mut q = queue.borrow_mut();
            if q.is_empty() {
                Ok(T::default())
            } else {
                q.remove(0)
            }
        }
    }

    impl GitRefOps for MockGitRefClient {
        fn resolve_ref(&self, _root: &Path, refname: &str) -> Result<Option<String>> {
            self.calls
                .borrow_mut()
                .push(format!("resolve_ref:{}", refname));
            Self::pop_or_default(&self.resolve_results)
        }

        fn list_refs(&self, _root: &Path, pattern: &str) -> Result<Vec<(String, String)>> {
            self.calls
                .borrow_mut()
                .push(format!("list_refs:{}", pattern));
            Self::pop_or_default(&self.list_results)
        }

        fn read_ref_blob(&self, _root: &Path, sha: &str, path: &str) -> Result<String> {
            self.calls
                .borrow_mut()
                .push(format!("read_ref_blob:{}:{}", sha, path));
            Self::pop_or_default(&self.read_blob_results)
        }

        fn create_commit(
            &self,
            _root: &Path,
            refname: &str,
            files: &[(&str, &str)],
            parent: Option<&str>,
        ) -> Result<String> {
            self.calls
                .borrow_mut()
                .push(format!("create_commit:{}:parent={:?}", refname, parent));
            if let Some((_, content)) = files.iter().find(|(name, _)| *name == "doc.md") {
                self.committed_blobs.borrow_mut().push(content.to_string());
            }
            Self::pop_or_default(&self.create_commit_results)
        }

        fn create_ref_commit(
            &self,
            _root: &Path,
            refname: &str,
            _files: &[(&str, &str)],
        ) -> Result<String> {
            self.calls
                .borrow_mut()
                .push(format!("create_ref_commit:{}", refname));
            Self::pop_or_default(&self.create_ref_commit_results)
        }

        fn update_ref(
            &self,
            _root: &Path,
            refname: &str,
            new_sha: &str,
            old_sha: &str,
        ) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("update_ref:{}:{}:{}", refname, new_sha, old_sha));
            Self::pop_or_default(&self.update_ref_results)
        }

        fn delete_ref(&self, _root: &Path, refname: &str) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("delete_ref:{}", refname));
            Self::pop_or_default(&self.delete_ref_results)
        }

        fn fetch_refs(&self, _root: &Path, remote: &str, pattern: &str) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("fetch_refs:{}:{}", remote, pattern));
            Self::pop_or_default(&self.fetch_results)
        }

        fn clone_repo(&self, remote: &str, branch: Option<&str>, dest: &Path) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "clone_repo:{}:{}:{}",
                remote,
                branch.unwrap_or("default"),
                dest.display()
            ));
            Self::pop_or_default(&self.clone_results)
        }

        fn update_clone(&self, clone: &Path, branch: Option<&str>) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "update_clone:{}:{}",
                clone.display(),
                branch.unwrap_or("default")
            ));
            Self::pop_or_default(&self.update_clone_results)
        }

        fn commit(&self, clone: &Path, message: &str) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("commit:{}:{}", clone.display(), message));
            Self::pop_or_default(&self.commit_results)
        }

        fn rebase_onto_remote(&self, clone: &Path, branch: Option<&str>) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "rebase_onto_remote:{}:{}",
                clone.display(),
                branch.unwrap_or("default")
            ));
            Self::pop_or_default(&self.rebase_onto_remote_results)
        }

        fn push(&self, clone: &Path, branch: Option<&str>) -> Result<usize> {
            self.calls.borrow_mut().push(format!(
                "push:{}:{}",
                clone.display(),
                branch.unwrap_or("default")
            ));
            Self::pop_or_default(&self.push_commits_results)
        }

        fn unpushed(&self, clone: &Path, branch: Option<&str>) -> Result<usize> {
            self.calls.borrow_mut().push(format!(
                "unpushed:{}:{}",
                clone.display(),
                branch.unwrap_or("default")
            ));
            Self::pop_or_default(&self.unpushed_results)
        }

        fn added_files(&self, clone: &Path, branch: Option<&str>) -> Result<Vec<String>> {
            self.calls.borrow_mut().push(format!(
                "added_files:{}:{}",
                clone.display(),
                branch.unwrap_or("default")
            ));
            Self::pop_or_default(&self.added_files_results)
        }

        fn has_uncommitted_changes(&self, clone: &Path) -> Result<bool> {
            self.calls
                .borrow_mut()
                .push(format!("has_uncommitted_changes:{}", clone.display()));
            Self::pop_or_default(&self.has_uncommitted_changes_results)
        }

        fn push_ref(&self, _root: &Path, remote: &str, refname: &str) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("push_ref:{}:{}", remote, refname));
            Self::pop_or_default(&self.push_results)
        }

        fn push_new_ref(
            &self,
            _root: &Path,
            remote: &str,
            refname: &str,
            new_sha: &str,
        ) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "push_new_ref:{}:{}:new_sha={}",
                remote, refname, new_sha
            ));
            Self::pop_or_default(&self.push_new_ref_results)
        }

        fn delete_remote_ref(
            &self,
            _root: &Path,
            remote: &str,
            refname: &str,
            expected_old: Option<&str>,
        ) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "delete_remote_ref:{}:{}:expected_old={:?}",
                remote, refname, expected_old
            ));
            Self::pop_or_default(&self.delete_remote_results)
        }

        fn push_ref_with_lease(
            &self,
            _root: &Path,
            remote: &str,
            refname: &str,
            new_sha: &str,
            expected_old: Option<&str>,
        ) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "push_ref_with_lease:{}:{}:new_sha={}:expected_old={:?}",
                remote, refname, new_sha, expected_old
            ));
            Self::pop_or_default(&self.push_with_lease_results)
        }

        fn read_commit_timestamp(&self, _root: &Path, sha: &str) -> Result<DateTime<Utc>> {
            self.calls
                .borrow_mut()
                .push(format!("read_commit_timestamp:{}", sha));
            let mut q = self.read_commit_timestamp_results.borrow_mut();
            if q.is_empty() {
                bail!("no read_commit_timestamp result configured")
            } else {
                q.remove(0)
            }
        }

        fn head(&self, root: &Path) -> Result<String> {
            // The root is logged, not ignored: which repository HEAD was read
            // from is the whole question in a docs-repo split, where the docs
            // root and `[governs] root` are different repositories.
            self.calls
                .borrow_mut()
                .push(format!("head:{}", root.display()));
            let mut q = self.head_results.borrow_mut();
            if q.is_empty() {
                Ok(FAKE_HEAD.to_string())
            } else {
                q.remove(0)
            }
        }

        fn renames(&self, _root: &Path, from: &str, to: &str) -> Result<Vec<(String, String)>> {
            self.calls
                .borrow_mut()
                .push(format!("renames:{}..{}", from, to));
            if let Some(message) = self.renames_error.borrow().as_deref() {
                bail!("{}", message);
            }
            Ok(self.renames_pairs.borrow().clone())
        }

        fn diff_stat(&self, root: &Path, from: &str, to: &str, paths: &[String]) -> Result<Drift> {
            // Root, range and pathspecs all logged: which repository was
            // diffed, in which direction, over which globs is the whole of
            // what a caller gets wrong, and the counts replayed below would
            // look identical for every wrong answer.
            self.calls.borrow_mut().push(format!(
                "diff_stat:{}:{}..{}:{}",
                root.display(),
                from,
                to,
                paths.join(",")
            ));
            if let Some(message) = self.diff_stat_error.borrow().as_deref() {
                bail!("{}", message);
            }
            Ok(*self.diff_stat_drift.borrow())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockGitRefClient;
    use super::*;
    use std::path::PathBuf;

    fn dummy_root() -> PathBuf {
        PathBuf::from("/tmp/fake")
    }

    #[test]
    fn mock_resolve_ref_returns_configured_result() {
        let mock = MockGitRefClient::new().with_resolve_result(Ok(Some("abc123".to_string())));
        let result = mock.resolve_ref(&dummy_root(), "refs/test").unwrap();
        assert_eq!(result, Some("abc123".to_string()));
        assert_eq!(mock.call_log().borrow()[0], "resolve_ref:refs/test");
    }

    #[test]
    fn mock_resolve_ref_returns_none_by_default() {
        let mock = MockGitRefClient::new();
        let result = mock.resolve_ref(&dummy_root(), "refs/test").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn mock_list_refs_returns_configured_result() {
        let refs = vec![
            ("refs/a".to_string(), "sha1".to_string()),
            ("refs/b".to_string(), "sha2".to_string()),
        ];
        let mock = MockGitRefClient::new().with_list_result(Ok(refs.clone()));
        let result = mock.list_refs(&dummy_root(), "refs/*").unwrap();
        assert_eq!(result, refs);
    }

    #[test]
    fn mock_read_blob_returns_configured_result() {
        let mock = MockGitRefClient::new().with_read_blob_result(Ok("file content".to_string()));
        let result = mock
            .read_ref_blob(&dummy_root(), "abc", "file.txt")
            .unwrap();
        assert_eq!(result, "file content");
    }

    #[test]
    fn mock_create_commit_returns_configured_sha() {
        let mock = MockGitRefClient::new().with_create_commit_result(Ok("danglingsha".to_string()));
        let result = mock
            .create_commit(&dummy_root(), "refs/test", &[("f.txt", "data")], None)
            .unwrap();
        assert_eq!(result, "danglingsha");
        assert_eq!(
            mock.call_log().borrow()[0],
            "create_commit:refs/test:parent=None"
        );
    }

    #[test]
    fn mock_create_ref_commit_returns_configured_sha() {
        let mock = MockGitRefClient::new().with_create_ref_commit_result(Ok("newsha".to_string()));
        let result = mock
            .create_ref_commit(&dummy_root(), "refs/test", &[("f.txt", "data")])
            .unwrap();
        assert_eq!(result, "newsha");
    }

    #[test]
    fn mock_update_ref_returns_configured_result() {
        let mock = MockGitRefClient::new().with_update_ref_result(Ok(()));
        mock.update_ref(&dummy_root(), "refs/test", "new", "old")
            .unwrap();
        assert_eq!(mock.call_log().borrow()[0], "update_ref:refs/test:new:old");
    }

    #[test]
    fn mock_records_all_calls_in_order() {
        let mock = MockGitRefClient::new()
            .with_resolve_result(Ok(Some("sha".to_string())))
            .with_delete_ref_result(Ok(()));

        mock.resolve_ref(&dummy_root(), "refs/a").unwrap();
        mock.delete_ref(&dummy_root(), "refs/b").unwrap();
        mock.push_ref(&dummy_root(), "origin", "refs/c").unwrap();

        let log = mock.call_log();
        let calls = log.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0], "resolve_ref:refs/a");
        assert_eq!(calls[1], "delete_ref:refs/b");
        assert_eq!(calls[2], "push_ref:origin:refs/c");
    }

    #[test]
    fn mock_consumes_results_in_order() {
        let mock = MockGitRefClient::new()
            .with_resolve_result(Ok(Some("first".to_string())))
            .with_resolve_result(Ok(Some("second".to_string())));

        let r1 = mock.resolve_ref(&dummy_root(), "refs/a").unwrap();
        let r2 = mock.resolve_ref(&dummy_root(), "refs/b").unwrap();
        let r3 = mock.resolve_ref(&dummy_root(), "refs/c").unwrap();

        assert_eq!(r1, Some("first".to_string()));
        assert_eq!(r2, Some("second".to_string()));
        assert_eq!(r3, None); // default when queue exhausted
    }

    #[test]
    fn create_commit_with_parent_records_parent_arg() {
        let mock = MockGitRefClient::new().with_create_commit_result(Ok("newsha".to_string()));
        mock.create_commit(
            &dummy_root(),
            "refs/test",
            &[("f.txt", "data")],
            Some("abc123"),
        )
        .unwrap();
        assert_eq!(
            mock.call_log().borrow()[0],
            "create_commit:refs/test:parent=Some(\"abc123\")"
        );
    }

    #[test]
    fn mock_push_ref_with_lease_records_call_with_expected_old() {
        let mock = MockGitRefClient::new().with_push_with_lease_result(Ok(()));
        mock.push_ref_with_lease(
            &dummy_root(),
            "origin",
            "refs/test",
            "newsha",
            Some("abc123"),
        )
        .unwrap();
        assert_eq!(
            mock.call_log().borrow()[0],
            "push_ref_with_lease:origin:refs/test:new_sha=newsha:expected_old=Some(\"abc123\")"
        );
    }

    #[test]
    fn mock_push_ref_with_lease_records_call_with_none() {
        let mock = MockGitRefClient::new().with_push_with_lease_result(Ok(()));
        mock.push_ref_with_lease(&dummy_root(), "origin", "refs/test", "newsha", None)
            .unwrap();
        assert_eq!(
            mock.call_log().borrow()[0],
            "push_ref_with_lease:origin:refs/test:new_sha=newsha:expected_old=None"
        );
    }

    #[test]
    fn mock_push_ref_with_lease_returns_configured_error() {
        let mock = MockGitRefClient::new()
            .with_push_with_lease_result(Err(anyhow::anyhow!("lease mismatch")));
        let result =
            mock.push_ref_with_lease(&dummy_root(), "origin", "refs/test", "newsha", Some("old"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("lease mismatch"));
    }

    #[test]
    fn create_commit_without_parent_creates_orphan() {
        let mock = MockGitRefClient::new().with_create_commit_result(Ok("newsha".to_string()));
        mock.create_commit(&dummy_root(), "refs/test", &[("f.txt", "data")], None)
            .unwrap();
        assert_eq!(
            mock.call_log().borrow()[0],
            "create_commit:refs/test:parent=None"
        );
    }

    #[test]
    fn read_commit_timestamp_mock_records_call() {
        use chrono::Utc;
        let ts = Utc::now();
        let mock = MockGitRefClient::new().with_read_commit_timestamp_result(Ok(ts));
        let result = mock.read_commit_timestamp(&dummy_root(), "abc123").unwrap();
        assert_eq!(result, ts);
        assert_eq!(mock.call_log().borrow()[0], "read_commit_timestamp:abc123");
    }

    #[test]
    fn mock_renames_replays_its_pairs_and_records_the_range() {
        let mock = MockGitRefClient::new().with_renames(&[("src/a/mod.rs", "src/b/mod.rs")]);

        let first = mock.renames(&dummy_root(), "abc123", "HEAD").unwrap();
        let second = mock.renames(&dummy_root(), "abc123", "HEAD").unwrap();

        assert_eq!(
            first,
            vec![("src/a/mod.rs".to_string(), "src/b/mod.rs".to_string())]
        );
        assert_eq!(second, first, "every call sees the same fixture");
        assert_eq!(mock.call_log().borrow()[0], "renames:abc123..HEAD");
    }

    #[test]
    fn parse_renames_keeps_rename_rows_and_drops_the_rest() {
        let stdout = "R100\tsrc/a/mod.rs\tsrc/b/mod.rs\nM\tsrc/c.rs\nR075\tsrc/a/x.rs\tsrc/b/x.rs\nD\tsrc/d.rs\n";

        assert_eq!(
            parse_renames(stdout),
            vec![
                ("src/a/mod.rs".to_string(), "src/b/mod.rs".to_string()),
                ("src/a/x.rs".to_string(), "src/b/x.rs".to_string()),
            ]
        );
    }

    #[test]
    fn mock_diff_stat_replays_its_counts_and_records_the_range_and_paths() {
        let drift = Drift {
            files: 12,
            insertions: 310,
            deletions: 85,
        };
        let mock = MockGitRefClient::new().with_diff_stat(drift);
        let paths = vec!["src/engine/**".to_string(), "src/cli.rs".to_string()];

        let first = mock
            .diff_stat(&dummy_root(), "abc123", "HEAD", &paths)
            .unwrap();
        let second = mock
            .diff_stat(&dummy_root(), "abc123", "HEAD", &paths)
            .unwrap();

        assert_eq!(first, drift);
        assert_eq!(second, first, "every call sees the same fixture");
        assert_eq!(
            mock.call_log().borrow()[0],
            format!(
                "diff_stat:{}:abc123..HEAD:src/engine/**,src/cli.rs",
                dummy_root().display()
            )
        );
    }

    #[test]
    fn mock_diff_stat_returns_its_configured_error() {
        let mock = MockGitRefClient::new().with_diff_stat_error("bad revision");
        let result = mock.diff_stat(&dummy_root(), "abc123", "HEAD", &[]);
        assert!(result.unwrap_err().to_string().contains("bad revision"));
    }

    const NUMSTAT: &str = "10\t2\tsrc/top.rs\n300\t83\tsrc/deep/nested.rs\n-\t-\tdocs/logo.png\n";

    #[test]
    fn sum_numstat_totals_every_row_when_nothing_is_governed() {
        assert_eq!(
            sum_numstat(NUMSTAT, None),
            Drift {
                files: 3,
                insertions: 310,
                deletions: 85
            }
        );
    }

    #[test]
    fn sum_numstat_counts_a_binary_row_as_a_file_and_no_lines() {
        assert_eq!(
            sum_numstat("-\t-\tdocs/logo.png\n", None),
            Drift {
                files: 1,
                insertions: 0,
                deletions: 0
            }
        );
    }

    #[test]
    fn sum_numstat_skips_the_rows_the_globs_do_not_match() {
        let governed = governed_matcher(&["src/deep/**".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(
            sum_numstat(NUMSTAT, Some(&governed)),
            Drift {
                files: 1,
                insertions: 300,
                deletions: 83
            }
        );
    }

    #[test]
    fn sum_numstat_of_an_unchanged_range_is_zero() {
        assert_eq!(sum_numstat("", None), Drift::default());
    }

    /// An uncompilable glob errors rather than silently governing nothing, which
    /// would read as a document nothing has touched.
    #[test]
    fn governed_matcher_rejects_a_glob_that_does_not_compile() {
        let error = governed_matcher(&["src/[".to_string()]).unwrap_err();
        assert!(error.to_string().contains("invalid governs glob 'src/['"));
    }

    #[test]
    fn mock_head_returns_a_fixed_sha_then_the_queued_result() {
        use super::test_support::FAKE_HEAD;
        let mock = MockGitRefClient::new();
        assert_eq!(mock.head(&dummy_root()).unwrap(), FAKE_HEAD);

        let mock = MockGitRefClient::new().with_head_result(Err(anyhow::anyhow!("no HEAD")));
        assert!(mock.head(&dummy_root()).is_err());
    }
}
