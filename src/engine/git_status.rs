use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileStatus {
    New,
    Modified,
}

pub struct GitStatusCache {
    statuses: Option<HashMap<PathBuf, GitFileStatus>>,
    stale: bool,
    repo_root: PathBuf,
}

pub fn parse_porcelain_line(line: &str) -> Option<(PathBuf, GitFileStatus)> {
    if line.len() < 4 {
        return None;
    }

    let x = line.as_bytes()[0];
    let y = line.as_bytes()[1];
    let raw_path = &line[3..];

    let (status, path) = match (x, y) {
        (b'?', b'?') | (b'A', b' ') | (b'A', b'M') => {
            (GitFileStatus::New, raw_path.to_string())
        }
        (b'R', b' ') | (b'R', b'M') => {
            let dest = raw_path
                .rsplit_once(" -> ")
                .map(|(_, d)| d.to_string())
                .unwrap_or_else(|| raw_path.to_string());
            (GitFileStatus::Modified, dest)
        }
        _ => (GitFileStatus::Modified, raw_path.to_string()),
    };

    Some((PathBuf::from(path), status))
}

pub fn query_git_status(repo_root: &Path) -> Option<HashMap<PathBuf, GitFileStatus>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();

    for line in stdout.lines() {
        if let Some((path, status)) = parse_porcelain_line(line) {
            map.insert(path, status);
        }
    }

    Some(map)
}

impl GitStatusCache {
    pub fn new(repo_root: &Path) -> Self {
        let statuses = query_git_status(repo_root);
        Self {
            statuses,
            stale: false,
            repo_root: repo_root.to_path_buf(),
        }
    }

    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    pub fn refresh(&mut self) {
        if !self.stale {
            return;
        }
        self.statuses = query_git_status(&self.repo_root);
        self.stale = false;
    }

    pub fn get(&self, path: &Path) -> Option<&GitFileStatus> {
        self.statuses.as_ref()?.get(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn in_git_repo() -> bool {
        Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_query_git_status_returns_some_in_repo() {
        if !in_git_repo() {
            return;
        }
        let root = std::env::current_dir().unwrap();
        let result = query_git_status(&root);
        assert!(result.is_some());
    }

    #[test]
    fn test_cache_new_and_get() {
        if !in_git_repo() {
            return;
        }
        let root = std::env::current_dir().unwrap();
        let cache = GitStatusCache::new(&root);
        // Just verify it doesn't panic and returns a valid cache
        let _ = cache.get(&root.join("nonexistent-file.txt"));
    }

    #[test]
    fn test_cache_invalidate_and_refresh() {
        if !in_git_repo() {
            return;
        }
        let root = std::env::current_dir().unwrap();
        let mut cache = GitStatusCache::new(&root);
        assert!(!cache.stale);

        cache.invalidate();
        assert!(cache.stale);

        cache.refresh();
        assert!(!cache.stale);
    }

    #[test]
    fn test_cache_refresh_noop_when_not_stale() {
        if !in_git_repo() {
            return;
        }
        let root = std::env::current_dir().unwrap();
        let mut cache = GitStatusCache::new(&root);
        // Should be a no-op, no panic
        cache.refresh();
        assert!(!cache.stale);
    }

    #[test]
    fn test_parse_new_files() {
        // Simulate parsing by checking untracked files show as New
        if !in_git_repo() {
            return;
        }
        let root = std::env::current_dir().unwrap();
        let tmp_file = root.join("_test_git_status_untracked.tmp");
        fs::write(&tmp_file, "test").unwrap();

        let result = query_git_status(&root);
        let _ = fs::remove_file(&tmp_file);

        let map = result.unwrap();
        let relative = PathBuf::from("_test_git_status_untracked.tmp");
        let status = map.get(&relative).unwrap();
        assert_eq!(*status, GitFileStatus::New);
    }

    #[test]
    fn test_non_git_directory() {
        let tmp = std::env::temp_dir().join("lazyspec_git_status_test");
        let _ = fs::create_dir_all(&tmp);

        let result = query_git_status(&tmp);
        assert!(result.is_none());

        let _ = fs::remove_dir(&tmp);
    }

    #[test]
    fn test_parse_porcelain_untracked() {
        let (path, status) = parse_porcelain_line("?? src/main.rs").unwrap();
        assert_eq!(status, GitFileStatus::New);
        assert_eq!(path, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn test_parse_porcelain_added() {
        let (path, status) = parse_porcelain_line("A  src/lib.rs").unwrap();
        assert_eq!(status, GitFileStatus::New);
        assert_eq!(path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_parse_porcelain_added_modified() {
        let (path, status) = parse_porcelain_line("AM src/lib.rs").unwrap();
        assert_eq!(status, GitFileStatus::New);
        assert_eq!(path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_parse_porcelain_modified_staged() {
        let (path, status) = parse_porcelain_line("M  src/lib.rs").unwrap();
        assert_eq!(status, GitFileStatus::Modified);
        assert_eq!(path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_parse_porcelain_modified_unstaged() {
        let (path, status) = parse_porcelain_line(" M src/lib.rs").unwrap();
        assert_eq!(status, GitFileStatus::Modified);
        assert_eq!(path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_parse_porcelain_modified_both() {
        let (path, status) = parse_porcelain_line("MM src/lib.rs").unwrap();
        assert_eq!(status, GitFileStatus::Modified);
        assert_eq!(path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_parse_porcelain_renamed() {
        let (path, status) = parse_porcelain_line("R  old.rs -> new.rs").unwrap();
        assert_eq!(status, GitFileStatus::Modified);
        assert_eq!(path, PathBuf::from("new.rs"));
    }

    #[test]
    fn test_parse_porcelain_renamed_modified() {
        let (path, status) = parse_porcelain_line("RM old.rs -> new.rs").unwrap();
        assert_eq!(status, GitFileStatus::Modified);
        assert_eq!(path, PathBuf::from("new.rs"));
    }

    #[test]
    fn test_parse_porcelain_deleted() {
        let (path, status) = parse_porcelain_line("D  src/old.rs").unwrap();
        assert_eq!(status, GitFileStatus::Modified);
        assert_eq!(path, PathBuf::from("src/old.rs"));
    }

    #[test]
    fn test_parse_porcelain_short_line_returns_none() {
        assert!(parse_porcelain_line("??").is_none());
        assert!(parse_porcelain_line("").is_none());
        assert!(parse_porcelain_line("M ").is_none());
    }
}
