use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::engine::agent_metadata::{AgentMetadata, AgentStatus};
use crate::engine::ipc::protocol::{AgentSnapshot, DaemonMessage};
use crate::engine::ipc::ConnectionState;
use crate::engine::runner::AgentEvent;

const OUTPUT_BUFFER_CAP: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Live,
    Offline,
}

#[derive(Debug, Clone)]
pub struct AgentsViewState {
    pub snapshots: BTreeMap<String, AgentSnapshot>,
    pub statuses: HashMap<String, AgentStatus>,
    pub output: HashMap<String, String>,
    pub connection: ConnectionState,
    pub source: DataSource,
    pub selected: usize,
    pub synced_at: HashMap<String, Instant>,
}

impl Default for AgentsViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentsViewState {
    pub fn new() -> Self {
        Self {
            snapshots: BTreeMap::new(),
            statuses: HashMap::new(),
            output: HashMap::new(),
            connection: ConnectionState::Reconnecting,
            source: DataSource::Live,
            selected: 0,
            synced_at: HashMap::new(),
        }
    }

    pub fn apply(&mut self, msg: DaemonMessage) {
        match msg {
            DaemonMessage::AgentEvent {
                session_id, event, ..
            } => match event {
                AgentEvent::Text { delta } => {
                    let buf = self.output.entry(session_id).or_default();
                    buf.push_str(&delta);
                    trim_to_cap(buf, OUTPUT_BUFFER_CAP);
                }
                AgentEvent::TurnCompleted {
                    input_tokens,
                    output_tokens,
                }
                | AgentEvent::TokenUsage {
                    input_tokens,
                    output_tokens,
                } => {
                    let snap =
                        self.snapshots
                            .entry(session_id.clone())
                            .or_insert_with(|| AgentSnapshot {
                                agent_id: String::new(),
                                session_id: session_id.clone(),
                                doc_id: String::new(),
                                elapsed_ms: 0,
                                tokens_in: 0,
                                tokens_out: 0,
                            });
                    snap.tokens_in = input_tokens;
                    snap.tokens_out = output_tokens;
                }
                AgentEvent::SessionStarted
                | AgentEvent::ToolCallStarted { .. }
                | AgentEvent::ToolCall { .. }
                | AgentEvent::SubprocessExited { .. } => {}
            },
            DaemonMessage::AgentStatus {
                session_id, status, ..
            } => {
                self.statuses.insert(session_id, status);
            }
            DaemonMessage::DaemonStatus { agents } => {
                let new_snapshots: BTreeMap<String, AgentSnapshot> = agents
                    .into_iter()
                    .map(|a| (a.session_id.clone(), a))
                    .collect();
                self.output.retain(|k, _| new_snapshots.contains_key(k));
                self.statuses.retain(|k, _| new_snapshots.contains_key(k));
                let now = Instant::now();
                for k in new_snapshots.keys() {
                    self.synced_at.insert(k.clone(), now);
                }
                self.synced_at.retain(|k, _| new_snapshots.contains_key(k));
                self.snapshots = new_snapshots;
                self.clamp_selected();
            }
            DaemonMessage::Error { .. } => {}
        }
    }

    pub fn load_offline(&mut self, sessions: Vec<AgentMetadata>) {
        self.snapshots.clear();
        for m in sessions {
            let elapsed_ms = (m.last_event_at - m.started_at).num_milliseconds().max(0) as u64;
            let snap = AgentSnapshot {
                agent_id: m.agent_id,
                session_id: m.session_id.clone(),
                doc_id: m.doc_id,
                elapsed_ms,
                tokens_in: m.tokens_in,
                tokens_out: m.tokens_out,
            };
            self.statuses.insert(m.session_id.clone(), m.status);
            self.snapshots.insert(m.session_id, snap);
        }
        self.source = DataSource::Offline;
        self.clamp_selected();
    }

    pub fn set_connection(&mut self, state: ConnectionState) {
        let was = self.connection;
        self.connection = state;
        if state == ConnectionState::Connected && was != ConnectionState::Connected {
            self.source = DataSource::Live;
            self.snapshots.clear();
            self.synced_at.clear();
            self.clamp_selected();
        }
    }

    pub fn selected_session(&self) -> Option<&str> {
        self.snapshots.keys().nth(self.selected).map(String::as_str)
    }

    pub fn effective_elapsed_ms(&self, session_id: &str) -> Option<u64> {
        let snap = self.snapshots.get(session_id)?;
        let baseline = snap.elapsed_ms;
        let delta = self
            .synced_at
            .get(session_id)
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        Some(baseline + delta)
    }

    pub fn aggregate_tokens(&self) -> (u64, u64) {
        self.snapshots.values().fold((0u64, 0u64), |(ti, to), s| {
            (ti + s.tokens_in, to + s.tokens_out)
        })
    }

    pub fn counts_by_status(&self) -> HashMap<AgentStatus, u32> {
        let mut counts = HashMap::new();
        for status in self.statuses.values() {
            *counts.entry(status.clone()).or_insert(0u32) += 1;
        }
        counts
    }

    pub fn select_next(&mut self) {
        let max = self.snapshots.len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn clamp_selected(&mut self) {
        let len = self.snapshots.len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
}

/// Build the list of documents eligible for manual kickoff: documents whose
/// type matches `orchestration.claim_type`, whose status is in
/// `orchestration.active_statuses`, and which are not already assigned to any
/// configured agent user (prevents redundant kickoffs).
#[cfg(feature = "agent")]
pub fn build_kickoff_candidates(
    docs: &[&crate::engine::document::DocMeta],
    orchestration: &crate::engine::config::OrchestrationConfig,
) -> Vec<crate::tui::state::forms::KickoffCandidate> {
    docs.iter()
        .filter(|d| d.doc_type.as_str() == orchestration.claim_type)
        .filter(|d| {
            orchestration
                .active_statuses
                .iter()
                .any(|s| s == &d.status.to_string())
        })
        .filter(|d| {
            !d.assignees
                .iter()
                .any(|a| orchestration.agent_users.iter().any(|au| au == a))
        })
        .map(|d| crate::tui::state::forms::KickoffCandidate {
            doc_id: d.id.clone(),
            title: d.title.clone(),
            type_name: orchestration.claim_type.clone(),
            current_assignees: d.assignees.clone(),
        })
        .collect()
}

fn trim_to_cap(buf: &mut String, cap: usize) {
    if buf.len() <= cap {
        return;
    }
    let overflow = buf.len() - cap;
    // Find the first char boundary at or after `overflow` so we don't split a
    // multi-byte char. `char_indices` yields valid boundaries; fall back to
    // the buffer length if every boundary is before `overflow`.
    let split = buf
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= overflow)
        .unwrap_or(buf.len());
    buf.drain(..split);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn text_event(session: &str, delta: &str) -> DaemonMessage {
        DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: session.into(),
            event: AgentEvent::Text {
                delta: delta.into(),
            },
        }
    }

    fn snapshot(session: &str, doc_id: &str, tokens_in: u64, tokens_out: u64) -> AgentSnapshot {
        AgentSnapshot {
            agent_id: format!("agent-{session}"),
            session_id: session.into(),
            doc_id: doc_id.into(),
            elapsed_ms: 0,
            tokens_in,
            tokens_out,
        }
    }

    #[test]
    fn apply_text_event_appends_to_session_buffer() {
        let mut s = AgentsViewState::new();
        s.apply(text_event("s1", "hello "));
        s.apply(text_event("s1", "world"));
        assert_eq!(s.output.get("s1").map(String::as_str), Some("hello world"));
    }

    #[test]
    fn apply_text_event_buffers_per_session() {
        let mut s = AgentsViewState::new();
        s.apply(text_event("s1", "alpha"));
        s.apply(text_event("s2", "beta"));
        assert_eq!(s.output.get("s1").map(String::as_str), Some("alpha"));
        assert_eq!(s.output.get("s2").map(String::as_str), Some("beta"));
    }

    #[test]
    fn output_buffer_caps_at_64kib() {
        let mut s = AgentsViewState::new();
        let big = "x".repeat(70_000);
        let suffix = "TAIL_MARKER";
        let payload = format!("{big}{suffix}");
        s.apply(text_event("s1", &payload));
        let buf = s.output.get("s1").expect("buffer exists");
        assert!(
            buf.len() <= OUTPUT_BUFFER_CAP,
            "buffer len {} > cap {}",
            buf.len(),
            OUTPUT_BUFFER_CAP
        );
        assert!(
            buf.ends_with(suffix),
            "buffer should end with the most recent suffix"
        );
    }

    #[test]
    fn apply_turn_completed_updates_snapshot_tokens() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "STORY-1", 5, 5)],
        });
        s.apply(DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            event: AgentEvent::TurnCompleted {
                input_tokens: 42,
                output_tokens: 99,
            },
        });
        let snap = s.snapshots.get("s1").expect("snapshot present");
        assert_eq!(snap.tokens_in, 42);
        assert_eq!(snap.tokens_out, 99);
    }

    #[test]
    fn apply_token_usage_updates_existing_snapshot_tokens() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "STORY-1", 0, 0)],
        });
        s.apply(DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            event: AgentEvent::TokenUsage {
                input_tokens: 7,
                output_tokens: 11,
            },
        });
        let snap = s.snapshots.get("s1").expect("snapshot present");
        assert_eq!(snap.tokens_in, 7);
        assert_eq!(snap.tokens_out, 11);
    }

    #[test]
    fn apply_token_usage_creates_stub_when_snapshot_absent() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            event: AgentEvent::TokenUsage {
                input_tokens: 7,
                output_tokens: 11,
            },
        });
        let snap = s.snapshots.get("s1").expect("stub snapshot created");
        assert_eq!(snap.tokens_in, 7);
        assert_eq!(snap.tokens_out, 11);
        assert_eq!(snap.doc_id, "");
        assert_eq!(snap.agent_id, "");
    }

    #[test]
    fn apply_agent_status_updates_statuses() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::AgentStatus {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            status: AgentStatus::Running,
        });
        let counts = s.counts_by_status();
        assert_eq!(counts.get(&AgentStatus::Running).copied(), Some(1));
    }

    #[test]
    fn apply_daemon_status_replaces_snapshots_preserving_output() {
        let mut s = AgentsViewState::new();
        s.apply(text_event("s1", "history"));
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![
                snapshot("s1", "STORY-1", 0, 0),
                snapshot("s2", "STORY-2", 0, 0),
            ],
        });
        assert_eq!(s.output.get("s1").map(String::as_str), Some("history"));
        assert!(s.snapshots.contains_key("s1"));
        assert!(s.snapshots.contains_key("s2"));
    }

    #[test]
    fn apply_daemon_status_prunes_orphan_statuses_and_output() {
        let mut s = AgentsViewState::new();
        s.apply(text_event("s1", "s1-buf"));
        s.apply(text_event("s2", "s2-buf"));
        s.apply(DaemonMessage::AgentStatus {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            status: AgentStatus::Running,
        });
        s.apply(DaemonMessage::AgentStatus {
            agent_id: "a2".into(),
            session_id: "s2".into(),
            status: AgentStatus::Crashed,
        });
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![
                snapshot("s1", "STORY-1", 0, 0),
                snapshot("s2", "STORY-2", 0, 0),
            ],
        });

        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "STORY-1", 0, 0)],
        });

        assert!(s.statuses.contains_key("s1"));
        assert!(!s.statuses.contains_key("s2"));
        assert_eq!(s.output.get("s1").map(String::as_str), Some("s1-buf"));
        assert!(!s.output.contains_key("s2"));
        let counts = s.counts_by_status();
        assert_eq!(counts.get(&AgentStatus::Running).copied(), Some(1));
        assert_eq!(counts.get(&AgentStatus::Crashed).copied(), None);
    }

    #[test]
    fn load_offline_populates_from_metadata() {
        let mut s = AgentsViewState::new();
        let now = Utc::now();
        let sessions = vec![
            AgentMetadata {
                agent_id: "a1".into(),
                session_id: "s1".into(),
                doc_id: "STORY-1".into(),
                doc_type: "story".into(),
                status: AgentStatus::Running,
                started_at: now,
                last_event_at: now + Duration::milliseconds(1500),
                tokens_in: 10,
                tokens_out: 20,
                turn_count: 1,
                error: None,
                session_start_iteration_ids: vec![],
            },
            AgentMetadata {
                agent_id: "a2".into(),
                session_id: "s2".into(),
                doc_id: "STORY-2".into(),
                doc_type: "story".into(),
                status: AgentStatus::Crashed,
                started_at: now,
                last_event_at: now + Duration::milliseconds(500),
                tokens_in: 3,
                tokens_out: 7,
                turn_count: 1,
                error: None,
                session_start_iteration_ids: vec![],
            },
        ];
        s.load_offline(sessions);
        assert_eq!(s.snapshots.len(), 2);
        assert_eq!(s.snapshots.get("s1").unwrap().elapsed_ms, 1500);
        assert_eq!(s.statuses.get("s1"), Some(&AgentStatus::Running));
        assert_eq!(s.statuses.get("s2"), Some(&AgentStatus::Crashed));
        assert_eq!(s.source, DataSource::Offline);
    }

    #[test]
    fn view_state_survives_reconnect_event_gap() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![AgentSnapshot {
                agent_id: "a1".into(),
                session_id: "sess-1".into(),
                doc_id: "STORY-1".into(),
                elapsed_ms: 0,
                tokens_in: 0,
                tokens_out: 0,
            }],
        });
        s.apply(DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "sess-1".into(),
            event: AgentEvent::Text {
                delta: "before-".into(),
            },
        });
        s.set_connection(ConnectionState::Reconnecting);
        s.apply(DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "sess-1".into(),
            event: AgentEvent::Text {
                delta: "after".into(),
            },
        });
        assert_eq!(
            s.output.get("sess-1").map(String::as_str),
            Some("before-after")
        );
        assert_eq!(s.connection, ConnectionState::Reconnecting);
    }

    #[test]
    fn set_connection_to_connected_clears_snapshots_keeps_output() {
        let mut s = AgentsViewState::new();
        s.apply(text_event("s1", "buffered"));
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "STORY-1", 1, 1)],
        });
        s.source = DataSource::Offline;

        s.set_connection(ConnectionState::Connected);

        assert!(s.snapshots.is_empty(), "snapshots should be cleared");
        assert_eq!(
            s.output.get("s1").map(String::as_str),
            Some("buffered"),
            "output buffer should survive reconnect"
        );
        assert_eq!(s.source, DataSource::Live);
    }

    #[test]
    fn aggregate_tokens_sums_across_snapshots() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![
                snapshot("a", "d1", 1, 2),
                snapshot("b", "d2", 10, 20),
                snapshot("c", "d3", 100, 200),
            ],
        });
        assert_eq!(s.aggregate_tokens(), (111, 222));
    }

    #[test]
    fn counts_by_status_groups_correctly() {
        let mut s = AgentsViewState::new();
        for (sid, status) in [
            ("s1", AgentStatus::Running),
            ("s2", AgentStatus::Running),
            ("s3", AgentStatus::Crashed),
        ] {
            s.apply(DaemonMessage::AgentStatus {
                agent_id: "a".into(),
                session_id: sid.into(),
                status,
            });
        }
        let counts = s.counts_by_status();
        assert_eq!(counts.get(&AgentStatus::Running).copied(), Some(2));
        assert_eq!(counts.get(&AgentStatus::Crashed).copied(), Some(1));
    }

    #[test]
    fn selected_session_returns_lexicographic_order() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![
                snapshot("b", "d", 0, 0),
                snapshot("a", "d", 0, 0),
                snapshot("c", "d", 0, 0),
            ],
        });
        s.selected = 1;
        assert_eq!(s.selected_session(), Some("b"));
    }

    #[test]
    fn select_next_clamps_at_end() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("a", "d", 0, 0), snapshot("b", "d", 0, 0)],
        });
        s.selected = 1;
        s.select_next();
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn select_prev_clamps_at_zero() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("a", "d", 0, 0), snapshot("b", "d", 0, 0)],
        });
        s.selected = 0;
        s.select_prev();
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn apply_daemon_status_records_synced_at() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "d", 0, 0), snapshot("s2", "d", 0, 0)],
        });
        assert_eq!(s.synced_at.len(), 2);
        assert!(s.synced_at.contains_key("s1"));
        assert!(s.synced_at.contains_key("s2"));
    }

    #[test]
    fn apply_daemon_status_prunes_orphan_synced_at() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "d", 0, 0), snapshot("s2", "d", 0, 0)],
        });
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "d", 0, 0)],
        });
        assert_eq!(s.synced_at.len(), 1);
        assert!(s.synced_at.contains_key("s1"));
        assert!(!s.synced_at.contains_key("s2"));
    }

    #[test]
    fn effective_elapsed_ms_extrapolates_local_time() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![AgentSnapshot {
                agent_id: "a".into(),
                session_id: "s1".into(),
                doc_id: "d".into(),
                elapsed_ms: 100,
                tokens_in: 0,
                tokens_out: 0,
            }],
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        let got = s.effective_elapsed_ms("s1").expect("snapshot exists");
        assert!(
            got >= 150,
            "effective_elapsed_ms should be >= 150 (100 baseline + 50 slept), got {got}"
        );
    }

    #[test]
    fn effective_elapsed_ms_returns_none_for_unknown_session() {
        let s = AgentsViewState::new();
        assert_eq!(s.effective_elapsed_ms("nope"), None);
    }

    #[test]
    fn effective_elapsed_ms_returns_baseline_when_offline_loaded() {
        let mut s = AgentsViewState::new();
        let now = Utc::now();
        s.load_offline(vec![AgentMetadata {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            doc_id: "STORY-1".into(),
            doc_type: "story".into(),
            status: AgentStatus::Running,
            started_at: now,
            last_event_at: now + Duration::milliseconds(100),
            tokens_in: 0,
            tokens_out: 0,
            turn_count: 1,
            error: None,
            session_start_iteration_ids: vec![],
        }]);
        let got = s.effective_elapsed_ms("s1").expect("snapshot exists");
        assert_eq!(got, 100);
    }

    #[test]
    fn set_connection_connected_clears_synced_at() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "d", 0, 0)],
        });
        assert!(!s.synced_at.is_empty());
        s.set_connection(ConnectionState::Reconnecting);
        s.set_connection(ConnectionState::Connected);
        assert!(s.synced_at.is_empty());
    }

    #[test]
    fn token_usage_does_not_reset_synced_at() {
        let mut s = AgentsViewState::new();
        s.apply(DaemonMessage::DaemonStatus {
            agents: vec![snapshot("s1", "d", 0, 0)],
        });
        let before = *s.synced_at.get("s1").expect("synced_at recorded");
        s.apply(DaemonMessage::AgentEvent {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            event: AgentEvent::TokenUsage {
                input_tokens: 5,
                output_tokens: 7,
            },
        });
        let after = *s.synced_at.get("s1").expect("synced_at still present");
        assert_eq!(before, after, "TokenUsage should not update synced_at");
    }

    #[cfg(feature = "agent")]
    mod kickoff {
        use super::super::build_kickoff_candidates;
        use crate::engine::config::Config;
        use crate::engine::document::{DocMeta, DocType, Status};
        use chrono::NaiveDate;
        use std::path::PathBuf;

        fn doc(id: &str, doc_type: &str, status: Status, assignees: Vec<&str>) -> DocMeta {
            DocMeta {
                path: PathBuf::from(format!("{}.md", id)),
                title: format!("Title for {}", id),
                doc_type: DocType::new(doc_type),
                status,
                author: "tester".to_string(),
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                tags: vec![],
                provenance: vec![],
                related: vec![],
                validate_ignore: false,
                virtual_doc: false,
                id: id.to_string(),
                assignees: assignees.into_iter().map(String::from).collect(),
            }
        }

        fn orchestration() -> crate::engine::config::OrchestrationConfig {
            // Round-trip through TOML parse to get a fully populated struct
            // without needing to enumerate every default field by hand.
            let toml = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
agent_users = ["claude-bot"]
claim_type = "story"
active_statuses = ["accepted", "in-progress"]
"#;
            Config::parse(toml).unwrap().orchestration.unwrap()
        }

        #[test]
        fn candidates_filtered_by_claim_type() {
            let s1 = doc("STORY-1", "story", Status::InProgress, vec![]);
            let r1 = doc("RFC-1", "rfc", Status::InProgress, vec![]);
            let i1 = doc("ITERATION-1", "iteration", Status::InProgress, vec![]);
            let docs = vec![&s1, &r1, &i1];
            let cands = build_kickoff_candidates(&docs, &orchestration());
            assert_eq!(cands.len(), 1);
            assert_eq!(cands[0].doc_id, "STORY-1");
        }

        #[test]
        fn candidates_filtered_by_active_status() {
            let s1 = doc("STORY-1", "story", Status::InProgress, vec![]);
            let s2 = doc("STORY-2", "story", Status::Complete, vec![]);
            let s3 = doc("STORY-3", "story", Status::Draft, vec![]);
            let docs = vec![&s1, &s2, &s3];
            let cands = build_kickoff_candidates(&docs, &orchestration());
            assert_eq!(cands.len(), 1);
            assert_eq!(cands[0].doc_id, "STORY-1");
        }

        #[test]
        fn candidates_skip_already_assigned() {
            let s1 = doc("STORY-1", "story", Status::InProgress, vec!["claude-bot"]);
            let s2 = doc("STORY-2", "story", Status::InProgress, vec![]);
            let docs = vec![&s1, &s2];
            let cands = build_kickoff_candidates(&docs, &orchestration());
            assert_eq!(cands.len(), 1);
            assert_eq!(cands[0].doc_id, "STORY-2");
        }

        #[test]
        fn candidates_pass_through_non_agent_assignees() {
            let s1 = doc("STORY-1", "story", Status::InProgress, vec!["someone-else"]);
            let docs = vec![&s1];
            let cands = build_kickoff_candidates(&docs, &orchestration());
            assert_eq!(cands.len(), 1);
            assert_eq!(cands[0].doc_id, "STORY-1");
            assert_eq!(cands[0].current_assignees, vec!["someone-else".to_string()]);
            assert_eq!(cands[0].type_name, "story");
        }
    }
}
