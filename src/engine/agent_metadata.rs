use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::git_ref::GitRefOps;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Crashed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentMetadata {
    pub agent_id: String,
    pub session_id: String,
    pub doc_id: String,
    pub doc_type: String,
    pub status: AgentStatus,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub turn_count: u32,
    pub error: Option<String>,
    pub session_start_iteration_ids: Vec<String>,
}

const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

/// Writes agent session metadata under `refs/lazyspec/agents/{session-id}` as
/// a commit chain (mirror of RFC-035 lease pattern). Each write appends a
/// commit whose parent is the prior chain head, so history is preserved
/// (STORY-124 AC1, AC3).
pub trait AgentMetadataWriter: Send + Sync {
    /// Append a metadata commit and advance the local ref. Returns the new
    /// chain-head sha. Push to remote is the caller's responsibility (Group B).
    fn write(&self, metadata: &AgentMetadata) -> Result<String>;

    /// Mark a session crashed, preserving prior chain-head fields where
    /// possible. Used by boot orphan recovery.
    fn mark_crashed(&self, session_id: &str) -> Result<()>;
}

pub struct GitRefAgentMetadata<G: GitRefOps> {
    pub root: PathBuf,
    pub git: G,
}

impl<G: GitRefOps> GitRefAgentMetadata<G> {
    pub fn new(root: PathBuf, git: G) -> Self {
        Self { root, git }
    }
}

fn agent_ref(session_id: &str) -> String {
    format!("refs/lazyspec/agents/{}", session_id)
}

impl<G: GitRefOps + Send + Sync> AgentMetadataWriter for GitRefAgentMetadata<G> {
    fn write(&self, metadata: &AgentMetadata) -> Result<String> {
        let refname = agent_ref(&metadata.session_id);
        let prev_sha = self.git.resolve_ref(&self.root, &refname)?;
        let json = serde_json::to_string_pretty(metadata)?;
        let new_sha = self.git.create_commit(
            &self.root,
            &refname,
            &[("metadata.json", &json)],
            prev_sha.as_deref(),
        )?;
        // CAS local ref. First write uses ZERO_SHA sentinel ("must not exist"),
        // matching lease.rs::acquire and create_ref_commit's prior convention.
        // Group B owns the remote push (push_ref_with_lease with the same CAS).
        let expected_old = prev_sha.as_deref().unwrap_or(ZERO_SHA);
        self.git
            .update_ref(&self.root, &refname, &new_sha, expected_old)?;
        Ok(new_sha)
    }

    fn mark_crashed(&self, session_id: &str) -> Result<()> {
        let refname = agent_ref(session_id);
        let prev_sha = self.git.resolve_ref(&self.root, &refname)?;

        let metadata = match prev_sha.as_deref() {
            Some(sha) => {
                let blob = self.git.read_ref_blob(&self.root, sha, "metadata.json")?;
                let mut md: AgentMetadata = serde_json::from_str(&blob)?;
                md.status = AgentStatus::Crashed;
                md.last_event_at = Utc::now();
                md
            }
            // Fallback: ref exists but blob is unreadable, or first-ever write
            // is a crash marker (shouldn't happen in real boot recovery — the
            // ref only exists if an agent ever wrote it). Emit a minimal
            // record so the boot path still produces a crashed marker.
            None => {
                let now = Utc::now();
                AgentMetadata {
                    agent_id: String::new(),
                    session_id: session_id.to_string(),
                    doc_id: String::new(),
                    doc_type: String::new(),
                    status: AgentStatus::Crashed,
                    started_at: now,
                    last_event_at: now,
                    tokens_in: 0,
                    tokens_out: 0,
                    turn_count: 0,
                    error: None,
                    session_start_iteration_ids: vec![],
                }
            }
        };

        self.write(&metadata)?;
        Ok(())
    }
}

/// Read the latest metadata for `session_id` from `refs/lazyspec/agents/{session_id}`.
///
/// Free function (no writer/daemon required) so TUI, CLI, and ad-hoc consumers
/// can inspect live session state directly via git refs (STORY-124 AC8,
/// RFC-041 § metadata refs).
pub fn read_agent_metadata<G: GitRefOps>(
    git: &G,
    root: &Path,
    session_id: &str,
) -> Result<Option<AgentMetadata>> {
    let refname = agent_ref(session_id);
    let sha = match git.resolve_ref(root, &refname)? {
        Some(s) => s,
        None => return Ok(None),
    };
    let blob = git.read_ref_blob(root, &sha, "metadata.json")?;
    let metadata: AgentMetadata = serde_json::from_str(&blob)?;
    Ok(Some(metadata))
}

pub struct NullAgentMetadata;

impl AgentMetadataWriter for NullAgentMetadata {
    fn write(&self, _metadata: &AgentMetadata) -> Result<String> {
        Ok(String::new())
    }

    fn mark_crashed(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::git_ref::GitRefOps;
    use anyhow::bail;
    use chrono::{DateTime, Utc};
    use std::path::Path;
    use std::sync::Mutex;

    fn dummy_root() -> PathBuf {
        PathBuf::from("/tmp/fake")
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CreateCommitCall {
        refname: String,
        files: Vec<(String, String)>,
        parent: Option<String>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct UpdateRefCall {
        refname: String,
        new_sha: String,
        old_sha: String,
    }

    /// Minimal chain-aware fake. Task 4 will extend the shared `RecordingGit`
    /// with similar semantics; for now this inline fake covers the chained-
    /// write surface that task 2 needs to test.
    #[derive(Default)]
    struct ChainFake {
        // Mutable head sha for the ref (None = does not exist).
        head: Mutex<Option<String>>,
        // Optional blob seed for the current head (used by mark_crashed tests).
        blob: Mutex<Option<String>>,
        // Counter for synthetic commit shas.
        next_sha: Mutex<u32>,
        create_commit_calls: Mutex<Vec<CreateCommitCall>>,
        update_ref_calls: Mutex<Vec<UpdateRefCall>>,
    }

    impl ChainFake {
        fn with_head(self, sha: &str, blob: &str) -> Self {
            *self.head.lock().unwrap() = Some(sha.to_string());
            *self.blob.lock().unwrap() = Some(blob.to_string());
            self
        }
    }

    impl GitRefOps for ChainFake {
        fn resolve_ref(&self, _root: &Path, _refname: &str) -> Result<Option<String>> {
            Ok(self.head.lock().unwrap().clone())
        }
        fn list_refs(&self, _root: &Path, _pattern: &str) -> Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn read_ref_blob(&self, _root: &Path, _sha: &str, _path: &str) -> Result<String> {
            self.blob
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| anyhow::anyhow!("no blob seeded"))
        }
        fn create_commit(
            &self,
            _root: &Path,
            refname: &str,
            files: &[(&str, &str)],
            parent: Option<&str>,
        ) -> Result<String> {
            let mut n = self.next_sha.lock().unwrap();
            *n += 1;
            let new_sha = format!("sha-{}", *n);
            self.create_commit_calls.lock().unwrap().push(CreateCommitCall {
                refname: refname.to_string(),
                files: files
                    .iter()
                    .map(|(p, c)| (p.to_string(), c.to_string()))
                    .collect(),
                parent: parent.map(|s| s.to_string()),
            });
            // Update the seeded blob so subsequent reads see what we just wrote.
            if let Some((_, content)) = files.iter().find(|(p, _)| *p == "metadata.json") {
                *self.blob.lock().unwrap() = Some(content.to_string());
            }
            Ok(new_sha)
        }
        fn create_ref_commit(
            &self,
            _root: &Path,
            _refname: &str,
            _files: &[(&str, &str)],
        ) -> Result<String> {
            bail!("create_ref_commit must not be used by chained writes")
        }
        fn update_ref(
            &self,
            _root: &Path,
            refname: &str,
            new_sha: &str,
            old_sha: &str,
        ) -> Result<()> {
            self.update_ref_calls.lock().unwrap().push(UpdateRefCall {
                refname: refname.to_string(),
                new_sha: new_sha.to_string(),
                old_sha: old_sha.to_string(),
            });
            *self.head.lock().unwrap() = Some(new_sha.to_string());
            Ok(())
        }
        fn delete_ref(&self, _root: &Path, _refname: &str) -> Result<()> {
            Ok(())
        }
        fn fetch_refs(&self, _root: &Path, _remote: &str, _pattern: &str) -> Result<()> {
            Ok(())
        }
        fn push_ref(&self, _root: &Path, _remote: &str, _refname: &str) -> Result<()> {
            Ok(())
        }
        fn delete_remote_ref(
            &self,
            _root: &Path,
            _remote: &str,
            _refname: &str,
            _expected_old: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }
        fn push_ref_with_lease(
            &self,
            _root: &Path,
            _remote: &str,
            _refname: &str,
            _new_sha: &str,
            _expected_old: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }
        fn read_commit_timestamp(&self, _root: &Path, _sha: &str) -> Result<DateTime<Utc>> {
            bail!("not implemented for this test")
        }
    }

    fn sample_metadata(status: AgentStatus) -> AgentMetadata {
        AgentMetadata {
            agent_id: "agent-1".to_string(),
            session_id: "sess-1".to_string(),
            doc_id: "STORY-124".to_string(),
            doc_type: "story".to_string(),
            status,
            started_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            last_event_at: DateTime::from_timestamp(1_700_000_500, 0).unwrap(),
            tokens_in: 1234,
            tokens_out: 5678,
            turn_count: 7,
            error: Some("boom".to_string()),
            session_start_iteration_ids: vec!["ITER-176".to_string(), "ITER-177".to_string()],
        }
    }

    #[test]
    fn null_metadata_mark_crashed_is_noop() {
        let writer = NullAgentMetadata;
        writer.mark_crashed("sess-1").unwrap();
    }

    #[test]
    fn null_metadata_write_returns_empty_sha() {
        let writer = NullAgentMetadata;
        let sha = writer.write(&sample_metadata(AgentStatus::Running)).unwrap();
        assert!(sha.is_empty());
    }

    #[test]
    fn write_first_commit_uses_none_parent_and_zero_sha_cas() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        let new_sha = writer.write(&sample_metadata(AgentStatus::Running)).unwrap();

        let commits = writer.git.create_commit_calls.lock().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].refname, "refs/lazyspec/agents/sess-1");
        assert_eq!(commits[0].parent, None);
        assert_eq!(commits[0].files.len(), 1);
        assert_eq!(commits[0].files[0].0, "metadata.json");

        let updates = writer.git.update_ref_calls.lock().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].new_sha, new_sha);
        assert_eq!(updates[0].old_sha, ZERO_SHA);
    }

    #[test]
    fn write_appends_chain() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        let first_sha = writer.write(&sample_metadata(AgentStatus::Running)).unwrap();
        let second_sha = writer.write(&sample_metadata(AgentStatus::Crashed)).unwrap();
        assert_ne!(first_sha, second_sha);

        let commits = writer.git.create_commit_calls.lock().unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parent, None);
        assert_eq!(commits[1].parent, Some(first_sha.clone()));

        let updates = writer.git.update_ref_calls.lock().unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].old_sha, ZERO_SHA);
        assert_eq!(updates[0].new_sha, first_sha);
        assert_eq!(updates[1].old_sha, first_sha);
        assert_eq!(updates[1].new_sha, second_sha);
    }

    #[test]
    fn mark_crashed_preserves_prior_fields() {
        let prior = sample_metadata(AgentStatus::Running);
        let prior_json = serde_json::to_string_pretty(&prior).unwrap();
        let fake = ChainFake::default().with_head("prev-sha", &prior_json);
        let writer = GitRefAgentMetadata::new(dummy_root(), fake);

        writer.mark_crashed("sess-1").unwrap();

        let commits = writer.git.create_commit_calls.lock().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].parent, Some("prev-sha".to_string()));

        let written: AgentMetadata = serde_json::from_str(&commits[0].files[0].1).unwrap();
        assert_eq!(written.status, AgentStatus::Crashed);
        // Preserved fields from prior:
        assert_eq!(written.agent_id, "agent-1");
        assert_eq!(written.doc_id, "STORY-124");
        assert_eq!(written.doc_type, "story");
        assert_eq!(written.tokens_in, 1234);
        assert_eq!(written.tokens_out, 5678);
        assert_eq!(written.turn_count, 7);
        assert_eq!(written.started_at, prior.started_at);
        assert_eq!(
            written.session_start_iteration_ids,
            vec!["ITER-176".to_string(), "ITER-177".to_string()]
        );
    }

    #[test]
    fn mark_crashed_no_prev_writes_minimal_record() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        writer.mark_crashed("sess-orphan").unwrap();

        let commits = writer.git.create_commit_calls.lock().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].parent, None);
        assert_eq!(commits[0].refname, "refs/lazyspec/agents/sess-orphan");

        let written: AgentMetadata = serde_json::from_str(&commits[0].files[0].1).unwrap();
        assert_eq!(written.status, AgentStatus::Crashed);
        assert_eq!(written.session_id, "sess-orphan");
        assert_eq!(written.agent_id, "");
        assert_eq!(written.turn_count, 0);

        let updates = writer.git.update_ref_calls.lock().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].old_sha, ZERO_SHA);
    }

    #[test]
    fn agent_metadata_round_trips_through_serde_json() {
        for status in [AgentStatus::Running, AgentStatus::Crashed] {
            let original = sample_metadata(status);
            let json = serde_json::to_string(&original).unwrap();
            let decoded: AgentMetadata = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, original);
        }
    }

    #[test]
    fn read_agent_metadata_returns_latest() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        let first = sample_metadata(AgentStatus::Running);
        writer.write(&first).unwrap();
        let mut second = sample_metadata(AgentStatus::Crashed);
        second.turn_count = 99;
        second.tokens_in = 4242;
        writer.write(&second).unwrap();

        let got = read_agent_metadata(&writer.git, &writer.root, "sess-1")
            .unwrap()
            .expect("expected Some metadata");
        assert_eq!(got, second);
    }

    #[test]
    fn read_agent_metadata_returns_none_for_missing_session() {
        let fake = ChainFake::default();
        let root = dummy_root();
        let got = read_agent_metadata(&fake, &root, "no-such-sid").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn agent_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&AgentStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&AgentStatus::Crashed).unwrap(),
            "\"crashed\""
        );
    }
}
