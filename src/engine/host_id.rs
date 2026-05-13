use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use uuid::Uuid;

const HOST_ID_FILE: &str = ".lazyspec/daemon-host-id";

/// Returns a stable per-workspace host id formatted as `"{hostname}-{uuid}"`.
///
/// The UUID component is persisted in `<root>/.lazyspec/daemon-host-id` and is
/// generated (UUID v4) on first call. Subsequent calls reuse the persisted UUID.
/// The hostname component is resolved at call time via the `hostname` system
/// command. Together these give the daemon a stable identity for tagging
/// leases (`lease.agent = "{host_id}:{session_id}"`) so it can release only
/// leases owned by this machine on graceful shutdown.
pub fn host_id(root: &Path) -> Result<String> {
    let path = root.join(HOST_ID_FILE);
    let uuid = if path.exists() {
        fs::read_to_string(&path)
            .with_context(|| format!("failed to read host id file: {}", path.display()))?
            .trim()
            .to_string()
    } else {
        let new_uuid = Uuid::new_v4().to_string();
        write_atomically(&path, &new_uuid)?;
        new_uuid
    };
    Ok(format!("{}-{}", hostname()?, uuid))
}

fn hostname() -> Result<String> {
    let output = Command::new("hostname")
        .output()
        .context("failed to run `hostname` command")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`hostname` command failed: {stderr}");
    }
    let name = String::from_utf8(output.stdout)
        .context("`hostname` produced non-UTF-8 output")?
        .trim()
        .to_string();
    if name.is_empty() {
        anyhow::bail!("`hostname` returned empty output");
    }
    Ok(name)
}

fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("host id path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    let tmp = parent.join(format!(".daemon-host-id.tmp.{}", std::process::id()));
    fs::write(&tmp, content)
        .with_context(|| format!("failed to write temp host id file: {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn host_id_is_idempotent_within_workspace() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".lazyspec")).unwrap();

        let first = host_id(tmp.path()).unwrap();
        let second = host_id(tmp.path()).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn host_id_persists_id_file() {
        let tmp = TempDir::new().unwrap();

        let _ = host_id(tmp.path()).unwrap();

        let file = tmp.path().join(".lazyspec/daemon-host-id");
        assert!(file.exists(), "expected daemon-host-id file to exist");

        let contents = fs::read_to_string(&file).unwrap();
        let trimmed = contents.trim();
        Uuid::parse_str(trimmed).expect("file contents should be a valid UUID");
    }

    #[test]
    fn host_id_reuses_existing_id_file() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".lazyspec")).unwrap();

        let preset = "11111111-2222-4333-8444-555555555555";
        fs::write(tmp.path().join(".lazyspec/daemon-host-id"), preset).unwrap();

        let result = host_id(tmp.path()).unwrap();

        assert!(
            result.ends_with(&format!("-{}", preset)),
            "expected suffix -{}, got {}",
            preset,
            result
        );
        let hostname_part = result.strip_suffix(&format!("-{}", preset)).unwrap();
        assert!(!hostname_part.is_empty(), "hostname segment was empty");
    }

    #[test]
    fn host_id_creates_lazyspec_dir_on_demand() {
        let tmp = TempDir::new().unwrap();
        assert!(!tmp.path().join(".lazyspec").exists());

        let result = host_id(tmp.path()).unwrap();

        assert!(
            tmp.path().join(".lazyspec").is_dir(),
            ".lazyspec dir should have been created"
        );
        assert!(tmp.path().join(".lazyspec/daemon-host-id").exists());
        assert!(result.contains('-'));
    }
}
