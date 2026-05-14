use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::git_ref::GitRefOps;
use super::lease::fetch_ref_optional;

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
    // Last-pushed sha per session. Persists for the daemon lifetime so the
    // next CAS targets the most recently confirmed remote head; cleared on
    // process restart (first post-restart push re-seeds via ZERO_SHA).
    last_pushed: Mutex<HashMap<String, String>>,
}

impl<G: GitRefOps> GitRefAgentMetadata<G> {
    pub fn new(root: PathBuf, git: G) -> Self {
        Self {
            root,
            git,
            last_pushed: Mutex::new(HashMap::new()),
        }
    }
}

fn agent_ref(session_id: &str) -> String {
    format!("refs/lazyspec/agents/{}", session_id)
}

impl<G: GitRefOps> GitRefAgentMetadata<G> {
    /// Push the agent metadata ref for `session_id` to `remote`. AC4/AC7:
    /// called by the tick loop on cadence; failures are swallowed so a
    /// transient remote outage never blocks local writes. The in-memory
    /// `last_pushed` map is only advanced on success, so the next interval's
    /// CAS naturally covers all accumulated commits (the chain head sha
    /// transitively includes its ancestors).
    pub fn push(&self, session_id: &str, remote: &str) -> Result<()> {
        let refname = agent_ref(session_id);
        let new_sha = match self.git.resolve_ref(&self.root, &refname)? {
            Some(sha) => sha,
            None => return Ok(()),
        };

        let expected_old = self
            .last_pushed
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .unwrap_or_else(|| ZERO_SHA.to_string());

        match self.git.push_ref_with_lease(
            &self.root,
            remote,
            &refname,
            &new_sha,
            Some(&expected_old),
        ) {
            Ok(()) => {
                self.last_pushed
                    .lock()
                    .unwrap()
                    .insert(session_id.to_string(), new_sha);
                Ok(())
            }
            Err(e) => {
                eprintln!("metadata push {}: {}", refname, e);
                Ok(())
            }
        }
    }

    /// Fetch every clone's agent metadata refs from `remote` so this clone can
    /// read peer sessions (STORY-124 AC5). Called by the tick loop on the same
    /// `metadata_push_interval_ms` gate as `push`. Reuses `fetch_ref_optional`
    /// so an absent remote ref (first-ever fetch) is not an error.
    pub fn fetch_all(&self, remote: &str) -> Result<()> {
        fetch_ref_optional(&self.git, &self.root, remote, "refs/lazyspec/agents/*")
    }
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

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct PushLeaseCall {
        remote: String,
        refname: String,
        new_sha: String,
        expected_old: Option<String>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct FetchRefsCall {
        remote: String,
        pattern: String,
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
        push_lease_calls: Mutex<Vec<PushLeaseCall>>,
        // Queue of results for push_ref_with_lease; defaults to Ok(()) when empty.
        push_lease_results: Mutex<Vec<Result<()>>>,
        fetch_refs_calls: Mutex<Vec<FetchRefsCall>>,
        // Queue of results for fetch_refs; defaults to Ok(()) when empty.
        fetch_refs_results: Mutex<Vec<Result<()>>>,
    }

    impl ChainFake {
        fn with_head(self, sha: &str, blob: &str) -> Self {
            *self.head.lock().unwrap() = Some(sha.to_string());
            *self.blob.lock().unwrap() = Some(blob.to_string());
            self
        }

        fn queue_push_result(&self, result: Result<()>) {
            self.push_lease_results.lock().unwrap().push(result);
        }

        fn queue_fetch_result(&self, result: Result<()>) {
            self.fetch_refs_results.lock().unwrap().push(result);
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
        fn fetch_refs(&self, _root: &Path, remote: &str, pattern: &str) -> Result<()> {
            self.fetch_refs_calls.lock().unwrap().push(FetchRefsCall {
                remote: remote.to_string(),
                pattern: pattern.to_string(),
            });
            let mut q = self.fetch_refs_results.lock().unwrap();
            if q.is_empty() {
                Ok(())
            } else {
                q.remove(0)
            }
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
            remote: &str,
            refname: &str,
            new_sha: &str,
            expected_old: Option<&str>,
        ) -> Result<()> {
            self.push_lease_calls.lock().unwrap().push(PushLeaseCall {
                remote: remote.to_string(),
                refname: refname.to_string(),
                new_sha: new_sha.to_string(),
                expected_old: expected_old.map(|s| s.to_string()),
            });
            let mut q = self.push_lease_results.lock().unwrap();
            if q.is_empty() {
                Ok(())
            } else {
                q.remove(0)
            }
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
    fn push_with_no_local_ref_is_noop() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        writer.push("sess-1", "origin").unwrap();
        assert!(writer.git.push_lease_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn first_push_after_write_uses_zero_sha_expected_old() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        let new_sha = writer.write(&sample_metadata(AgentStatus::Running)).unwrap();
        writer.push("sess-1", "origin").unwrap();

        let pushes = writer.git.push_lease_calls.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].remote, "origin");
        assert_eq!(pushes[0].refname, "refs/lazyspec/agents/sess-1");
        assert_eq!(pushes[0].new_sha, new_sha);
        assert_eq!(pushes[0].expected_old.as_deref(), Some(ZERO_SHA));
    }

    #[test]
    fn second_push_uses_prior_pushed_sha_as_expected_old() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        let first_sha = writer.write(&sample_metadata(AgentStatus::Running)).unwrap();
        writer.push("sess-1", "origin").unwrap();

        let second_sha = writer.write(&sample_metadata(AgentStatus::Crashed)).unwrap();
        writer.push("sess-1", "origin").unwrap();

        let pushes = writer.git.push_lease_calls.lock().unwrap();
        assert_eq!(pushes.len(), 2);
        assert_eq!(pushes[0].expected_old.as_deref(), Some(ZERO_SHA));
        assert_eq!(pushes[0].new_sha, first_sha);
        assert_eq!(pushes[1].expected_old.as_deref(), Some(first_sha.as_str()));
        assert_eq!(pushes[1].new_sha, second_sha);
    }

    #[test]
    fn push_failure_is_swallowed_and_does_not_advance_expected_old() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        let first_sha = writer.write(&sample_metadata(AgentStatus::Running)).unwrap();

        writer.git.queue_push_result(Err(anyhow::anyhow!("network down")));
        // Must not propagate the error to the caller.
        writer.push("sess-1", "origin").unwrap();

        // Subsequent successful push (after another write) still targets ZERO_SHA
        // because the prior attempt failed and didn't advance last_pushed; the
        // new chain head transitively covers the unsuccessful prior head.
        let second_sha = writer.write(&sample_metadata(AgentStatus::Crashed)).unwrap();
        writer.push("sess-1", "origin").unwrap();

        let pushes = writer.git.push_lease_calls.lock().unwrap();
        assert_eq!(pushes.len(), 2);
        assert_eq!(pushes[0].expected_old.as_deref(), Some(ZERO_SHA));
        assert_eq!(pushes[0].new_sha, first_sha);
        assert_eq!(pushes[1].expected_old.as_deref(), Some(ZERO_SHA));
        assert_eq!(pushes[1].new_sha, second_sha);
    }

    #[test]
    fn fetch_all_calls_fetch_refs_with_agents_glob() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        writer.fetch_all("origin").unwrap();

        let calls = writer.git.fetch_refs_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].remote, "origin");
        assert_eq!(calls[0].pattern, "refs/lazyspec/agents/*");
    }

    #[test]
    fn fetch_all_swallows_missing_remote_ref_error() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        writer.git.queue_fetch_result(Err(anyhow::anyhow!(
            "fatal: couldn't find remote ref refs/lazyspec/agents/sess-1"
        )));
        // Must succeed despite the underlying fetch error matching the
        // "first-ever fetch" signature handled by fetch_ref_optional.
        writer.fetch_all("origin").unwrap();
    }

    #[test]
    fn fetch_all_propagates_real_network_errors() {
        let writer = GitRefAgentMetadata::new(dummy_root(), ChainFake::default());
        writer
            .git
            .queue_fetch_result(Err(anyhow::anyhow!("network timeout")));
        let err = writer.fetch_all("origin").unwrap_err();
        assert!(err.to_string().contains("network timeout"));
    }

    // --- round-trip via a shared "remote store" fake ---

    /// Fake that simulates a coordination remote shared between two clones.
    /// `push_ref_with_lease` writes (sha, blob) into the remote map; `fetch_refs`
    /// copies all matching entries from the remote map into the clone's local
    /// state. Each clone keeps its own local refs/blobs.
    #[derive(Default, Clone)]
    struct RemoteStore {
        // refname -> (sha, metadata.json blob)
        entries: std::sync::Arc<Mutex<HashMap<String, (String, String)>>>,
    }

    struct SharedRemoteGit {
        // This clone's local ref state: refname -> sha.
        local_refs: Mutex<HashMap<String, String>>,
        // This clone's local blob store: sha -> metadata.json content.
        local_blobs: Mutex<HashMap<String, String>>,
        // Counter for synthetic shas (per clone).
        next_sha: Mutex<u32>,
        // Shared remote.
        remote: RemoteStore,
        // Stable identifier baked into generated shas so two clones produce
        // distinguishable shas (mirrors real git: same content, different
        // commit metadata → different sha).
        clone_id: String,
    }

    impl SharedRemoteGit {
        fn new(clone_id: &str, remote: RemoteStore) -> Self {
            Self {
                local_refs: Mutex::new(HashMap::new()),
                local_blobs: Mutex::new(HashMap::new()),
                next_sha: Mutex::new(0),
                remote,
                clone_id: clone_id.to_string(),
            }
        }
    }

    impl GitRefOps for SharedRemoteGit {
        fn resolve_ref(&self, _root: &Path, refname: &str) -> Result<Option<String>> {
            Ok(self.local_refs.lock().unwrap().get(refname).cloned())
        }
        fn list_refs(&self, _root: &Path, _pattern: &str) -> Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn read_ref_blob(&self, _root: &Path, sha: &str, _path: &str) -> Result<String> {
            self.local_blobs
                .lock()
                .unwrap()
                .get(sha)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no blob for sha {}", sha))
        }
        fn create_commit(
            &self,
            _root: &Path,
            _refname: &str,
            files: &[(&str, &str)],
            _parent: Option<&str>,
        ) -> Result<String> {
            let mut n = self.next_sha.lock().unwrap();
            *n += 1;
            let new_sha = format!("{}-sha-{}", self.clone_id, *n);
            if let Some((_, content)) = files.iter().find(|(p, _)| *p == "metadata.json") {
                self.local_blobs
                    .lock()
                    .unwrap()
                    .insert(new_sha.clone(), content.to_string());
            }
            Ok(new_sha)
        }
        fn create_ref_commit(
            &self,
            _root: &Path,
            _refname: &str,
            _files: &[(&str, &str)],
        ) -> Result<String> {
            bail!("unused")
        }
        fn update_ref(
            &self,
            _root: &Path,
            refname: &str,
            new_sha: &str,
            _old_sha: &str,
        ) -> Result<()> {
            self.local_refs
                .lock()
                .unwrap()
                .insert(refname.to_string(), new_sha.to_string());
            Ok(())
        }
        fn delete_ref(&self, _root: &Path, _refname: &str) -> Result<()> {
            Ok(())
        }
        fn fetch_refs(&self, _root: &Path, _remote: &str, pattern: &str) -> Result<()> {
            // Copy every remote entry matching the prefix (strip trailing '*')
            // into local refs and blobs.
            let prefix = pattern.trim_end_matches('*');
            let entries = self.remote.entries.lock().unwrap();
            let mut local_refs = self.local_refs.lock().unwrap();
            let mut local_blobs = self.local_blobs.lock().unwrap();
            for (refname, (sha, blob)) in entries.iter() {
                if refname.starts_with(prefix) {
                    local_refs.insert(refname.clone(), sha.clone());
                    local_blobs.insert(sha.clone(), blob.clone());
                }
            }
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
            refname: &str,
            new_sha: &str,
            _expected_old: Option<&str>,
        ) -> Result<()> {
            let blob = self
                .local_blobs
                .lock()
                .unwrap()
                .get(new_sha)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("push: no local blob for sha {}", new_sha))?;
            self.remote
                .entries
                .lock()
                .unwrap()
                .insert(refname.to_string(), (new_sha.to_string(), blob));
            Ok(())
        }
        fn read_commit_timestamp(&self, _root: &Path, _sha: &str) -> Result<DateTime<Utc>> {
            bail!("unused")
        }
    }

    #[test]
    fn cross_machine_round_trip_writer_a_to_reader_b() {
        // AC5: clone A writes + pushes, clone B fetches, B reads the same
        // metadata. Single shared remote store; two independent local stores.
        let remote = RemoteStore::default();
        let writer_a =
            GitRefAgentMetadata::new(dummy_root(), SharedRemoteGit::new("A", remote.clone()));
        let reader_b_git = SharedRemoteGit::new("B", remote.clone());

        let metadata = sample_metadata(AgentStatus::Running);
        writer_a.write(&metadata).unwrap();
        writer_a.push(&metadata.session_id, "origin").unwrap();

        // Before fetch, B sees nothing.
        let before = read_agent_metadata(&reader_b_git, &dummy_root(), &metadata.session_id)
            .unwrap();
        assert!(
            before.is_none(),
            "reader B should not see metadata before fetch"
        );

        // B fetches via the same code path the tick loop uses.
        let reader_b = GitRefAgentMetadata::new(dummy_root(), reader_b_git);
        reader_b.fetch_all("origin").unwrap();

        let got = read_agent_metadata(&reader_b.git, &dummy_root(), &metadata.session_id)
            .unwrap()
            .expect("reader B should see metadata after fetch");
        assert_eq!(got, metadata);
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
