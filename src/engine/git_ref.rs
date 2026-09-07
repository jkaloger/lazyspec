use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

// Wall-clock cap for the network `git fetch` in the sync path, so a slow or
// auth-prompting remote can't wedge the background poll thread (BUG-001).
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

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

#[cfg(test)]
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
        pub calls: RefCell<Vec<String>>,
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
                push_results: RefCell::new(vec![]),
                push_new_ref_results: RefCell::new(vec![]),
                delete_remote_results: RefCell::new(vec![]),
                push_with_lease_results: RefCell::new(vec![]),
                read_commit_timestamp_results: RefCell::new(vec![]),
                head_results: RefCell::new(vec![]),
                renames_pairs: RefCell::new(vec![]),
                renames_error: RefCell::new(None),
                calls: RefCell::new(vec![]),
                committed_blobs: RefCell::new(vec![]),
            }
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
        assert_eq!(mock.calls.borrow()[0], "resolve_ref:refs/test");
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
            mock.calls.borrow()[0],
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
        assert_eq!(mock.calls.borrow()[0], "update_ref:refs/test:new:old");
    }

    #[test]
    fn mock_records_all_calls_in_order() {
        let mock = MockGitRefClient::new()
            .with_resolve_result(Ok(Some("sha".to_string())))
            .with_delete_ref_result(Ok(()));

        mock.resolve_ref(&dummy_root(), "refs/a").unwrap();
        mock.delete_ref(&dummy_root(), "refs/b").unwrap();
        mock.push_ref(&dummy_root(), "origin", "refs/c").unwrap();

        let calls = mock.calls.borrow();
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
            mock.calls.borrow()[0],
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
            mock.calls.borrow()[0],
            "push_ref_with_lease:origin:refs/test:new_sha=newsha:expected_old=Some(\"abc123\")"
        );
    }

    #[test]
    fn mock_push_ref_with_lease_records_call_with_none() {
        let mock = MockGitRefClient::new().with_push_with_lease_result(Ok(()));
        mock.push_ref_with_lease(&dummy_root(), "origin", "refs/test", "newsha", None)
            .unwrap();
        assert_eq!(
            mock.calls.borrow()[0],
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
            mock.calls.borrow()[0],
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
        assert_eq!(mock.calls.borrow()[0], "read_commit_timestamp:abc123");
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
        assert_eq!(mock.calls.borrow()[0], "renames:abc123..HEAD");
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
    fn mock_head_returns_a_fixed_sha_then_the_queued_result() {
        use super::test_support::FAKE_HEAD;
        let mock = MockGitRefClient::new();
        assert_eq!(mock.head(&dummy_root()).unwrap(), FAKE_HEAD);

        let mock = MockGitRefClient::new().with_head_result(Err(anyhow::anyhow!("no HEAD")));
        assert!(mock.head(&dummy_root()).is_err());
    }
}
