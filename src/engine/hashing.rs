use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Pipes the given bytes to `git hash-object --stdin` and returns the 40-char hex SHA.
/// This is the single integration point for all content hashing.
pub fn hash_bytes(bytes: &[u8]) -> Result<String> {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn git hash-object --stdin")?;

    child
        .stdin
        .as_mut()
        .expect("stdin was piped")
        .write_all(bytes)
        .context("failed to write bytes to git hash-object stdin")?;

    let output = child
        .wait_with_output()
        .context("failed to wait on git hash-object")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git hash-object --stdin failed: {stderr}");
    }

    let sha = String::from_utf8(output.stdout)
        .context("git hash-object produced non-UTF-8 output")?
        .trim()
        .to_string();

    Ok(sha)
}

/// Runs `git hash-object <path>` on the raw file content (no normalization).
/// Used for whole-file refs.
pub fn hash_file(path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["hash-object"])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run git hash-object on file")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git hash-object {:?} failed: {stderr}", path);
    }

    let sha = String::from_utf8(output.stdout)
        .context("git hash-object produced non-UTF-8 output")?
        .trim()
        .to_string();

    Ok(sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_bytes_known_content() {
        // echo -n "hello" | git hash-object --stdin => b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0
        let sha = hash_bytes(b"hello").unwrap();
        assert_eq!(sha, "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0");
    }

    #[test]
    fn test_hash_bytes_empty() {
        let sha = hash_bytes(b"").unwrap();
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
        // Known empty blob hash
        assert_eq!(sha, "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn test_hash_file_matches_git() {
        let content = b"test file content for hashing";
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(content).unwrap();
        tmp.flush().unwrap();

        let file_sha = hash_file(tmp.path()).unwrap();
        let bytes_sha = hash_bytes(content).unwrap();

        assert_eq!(file_sha, bytes_sha);
    }
}
