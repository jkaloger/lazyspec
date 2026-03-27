use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use crate::engine::config::Config;

/// Resolves the GitHub repo from config or by inferring from git remote.
pub fn resolve_repo(config: &Config, root: &Path) -> Result<String> {
    if let Some(ref gh) = config.documents.github {
        if let Some(ref repo) = gh.repo {
            return Ok(repo.clone());
        }
    }
    infer_github_repo(root)
}

/// Parses `owner/repo` from a git remote URL.
///
/// Supports SSH (`git@github.com:owner/repo.git`) and
/// HTTPS (`https://github.com/owner/repo.git` or without `.git`).
fn parse_owner_repo(url: &str) -> Result<String> {
    let path = if let Some(rest) = url.strip_prefix("git@") {
        // git@github.com:owner/repo.git
        rest.split_once(':')
            .map(|(_, path)| path)
            .unwrap_or(rest)
    } else if url.starts_with("https://") || url.starts_with("http://") {
        // https://github.com/owner/repo.git
        url.split("//")
            .nth(1)
            .and_then(|s| s.splitn(2, '/').nth(1))
            .unwrap_or("")
    } else {
        bail!("unrecognised git remote URL format: {}", url);
    };

    let path = path.trim_end_matches(".git").trim_matches('/');

    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("could not extract owner/repo from remote URL: {}", url);
    }

    Ok(format!("{}/{}", parts[0], parts[1]))
}

/// Infers `owner/repo` by running `git remote get-url origin` in the given
/// project root directory. Falls back to URL parsing of the remote.
pub fn infer_github_repo(project_root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git remote get-url origin failed: {}",
            stderr.trim()
        );
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_owner_repo(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_url() {
        let result = parse_owner_repo("git@github.com:owner/repo.git").unwrap();
        assert_eq!(result, "owner/repo");
    }

    #[test]
    fn https_url_with_dot_git() {
        let result = parse_owner_repo("https://github.com/owner/repo.git").unwrap();
        assert_eq!(result, "owner/repo");
    }

    #[test]
    fn https_url_without_dot_git() {
        let result = parse_owner_repo("https://github.com/owner/repo").unwrap();
        assert_eq!(result, "owner/repo");
    }

    #[test]
    fn ssh_url_without_dot_git() {
        let result = parse_owner_repo("git@github.com:owner/repo").unwrap();
        assert_eq!(result, "owner/repo");
    }

    #[test]
    fn https_url_with_trailing_slash() {
        let result = parse_owner_repo("https://github.com/owner/repo/").unwrap();
        assert_eq!(result, "owner/repo");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_owner_repo("not-a-url").is_err());
    }

    #[test]
    fn rejects_url_missing_repo() {
        assert!(parse_owner_repo("https://github.com/owner").is_err());
    }

    #[test]
    fn ignores_extra_path_segments() {
        let result = parse_owner_repo("https://github.com/owner/repo/tree/main").unwrap();
        assert_eq!(result, "owner/repo");
    }
}
