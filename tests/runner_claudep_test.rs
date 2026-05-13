use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::RecvTimeoutError;
use tempfile::TempDir;

use lazyspec::engine::runner::{AgentContext, AgentEvent, AgentRunner, ClaudeP, ToolStatus};

const RECV_TIMEOUT: Duration = Duration::from_secs(2);

fn fake_binary(dir: &TempDir, body: &str) -> PathBuf {
    let script = dir.path().join("fake-claude.sh");
    let contents = format!("#!/bin/sh\n{}\n", body);
    std::fs::write(&script, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
    }
    script
}

fn ctx() -> AgentContext {
    AgentContext {
        workspace: PathBuf::from("/tmp"),
        doc_id: "STORY-127".into(),
        agent_id: "claude-bot".into(),
        branch: "feat/x".into(),
    }
}

fn runner(binary: PathBuf) -> ClaudeP {
    ClaudeP {
        binary: binary.to_string_lossy().into_owned(),
        allowed_tools: "Read,Edit".into(),
        turn_timeout_ms: 30_000,
    }
}

fn drain(events: &crossbeam_channel::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    loop {
        match events.recv_timeout(RECV_TIMEOUT) {
            Ok(ev) => out.push(ev),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("timed out draining events"),
        }
    }
    out
}

#[test]
fn spawn_returns_handle_with_pid_and_channels() {
    let dir = TempDir::new().unwrap();
    let bin = fake_binary(&dir, "exit 0");
    let handle = runner(bin).spawn(ctx()).unwrap();
    assert!(handle.pid > 0, "pid should be set");

    let events = drain(&handle.events);
    assert!(
        matches!(events.last(), Some(AgentEvent::SubprocessExited { .. })),
        "final event must be SubprocessExited, got {:?}",
        events
    );
}

#[test]
fn session_start_event_flows_through_channel() {
    let dir = TempDir::new().unwrap();
    let bin = fake_binary(
        &dir,
        r#"printf '%s\n' '{"type":"system","subtype":"init","session_id":"abc"}'"#,
    );
    let handle = runner(bin).spawn(ctx()).unwrap();

    let first = handle.events.recv_timeout(RECV_TIMEOUT).unwrap();
    assert_eq!(first, AgentEvent::SessionStarted);
}

#[test]
fn assistant_text_chunks_flow_in_order() {
    let dir = TempDir::new().unwrap();
    let body = r#"printf '%s\n' \
'{"type":"assistant","message":{"content":[{"type":"text","text":"one"}]}}' \
'{"type":"assistant","message":{"content":[{"type":"text","text":"two"}]}}' \
'{"type":"assistant","message":{"content":[{"type":"text","text":"three"}]}}'"#;
    let bin = fake_binary(&dir, body);
    let handle = runner(bin).spawn(ctx()).unwrap();

    let events = drain(&handle.events);
    let texts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Text { delta } => Some(delta.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["one", "two", "three"]);
}

#[test]
fn tool_result_flows_as_tool_call() {
    let dir = TempDir::new().unwrap();
    let body = r#"printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok","is_error":false}]}}'"#;
    let bin = fake_binary(&dir, body);
    let handle = runner(bin).spawn(ctx()).unwrap();

    let events = drain(&handle.events);
    let found = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::ToolCall { status: ToolStatus::Ok, name, .. } if name == "toolu_1"
        )
    });
    assert!(found, "expected ToolCall Ok, got {:?}", events);
}

#[test]
fn turn_complete_flows_with_usage() {
    let dir = TempDir::new().unwrap();
    let body = r#"printf '%s\n' '{"type":"result","subtype":"success","usage":{"input_tokens":10,"output_tokens":5}}'"#;
    let bin = fake_binary(&dir, body);
    let handle = runner(bin).spawn(ctx()).unwrap();

    let events = drain(&handle.events);
    let found = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::TurnCompleted {
                input_tokens: 10,
                output_tokens: 5
            }
        )
    });
    assert!(found, "expected TurnCompleted, got {:?}", events);
}

#[test]
fn subprocess_exit_emits_event_and_closes_channel() {
    let dir = TempDir::new().unwrap();
    let body = r#"printf '%s\n' '{"type":"system","subtype":"init"}'
exit 42"#;
    let bin = fake_binary(&dir, body);
    let handle = runner(bin).spawn(ctx()).unwrap();

    let mut events = Vec::new();
    let mut last_was_exit = false;
    loop {
        match handle.events.recv_timeout(RECV_TIMEOUT) {
            Ok(ev) => {
                last_was_exit = matches!(ev, AgentEvent::SubprocessExited { .. });
                events.push(ev);
                if last_was_exit {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("timed out, events so far: {:?}", events),
        }
    }
    assert!(last_was_exit, "expected SubprocessExited, got {:?}", events);
    assert_eq!(
        events.last(),
        Some(&AgentEvent::SubprocessExited { code: Some(42) }),
    );
    drop(handle.cancel);
    assert_eq!(
        handle.events.recv_timeout(RECV_TIMEOUT),
        Err(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn cancel_terminates_subprocess() {
    let dir = TempDir::new().unwrap();
    let bin = fake_binary(&dir, "exec sleep 30");
    let handle = runner(bin).spawn(ctx()).unwrap();

    handle.cancel.send(()).unwrap();

    let mut got_exit = false;
    loop {
        match handle.events.recv_timeout(RECV_TIMEOUT) {
            Ok(AgentEvent::SubprocessExited { .. }) => {
                got_exit = true;
                break;
            }
            Ok(_) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("cancel did not propagate within timeout"),
        }
    }
    assert!(got_exit, "expected SubprocessExited after cancel");
}
