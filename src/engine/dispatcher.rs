//! Pure candidate selection for the orchestrator tick loop.
//!
//! Filters in-memory candidate docs by eligibility (status, assignee, lease,
//! running) and reports remaining concurrency slots. No I/O. Callers wire it
//! to the store and lease/runner state.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use super::config::OrchestrationConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub doc_id: String,
    pub doc_type: String,
    pub status: String,
    pub priority: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub assignees: Vec<String>,
}

pub struct Dispatcher<'a> {
    pub orchestration: &'a OrchestrationConfig,
    pub active_lease_ids: &'a HashSet<String>,
    pub running_ids: &'a HashSet<String>,
}

impl<'a> Dispatcher<'a> {
    pub fn eligible(&self, candidates: &[Candidate]) -> Vec<Candidate> {
        let agent_users: HashSet<&String> = self.orchestration.agent_users.iter().collect();
        let active_statuses: HashSet<&String> = self.orchestration.active_statuses.iter().collect();

        let mut out: Vec<Candidate> = candidates
            .iter()
            .filter(|c| {
                active_statuses.contains(&c.status)
                    && c.assignees.iter().any(|a| agent_users.contains(a))
                    && !self.active_lease_ids.contains(&c.doc_id)
                    && !self.running_ids.contains(&c.doc_id)
            })
            .cloned()
            .collect();

        out.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        out
    }

    pub fn slots_available(&self, running_count: usize) -> usize {
        self.orchestration
            .max_concurrent_agents
            .saturating_sub(running_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::OrchestrationConfig;
    use std::path::PathBuf;

    fn orch(agent_users: Vec<&str>, active_statuses: Vec<&str>, max: usize) -> OrchestrationConfig {
        OrchestrationConfig {
            agent_users: agent_users.into_iter().map(String::from).collect(),
            claim_type: "story".into(),
            branch_template: "agent/{doc_id}".into(),
            workspace_root: PathBuf::from(".lazyspec/workspaces"),
            base_branch: "main".into(),
            runtime: Default::default(),
            hooks: Default::default(),
            poll_interval_ms: 1000,
            max_concurrent_agents: max,
            active_statuses: active_statuses.into_iter().map(String::from).collect(),
            heartbeat_interval_ms: 1000,
            metadata_push_interval_ms: 1000,
            stall_timeout_ms: 300_000,
            max_turns: 20,
            max_failure_attempts: 5,
            max_retry_backoff_ms: 300_000,
            handoff_states: vec!["in-review".to_string()],
            continuation_delay_ms: 1_000,
        }
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
    }

    fn cand(
        id: &str,
        status: &str,
        priority: Option<i64>,
        created: i64,
        assignees: Vec<&str>,
    ) -> Candidate {
        Candidate {
            doc_id: id.into(),
            doc_type: "story".into(),
            status: status.into(),
            priority,
            created_at: ts(created),
            assignees: assignees.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn eligible_filters_by_status() {
        let o = orch(vec!["bot"], vec!["todo"], 4);
        let leases = HashSet::new();
        let running = HashSet::new();
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };

        let candidates = vec![
            cand("S-1", "todo", Some(1), 100, vec!["bot"]),
            cand("S-2", "done", Some(1), 100, vec!["bot"]),
        ];

        let result = d.eligible(&candidates);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].doc_id, "S-1");
    }

    #[test]
    fn eligible_requires_agent_assignee_intersection() {
        let o = orch(vec!["bot"], vec!["todo"], 4);
        let leases = HashSet::new();
        let running = HashSet::new();
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };

        let candidates = vec![
            cand("S-1", "todo", Some(1), 100, vec!["bot"]),
            cand("S-2", "todo", Some(1), 100, vec!["human"]),
            cand("S-3", "todo", Some(1), 100, vec![]),
        ];

        let result = d.eligible(&candidates);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].doc_id, "S-1");
    }

    #[test]
    fn eligible_excludes_locally_leased_doc() {
        let o = orch(vec!["bot"], vec!["todo"], 4);
        let mut leases = HashSet::new();
        leases.insert("S-2".to_string());
        let running = HashSet::new();
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };

        let candidates = vec![
            cand("S-1", "todo", Some(1), 100, vec!["bot"]),
            cand("S-2", "todo", Some(1), 100, vec!["bot"]),
        ];

        let result = d.eligible(&candidates);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].doc_id, "S-1");
    }

    #[test]
    fn eligible_excludes_running_doc() {
        let o = orch(vec!["bot"], vec!["todo"], 4);
        let leases = HashSet::new();
        let mut running = HashSet::new();
        running.insert("S-1".to_string());
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };

        let candidates = vec![
            cand("S-1", "todo", Some(1), 100, vec!["bot"]),
            cand("S-2", "todo", Some(1), 100, vec!["bot"]),
        ];

        let result = d.eligible(&candidates);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].doc_id, "S-2");
    }

    #[test]
    fn eligible_sort_priority_then_created_then_id() {
        let o = orch(vec!["bot"], vec!["todo"], 4);
        let leases = HashSet::new();
        let running = HashSet::new();
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };

        // Mixed priorities, created_at, and ids.
        // Expected order:
        //   priority 1, created 100, id "B-1"   (lowest priority wins)
        //   priority 1, created 200, id "A-2"   (same prio, older first)
        //   priority 2, created 50,  id "C-3"   (later priority bucket)
        //   priority 2, created 50,  id "D-4"   (tied -> id asc)
        let candidates = vec![
            cand("A-2", "todo", Some(1), 200, vec!["bot"]),
            cand("D-4", "todo", Some(2), 50, vec!["bot"]),
            cand("C-3", "todo", Some(2), 50, vec!["bot"]),
            cand("B-1", "todo", Some(1), 100, vec!["bot"]),
        ];

        let result = d.eligible(&candidates);
        let ids: Vec<&str> = result.iter().map(|c| c.doc_id.as_str()).collect();
        assert_eq!(ids, vec!["B-1", "A-2", "C-3", "D-4"]);
    }

    #[test]
    fn slots_available_respects_max_minus_running() {
        let o = orch(vec!["bot"], vec!["todo"], 3);
        let leases = HashSet::new();
        let running = HashSet::new();
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };
        assert_eq!(d.slots_available(2), 1);
    }

    #[test]
    fn slots_available_saturates_at_zero_when_over_cap() {
        let o = orch(vec!["bot"], vec!["todo"], 3);
        let leases = HashSet::new();
        let running = HashSet::new();
        let d = Dispatcher {
            orchestration: &o,
            active_lease_ids: &leases,
            running_ids: &running,
        };
        assert_eq!(d.slots_available(5), 0);
    }
}
