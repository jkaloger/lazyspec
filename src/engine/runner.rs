use std::path::PathBuf;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

mod claudep;
mod stream;

pub use claudep::ClaudeP;

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub workspace: PathBuf,
    pub doc_id: String,
    pub agent_id: String,
    pub branch: String,
}

pub struct AgentHandle {
    pub pid: u32,
    pub events: Receiver<AgentEvent>,
    pub cancel: Sender<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted,
    Text {
        delta: String,
    },
    ToolCallStarted {
        name: String,
    },
    ToolCall {
        name: String,
        summary: String,
        status: ToolStatus,
    },
    TurnCompleted {
        input_tokens: u64,
        output_tokens: u64,
    },
    SubprocessExited {
        code: Option<i32>,
    },
}

pub trait AgentRunner {
    fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_event_variants_are_exhaustive() {
        let ctx = AgentContext {
            workspace: PathBuf::from("/tmp/ws"),
            doc_id: "STORY-127".into(),
            agent_id: "claude-bot".into(),
            branch: "feat/x".into(),
        };
        assert_eq!(ctx.doc_id, "STORY-127");

        let events = vec![
            AgentEvent::SessionStarted,
            AgentEvent::Text { delta: "hi".into() },
            AgentEvent::ToolCallStarted {
                name: "Read".into(),
            },
            AgentEvent::ToolCall {
                name: "Read".into(),
                summary: "/etc/hosts".into(),
                status: ToolStatus::Ok,
            },
            AgentEvent::TurnCompleted {
                input_tokens: 1,
                output_tokens: 2,
            },
            AgentEvent::SubprocessExited { code: Some(0) },
            AgentEvent::SubprocessExited { code: None },
        ];

        for ev in events {
            match ev {
                AgentEvent::SessionStarted => {}
                AgentEvent::Text { .. } => {}
                AgentEvent::ToolCallStarted { .. } => {}
                AgentEvent::ToolCall { .. } => {}
                AgentEvent::TurnCompleted { .. } => {}
                AgentEvent::SubprocessExited { .. } => {}
            }
        }
    }
}
