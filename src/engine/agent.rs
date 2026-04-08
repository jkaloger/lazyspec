use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

pub fn resolve_agent_id(root: &Path) -> Result<String> {
    resolve_agent_id_with_env(
        root,
        std::env::var("LAZYSPEC_AGENT_ID").ok(),
        std::env::var("CLAUDE_SESSION_ID").ok(),
    )
}

pub fn resolve_agent_id_with_env(
    root: &Path,
    lazyspec_agent_id: Option<String>,
    claude_session_id: Option<String>,
) -> Result<String> {
    if let Some(id) = lazyspec_agent_id.filter(|s| !s.is_empty()) {
        return Ok(id);
    }

    if let Some(id) = claude_session_id.filter(|s| !s.is_empty()) {
        return Ok(id);
    }

    let output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(root)
        .output()?;

    if !output.status.success() {
        bail!("git config user.name failed; set $LAZYSPEC_AGENT_ID or configure git user.name");
    }

    let user_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if user_name.is_empty() {
        bail!("git config user.name is empty; set $LAZYSPEC_AGENT_ID or configure git user.name");
    }

    Ok(user_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn uses_lazyspec_agent_id_when_set() {
        let result =
            resolve_agent_id_with_env(Path::new("/tmp/fake"), Some("my-agent".into()), None)
                .unwrap();
        assert_eq!(result, "my-agent");
    }

    #[test]
    fn uses_claude_session_id_when_lazyspec_unset() {
        let result =
            resolve_agent_id_with_env(Path::new("/tmp/fake"), None, Some("sess-123".into()))
                .unwrap();
        assert_eq!(result, "sess-123");
    }

    #[test]
    fn lazyspec_agent_id_takes_priority_over_claude_session_id() {
        let result = resolve_agent_id_with_env(
            Path::new("/tmp/fake"),
            Some("agent-1".into()),
            Some("sess-123".into()),
        )
        .unwrap();
        assert_eq!(result, "agent-1");
    }

    #[test]
    fn empty_strings_treated_as_unset() {
        let result =
            resolve_agent_id_with_env(Path::new("/tmp/fake"), Some("".into()), Some("sess".into()))
                .unwrap();
        assert_eq!(result, "sess");
    }

    #[test]
    fn falls_back_to_git_config_username() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "TestUser"])
            .current_dir(root)
            .output()
            .unwrap();

        let result = resolve_agent_id_with_env(root, None, None).unwrap();
        assert_eq!(result, "TestUser");
    }

    #[test]
    fn both_empty_strings_falls_back_to_git() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "GitUser"])
            .current_dir(root)
            .output()
            .unwrap();

        let result = resolve_agent_id_with_env(root, Some("".into()), Some("".into())).unwrap();
        assert_eq!(result, "GitUser");
    }
}
