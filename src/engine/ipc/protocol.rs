use serde::{Deserialize, Serialize};

use crate::engine::agent_metadata::AgentStatus as AgentLifecycleStatus;
use crate::engine::runner::AgentEvent;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Subscribe,
    Unsubscribe,
    Cancel {
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },
    Status,
    Kick,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMessage {
    AgentEvent {
        agent_id: String,
        session_id: String,
        doc_id: String,
        #[serde(flatten)]
        event: AgentEvent,
    },
    AgentStatus {
        agent_id: String,
        session_id: String,
        status: AgentLifecycleStatus,
    },
    DaemonStatus {
        agents: Vec<AgentSnapshot>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub agent_id: String,
    pub session_id: String,
    pub doc_id: String,
    pub elapsed_ms: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn client_msg_round_trips_subscribe_unsubscribe_cancel_status_kick() {
        let cases = [
            (json!({"type": "subscribe"}), ClientMessage::Subscribe),
            (json!({"type": "unsubscribe"}), ClientMessage::Unsubscribe),
            (
                json!({"type": "cancel", "agent_id": "a1", "session_id": "s1"}),
                ClientMessage::Cancel {
                    agent_id: Some("a1".into()),
                    session_id: Some("s1".into()),
                },
            ),
            (json!({"type": "status"}), ClientMessage::Status),
            (json!({"type": "kick"}), ClientMessage::Kick),
        ];

        for (wire, expected) in cases {
            let parsed: ClientMessage = serde_json::from_value(wire.clone())
                .unwrap_or_else(|e| panic!("failed to parse {wire}: {e}"));
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn daemon_msg_round_trips_event_status_daemon_status_error() {
        let event_msg = DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            doc_id: "STORY-1".into(),
            event: AgentEvent::Text { delta: "hi".into() },
        };
        let v: Value = serde_json::from_str(&serde_json::to_string(&event_msg).unwrap()).unwrap();
        assert_eq!(v["type"], "agent_event");
        assert_eq!(v["agent_id"], "a1");
        assert_eq!(v["session_id"], "s1");
        assert_eq!(v["doc_id"], "STORY-1");
        assert_eq!(v["event_type"], "text");
        assert_eq!(v["delta"], "hi");

        let status_msg = DaemonMessage::AgentStatus {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            status: AgentLifecycleStatus::Running,
        };
        let v: Value = serde_json::from_str(&serde_json::to_string(&status_msg).unwrap()).unwrap();
        assert_eq!(v["type"], "agent_status");
        assert_eq!(v["agent_id"], "a1");
        assert_eq!(v["session_id"], "s1");
        assert_eq!(v["status"], "running");

        let daemon_status = DaemonMessage::DaemonStatus {
            agents: vec![AgentSnapshot {
                agent_id: "a1".into(),
                session_id: "s1".into(),
                doc_id: "STORY-1".into(),
                elapsed_ms: 1234,
                tokens_in: 10,
                tokens_out: 20,
            }],
        };
        let v: Value =
            serde_json::from_str(&serde_json::to_string(&daemon_status).unwrap()).unwrap();
        assert_eq!(v["type"], "daemon_status");
        assert_eq!(v["agents"][0]["agent_id"], "a1");
        assert_eq!(v["agents"][0]["session_id"], "s1");
        assert_eq!(v["agents"][0]["doc_id"], "STORY-1");
        assert_eq!(v["agents"][0]["elapsed_ms"], 1234);
        assert_eq!(v["agents"][0]["tokens_in"], 10);
        assert_eq!(v["agents"][0]["tokens_out"], 20);

        let err = DaemonMessage::Error {
            message: "boom".into(),
        };
        let v: Value = serde_json::from_str(&serde_json::to_string(&err).unwrap()).unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["message"], "boom");
    }

    #[test]
    fn cancel_accepts_agent_id_or_session_id() {
        let by_agent: ClientMessage =
            serde_json::from_str(r#"{"type":"cancel","agent_id":"a1"}"#).unwrap();
        assert_eq!(
            by_agent,
            ClientMessage::Cancel {
                agent_id: Some("a1".into()),
                session_id: None,
            }
        );

        let by_session: ClientMessage =
            serde_json::from_str(r#"{"type":"cancel","session_id":"s1"}"#).unwrap();
        assert_eq!(
            by_session,
            ClientMessage::Cancel {
                agent_id: None,
                session_id: Some("s1".into()),
            }
        );

        let neither: ClientMessage = serde_json::from_str(r#"{"type":"cancel"}"#).unwrap();
        assert_eq!(
            neither,
            ClientMessage::Cancel {
                agent_id: None,
                session_id: None,
            }
        );
    }

    #[test]
    fn daemon_msg_event_round_trips_through_deserialize() {
        let orig = DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            doc_id: "STORY-1".into(),
            event: AgentEvent::Text { delta: "hi".into() },
        };
        let s = serde_json::to_string(&orig).unwrap();
        let parsed: DaemonMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, orig);

        let orig2 = DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            doc_id: "STORY-1".into(),
            event: AgentEvent::TurnCompleted {
                input_tokens: 10,
                output_tokens: 20,
            },
        };
        let s2 = serde_json::to_string(&orig2).unwrap();
        let parsed2: DaemonMessage = serde_json::from_str(&s2).unwrap();
        assert_eq!(parsed2, orig2);

        let status_msg = DaemonMessage::DaemonStatus {
            agents: vec![AgentSnapshot {
                agent_id: "a1".into(),
                session_id: "s1".into(),
                doc_id: "STORY-1".into(),
                elapsed_ms: 0,
                tokens_in: 0,
                tokens_out: 0,
            }],
        };
        let s3 = serde_json::to_string(&status_msg).unwrap();
        let parsed3: DaemonMessage = serde_json::from_str(&s3).unwrap();
        assert_eq!(parsed3, status_msg);

        let err = DaemonMessage::Error {
            message: "x".into(),
        };
        let s4 = serde_json::to_string(&err).unwrap();
        let parsed4: DaemonMessage = serde_json::from_str(&s4).unwrap();
        assert_eq!(parsed4, err);
    }

    #[test]
    fn unknown_type_returns_serde_err() {
        let err = serde_json::from_str::<ClientMessage>(r#"{"type":"unknown"}"#);
        assert!(err.is_err());
    }
}
