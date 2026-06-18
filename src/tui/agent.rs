use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::engine::agent::{AgentContext, AgentRunner, ClaudeP};

// --- Agent record model and persistence ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub session_id: String,
    pub doc_title: String,
    pub doc_path: PathBuf,
    pub action: String,
    pub status: AgentStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
}

pub fn agent_history_dir(override_path: Option<&Path>) -> PathBuf {
    // The spawner always passes an explicit dir (its `history_dir`, under the
    // repo's `.lazyspec/cache/agents/`). The `None` fallback exists only for
    // callers without a repo root in scope.
    let dir = match override_path {
        Some(p) => p.to_path_buf(),
        None => dirs_home().join(".lazyspec").join("cache").join("agents"),
    };
    let _ = fs::create_dir_all(&dir);
    dir
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn save_record(record: &AgentRecord, override_path: Option<&Path>) -> Result<()> {
    let dir = agent_history_dir(override_path);
    let file_path = dir.join(format!("{}.json", record.session_id));
    let json = serde_json::to_string_pretty(record)?;
    fs::write(file_path, json)?;
    Ok(())
}

pub fn load_all_records(override_path: Option<&Path>) -> Result<Vec<AgentRecord>> {
    let dir = agent_history_dir(override_path);
    let mut records = Vec::new();

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(records),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        match serde_json::from_str::<AgentRecord>(&content) {
            Ok(record) => records.push(record),
            Err(_) => continue,
        }
    }

    records.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(records)
}

pub fn update_record_status(
    session_id: &str,
    status: AgentStatus,
    override_path: Option<&Path>,
) -> Result<()> {
    let dir = agent_history_dir(override_path);
    let file_path = dir.join(format!("{session_id}.json"));
    let content = fs::read_to_string(&file_path)?;
    let mut record: AgentRecord = serde_json::from_str(&content)?;
    record.status = status;
    if record.status == AgentStatus::Complete || record.status == AgentStatus::Failed {
        record.finished_at = Some(chrono::Utc::now().to_rfc3339());
    }
    let json = serde_json::to_string_pretty(&record)?;
    fs::write(file_path, json)?;
    Ok(())
}

pub struct AgentSpawner {
    running: Vec<(String, Child)>,
    pub records: Vec<AgentRecord>,
    runner: Arc<dyn AgentRunner>,
    history_dir: PathBuf,
}

impl AgentSpawner {
    pub fn new(root: &Path) -> Self {
        Self::with_runner(Arc::new(ClaudeP), root)
    }

    pub fn with_runner(runner: Arc<dyn AgentRunner>, root: &Path) -> Self {
        let history_dir = root.join(".lazyspec").join("cache").join("agents");
        let records = load_all_records(Some(&history_dir)).unwrap_or_default();
        AgentSpawner {
            running: Vec::new(),
            records,
            runner,
            history_dir,
        }
    }

    pub fn history_dir(&self) -> &Path {
        &self.history_dir
    }

    pub fn spawn(
        &mut self,
        prompt: &str,
        allowed_tools: Option<&str>,
        doc_path: &Path,
        doc_title: &str,
        action: &str,
    ) -> Result<()> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let ctx = AgentContext {
            prompt: prompt.to_string(),
            allowed_tools: allowed_tools.map(|t| t.to_string()),
            doc_path: doc_path.to_path_buf(),
            session_id: session_id.clone(),
        };
        let handle = self.runner.spawn(ctx)?;

        let record = AgentRecord {
            session_id: session_id.clone(),
            doc_title: doc_title.to_string(),
            doc_path: doc_path.to_path_buf(),
            action: action.to_string(),
            status: AgentStatus::Running,
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
        };

        let _ = save_record(&record, Some(&self.history_dir));
        self.records.push(record);
        self.running.push((handle.session_id, handle.child));
        Ok(())
    }

    pub fn poll_finished(&mut self) {
        let mut finished = Vec::new();

        self.running
            .retain_mut(|(session_id, child)| match child.try_wait() {
                Ok(Some(exit_status)) => {
                    let status = if exit_status.success() {
                        AgentStatus::Complete
                    } else {
                        AgentStatus::Failed
                    };
                    finished.push((session_id.clone(), status));
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    finished.push((session_id.clone(), AgentStatus::Failed));
                    false
                }
            });

        let now = chrono::Utc::now().to_rfc3339();
        for (session_id, status) in finished {
            let _ = update_record_status(&session_id, status.clone(), Some(&self.history_dir));
            if let Some(rec) = self.records.iter_mut().find(|r| r.session_id == session_id) {
                rec.status = status;
                rec.finished_at = Some(now.clone());
            }
        }
    }

    pub fn active_count(&self) -> usize {
        self.running.len()
    }
}

#[cfg(test)]
mod tests {
    // The test fakes use `RefCell`/single-threaded interior mutability and so are
    // not `Sync`, but the spawner's seam is `Arc<dyn AgentRunner>`. These fakes
    // are only ever touched on the test thread, so wrapping them in `Arc` to
    // match that type is safe; the lint targets cross-thread misuse.
    #![allow(clippy::arc_with_non_send_sync)]

    use super::*;
    use tempfile::TempDir;

    fn sample_record(session_id: &str, started_at: &str) -> AgentRecord {
        AgentRecord {
            session_id: session_id.to_string(),
            doc_title: "Test Doc".to_string(),
            doc_path: PathBuf::from("/tmp/test.md"),
            action: "Expand document".to_string(),
            status: AgentStatus::Running,
            started_at: started_at.to_string(),
            finished_at: None,
        }
    }

    #[test]
    fn agent_record_roundtrip_serialize() {
        let record = sample_record("abc-123", "2026-03-09T10:00:00Z");
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: AgentRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.session_id, "abc-123");
        assert_eq!(deserialized.status, AgentStatus::Running);
        assert_eq!(deserialized.finished_at, None);
    }

    #[test]
    fn agent_record_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let r1 = sample_record("id-1", "2026-03-09T10:00:00Z");
        let r2 = sample_record("id-2", "2026-03-09T11:00:00Z");

        save_record(&r1, Some(dir)).unwrap();
        save_record(&r2, Some(dir)).unwrap();

        let loaded = load_all_records(Some(dir)).unwrap();
        assert_eq!(loaded.len(), 2);
        // Descending by started_at
        assert_eq!(loaded[0].session_id, "id-2");
        assert_eq!(loaded[1].session_id, "id-1");
    }

    #[test]
    fn agent_record_update_status() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let record = sample_record("id-update", "2026-03-09T10:00:00Z");
        save_record(&record, Some(dir)).unwrap();

        update_record_status("id-update", AgentStatus::Complete, Some(dir)).unwrap();

        let loaded = load_all_records(Some(dir)).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].status, AgentStatus::Complete);
        assert!(loaded[0].finished_at.is_some());
    }

    #[test]
    fn agent_record_load_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let records = load_all_records(Some(tmp.path())).unwrap();
        assert!(records.is_empty());
    }

    // --- Spawner delegation, persistence, and polling via the AgentRunner seam ---

    use crate::engine::agent::{AgentContext, AgentHandle, AgentRunner, FakeRunner};
    use std::process::{Command, Stdio};

    /// A test runner that returns handles wrapping a platform `false` child, so
    /// `try_wait()` reports a non-zero exit. `FakeRunner` only ever wraps `true`
    /// (or errors), so it cannot exercise the Failed-via-exit path AC6 needs.
    struct FailingChildRunner {
        captured: std::cell::RefCell<Vec<AgentContext>>,
    }

    impl FailingChildRunner {
        fn new() -> Self {
            FailingChildRunner {
                captured: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl AgentRunner for FailingChildRunner {
        fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
            self.captured.borrow_mut().push(ctx.clone());
            let child = Command::new("false")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            Ok(AgentHandle {
                session_id: ctx.session_id,
                child,
            })
        }
    }

    fn wait_for_drain(spawner: &mut AgentSpawner) {
        for _ in 0..10_000 {
            spawner.poll_finished();
            if spawner.active_count() == 0 {
                return;
            }
            // Yield (not sleep) so the short-lived `true`/`false` children get
            // scheduled and exit; keeps the test deterministic, not timed.
            std::thread::yield_now();
        }
        panic!("children did not exit within the poll budget");
    }

    // AC1: the spawner builds an AgentContext and obtains a handle from the runner.
    #[test]
    fn spawner_builds_context_and_delegates_to_runner() {
        let tmp = TempDir::new().unwrap();
        let fake = Arc::new(FakeRunner::new());
        let mut spawner = AgentSpawner::with_runner(fake.clone(), tmp.path());

        let doc_path = Path::new("docs/rfcs/RFC-001.md");
        spawner
            .spawn("expand it", None, doc_path, "RFC One", "Expand document")
            .unwrap();

        let captured = fake.captured.borrow();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].prompt, "expand it");
        assert_eq!(captured[0].doc_path, doc_path);
        assert!(!captured[0].session_id.is_empty());
    }

    // AC5: the spawner -- not the runner -- creates, persists, and tracks the record.
    #[test]
    fn spawner_creates_and_persists_record_with_fake_runner() {
        let tmp = TempDir::new().unwrap();
        let fake = Arc::new(FakeRunner::new());
        let mut spawner = AgentSpawner::with_runner(fake, tmp.path());

        let doc_path = Path::new("docs/rfcs/RFC-001.md");
        spawner
            .spawn("p", None, doc_path, "RFC One", "Expand document")
            .unwrap();

        assert_eq!(spawner.records.len(), 1);
        let rec = &spawner.records[0];
        assert_eq!(rec.status, AgentStatus::Running);
        assert_eq!(rec.doc_title, "RFC One");
        assert_eq!(rec.action, "Expand document");
        assert_eq!(rec.doc_path, doc_path);
        assert_eq!(spawner.active_count(), 1);

        let history = tmp.path().join(".lazyspec").join("cache").join("agents");
        let persisted = load_all_records(Some(&history)).unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].session_id, rec.session_id);
        assert!(history.join(format!("{}.json", rec.session_id)).exists());
    }

    // AC6: polling marks the success child Complete and the failure child Failed,
    // updates and persists both, and clears the active count.
    #[test]
    fn poll_marks_complete_and_failed_via_fake_handles() {
        let tmp = TempDir::new().unwrap();
        let history = tmp.path().join(".lazyspec").join("cache").join("agents");

        // Success: FakeRunner wraps a platform `true` child.
        let ok = Arc::new(FakeRunner::new());
        let mut spawner = AgentSpawner::with_runner(ok, tmp.path());
        spawner
            .spawn("ok", None, Path::new("a.md"), "A", "Expand document")
            .unwrap();
        let ok_id = spawner.records[0].session_id.clone();

        // Failure: a runner wrapping a platform `false` child.
        let fail = Arc::new(FailingChildRunner::new());
        spawner.runner = fail;
        spawner
            .spawn("fail", None, Path::new("b.md"), "B", "Expand document")
            .unwrap();
        let fail_id = spawner.records[1].session_id.clone();

        assert_eq!(spawner.active_count(), 2);
        wait_for_drain(&mut spawner);
        assert_eq!(spawner.active_count(), 0);

        let ok_rec = spawner
            .records
            .iter()
            .find(|r| r.session_id == ok_id)
            .unwrap();
        assert_eq!(ok_rec.status, AgentStatus::Complete);
        assert!(ok_rec.finished_at.is_some());

        let fail_rec = spawner
            .records
            .iter()
            .find(|r| r.session_id == fail_id)
            .unwrap();
        assert_eq!(fail_rec.status, AgentStatus::Failed);

        let persisted = load_all_records(Some(&history)).unwrap();
        let ok_p = persisted.iter().find(|r| r.session_id == ok_id).unwrap();
        let fail_p = persisted.iter().find(|r| r.session_id == fail_id).unwrap();
        assert_eq!(ok_p.status, AgentStatus::Complete);
        assert!(ok_p.finished_at.is_some());
        assert_eq!(fail_p.status, AgentStatus::Failed);
        assert!(fail_p.finished_at.is_some());
    }

    // Per-call tool forwarding: the caller's allowed_tools are threaded verbatim
    // into the AgentContext (no longer a hardcoded list); None forwards as None.
    #[test]
    fn spawn_forwards_per_call_allowed_tools() {
        let tmp = TempDir::new().unwrap();
        let fake = Arc::new(FakeRunner::new());
        let mut spawner = AgentSpawner::with_runner(fake.clone(), tmp.path());

        let full_path = tmp.path().join("docs/rfcs/RFC-001.md");
        spawner
            .spawn(
                "expand prompt",
                Some("Read,Edit"),
                &full_path,
                "RFC One",
                "refine",
            )
            .unwrap();
        spawner
            .spawn("no tools", None, &full_path, "RFC One", "refine")
            .unwrap();

        let captured = fake.captured.borrow();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].allowed_tools, Some("Read,Edit".to_string()));
        assert_eq!(captured[0].prompt, "expand prompt");
        assert_eq!(captured[0].doc_path, full_path);
        // session_id is a v4 uuid (round-trips through the uuid parser).
        assert!(uuid::Uuid::parse_str(&captured[0].session_id).is_ok());
        assert_eq!(captured[1].allowed_tools, None);
    }
}
