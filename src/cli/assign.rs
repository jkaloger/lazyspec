use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::Config;
use crate::engine::store::Store;
use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct AssignOutput {
    id: String,
    assignee_added: String,
    assignees: Vec<String>,
}

/// Resolve the effective user to assign: explicit `--user` wins, else first of
/// `config.orchestration.agent_users`. Errors if neither is available.
pub(crate) fn resolve_user(user: Option<&str>, config: &Config) -> Result<String> {
    if let Some(u) = user {
        return Ok(u.to_string());
    }
    let default = config
        .orchestration
        .as_ref()
        .and_then(|o| o.agent_users.first().cloned());
    match default {
        Some(u) => Ok(u),
        None => bail!(
            "no user specified and no default in [orchestration] agent_users; \
             pass --user <login> or configure [orchestration] agent_users in lazyspec.toml"
        ),
    }
}

pub fn run(root: &Path, doc_id: &str, user: Option<&str>, json: bool) -> Result<()> {
    let fs = crate::engine::fs::RealFileSystem;
    let config = Config::load(root, &fs)?;
    let resolved_user = resolve_user(user, &config)?;

    let store = Store::load(root, &config)?;
    let doc = resolve_shorthand_or_path(&store, doc_id)?;
    let id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();

    // Append unconditionally (mirrors set_provenance append semantics). Idempotency
    // is left to a higher layer for v1.
    let mut new_list = doc.assignees.clone();
    new_list.push(resolved_user.clone());

    crate::engine::assignees::set_assignees(root, &config, &type_name, &id, &new_list)?;

    // Best-effort kick to daemon. T8 will replace this stub with a real send.
    send_kick(root);

    // Reload to reflect persisted state.
    let store = Store::load(root, &config)?;
    let reloaded = resolve_shorthand_or_path(&store, &id)?;
    let assignees = reloaded.assignees.clone();

    if json {
        let output = AssignOutput {
            id: id.clone(),
            assignee_added: resolved_user.clone(),
            assignees,
        };
        println!("{}", serde_json::to_string(&output)?);
    } else {
        println!(
            "Assigned {} to {} (assignees: {:?})",
            resolved_user, id, assignees
        );
    }
    Ok(())
}

pub const DAEMON_SOCKET: &str = ".lazyspec/daemon.sock";

fn send_kick(root: &Path) {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    let path = root.join(DAEMON_SOCKET);
    if let Ok(mut stream) = UnixStream::connect(&path) {
        let _ = stream.write_all(b"kick\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, OrchestrationConfig};

    fn config_with_agents(agents: Vec<&str>) -> Config {
        Config {
            orchestration: Some(OrchestrationConfig {
                agent_users: agents.into_iter().map(String::from).collect(),
                claim_type: "story".to_string(),
                branch_template: "agents/{{ story_id }}".to_string(),
                workspace_root: std::path::PathBuf::from(".lazyspec/work"),
                base_branch: "origin/main".to_string(),
                runtime: Default::default(),
                hooks: Default::default(),
                poll_interval_ms: 30_000,
                max_concurrent_agents: 4,
                active_statuses: vec!["todo".to_string(), "in-progress".to_string()],
                heartbeat_interval_ms: 300_000,
                metadata_push_interval_ms: 30_000,
                stall_timeout_ms: 300_000,
                max_turns: 20,
                max_failure_attempts: 5,
                max_retry_backoff_ms: 300_000,
                handoff_states: vec!["in-review".to_string()],
                continuation_delay_ms: 1_000,
            }),
            ..Config::default()
        }
    }

    #[test]
    fn resolves_default_user_from_agent_users() {
        let config = config_with_agents(vec!["claude-bot"]);
        let resolved = resolve_user(None, &config).unwrap();
        assert_eq!(resolved, "claude-bot");
    }

    #[test]
    fn errors_when_no_user_and_no_agent_users() {
        let config = config_with_agents(vec![]);
        let err = resolve_user(None, &config).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("agent_users") || msg.contains("--user"),
            "error should mention agent_users or --user, got: {}",
            msg
        );
    }

    #[test]
    fn prefers_explicit_user_over_default() {
        let config = config_with_agents(vec!["claude-bot"]);
        let resolved = resolve_user(Some("alice"), &config).unwrap();
        assert_eq!(resolved, "alice");
    }

    #[test]
    fn errors_when_no_user_and_no_orchestration_section() {
        let config = Config::default();
        let err = resolve_user(None, &config).unwrap_err();
        assert!(err.to_string().contains("agent_users"));
    }

    #[test]
    fn send_kick_writes_to_listening_socket() {
        use std::io::Read;
        use std::os::unix::net::UnixListener;
        use std::sync::mpsc;
        use std::time::Duration;

        let td = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        let sock_path = td.path().join(DAEMON_SOCKET);
        let listener = UnixListener::bind(&sock_path).unwrap();

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let handle = std::thread::spawn(move || {
            let (mut stream, _addr) = listener.accept().unwrap();
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).unwrap();
            tx.send(buf).unwrap();
        });

        send_kick(td.path());

        let bytes = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(bytes, b"kick\n");
        handle.join().unwrap();
    }

    #[test]
    fn send_kick_no_op_when_socket_absent() {
        let td = tempfile::TempDir::new().unwrap();
        // No .lazyspec dir, no socket. Should complete without panicking.
        send_kick(td.path());
    }

    #[test]
    fn send_kick_no_op_when_socket_path_is_not_a_socket() {
        let td = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(td.path().join(".lazyspec")).unwrap();
        std::fs::write(td.path().join(DAEMON_SOCKET), b"not a socket").unwrap();
        // UnixStream::connect should error; send_kick must swallow.
        send_kick(td.path());
    }
}
