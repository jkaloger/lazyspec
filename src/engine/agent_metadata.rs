use anyhow::Result;
use std::path::PathBuf;

use super::git_ref::GitRefOps;

/// Writes agent session metadata under `refs/lazyspec/agents/{session-id}`.
///
/// Iter C (slice 4) only needs `mark_crashed` for boot orphan recovery.
/// Slice 8 (STORY-124) replaces the body with the full `AgentMetadata`
/// schema; the ref name + `crashed` marker survive that transition.
pub trait AgentMetadataWriter: Send + Sync {
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
    fn mark_crashed(&self, session_id: &str) -> Result<()> {
        // Iter C v1: orphan commit overwrites any existing ref. Slice 8 will
        // chain commits onto the full schema; for now we only care that the
        // ref points at a tree containing `status.txt = "crashed"`.
        let refname = agent_ref(session_id);
        self.git
            .create_ref_commit(&self.root, &refname, &[("status.txt", "crashed")])?;
        Ok(())
    }
}

pub struct NullAgentMetadata;

impl AgentMetadataWriter for NullAgentMetadata {
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

    #[derive(Default)]
    struct RecordingGit {
        calls: Mutex<Vec<RecordedCall>>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RecordedCall {
        refname: String,
        files: Vec<(String, String)>,
    }

    impl GitRefOps for RecordingGit {
        fn resolve_ref(&self, _root: &Path, _refname: &str) -> Result<Option<String>> {
            Ok(None)
        }
        fn list_refs(&self, _root: &Path, _pattern: &str) -> Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn read_ref_blob(&self, _root: &Path, _sha: &str, _path: &str) -> Result<String> {
            Ok(String::new())
        }
        fn create_commit(
            &self,
            _root: &Path,
            _refname: &str,
            _files: &[(&str, &str)],
            _parent: Option<&str>,
        ) -> Result<String> {
            bail!("not implemented for this test")
        }
        fn create_ref_commit(
            &self,
            _root: &Path,
            refname: &str,
            files: &[(&str, &str)],
        ) -> Result<String> {
            self.calls.lock().unwrap().push(RecordedCall {
                refname: refname.to_string(),
                files: files
                    .iter()
                    .map(|(p, c)| (p.to_string(), c.to_string()))
                    .collect(),
            });
            Ok("commit-sha".to_string())
        }
        fn update_ref(
            &self,
            _root: &Path,
            _refname: &str,
            _new_sha: &str,
            _old_sha: &str,
        ) -> Result<()> {
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

    #[test]
    fn null_metadata_mark_crashed_is_noop() {
        let writer = NullAgentMetadata;
        writer.mark_crashed("sess-1").unwrap();
    }

    #[test]
    fn git_ref_metadata_writes_status_blob() {
        let writer = GitRefAgentMetadata::new(dummy_root(), RecordingGit::default());

        writer.mark_crashed("sess-1").unwrap();

        let calls = writer.git.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].refname, "refs/lazyspec/agents/sess-1");
        assert_eq!(
            calls[0].files,
            vec![("status.txt".to_string(), "crashed".to_string())]
        );
    }

    #[test]
    fn git_ref_metadata_idempotent_re_mark() {
        let writer = GitRefAgentMetadata::new(dummy_root(), RecordingGit::default());

        writer.mark_crashed("sess-1").unwrap();
        writer.mark_crashed("sess-1").unwrap();

        let calls = writer.git.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].refname, "refs/lazyspec/agents/sess-1");
        assert_eq!(calls[1].refname, "refs/lazyspec/agents/sess-1");
    }
}
