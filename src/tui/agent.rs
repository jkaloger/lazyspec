use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::Result;
use serde::{Deserialize, Serialize};

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
    let dir = match override_path {
        Some(p) => p.to_path_buf(),
        None => dirs_home().join(".lazyspec").join("agents"),
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

// --- Prompt builders ---

pub fn build_create_children_prompt(doc_content: &str, child_type: &str) -> String {
    format!(
        "You are a specification document generator. Given the parent document below, generate \
child documents of type \"{child_type}\" that break down the parent into actionable pieces. \
For each child document, run `lazyspec create {child_type}` with an appropriate title \
and fill in the generated file with relevant content derived from the parent. \
Preserve traceability by including a relation back to the parent document.\n\n\
---\n\n{doc_content}"
    )
}

pub fn build_expand_prompt(doc_content: &str) -> String {
    format!(
        "You are editing a specification document. Your task is to flesh out and expand any sparse \
or incomplete sections while preserving the YAML frontmatter exactly as-is. Do not remove \
or reorder existing content. Focus on adding detail, clarifying intent, and filling gaps. \
Output the complete updated document.\n\n---\n\n{}",
        doc_content
    )
}

pub struct AgentSpawner {
    running: Vec<(String, Child)>,
    pub records: Vec<AgentRecord>,
}

impl AgentSpawner {
    pub fn new() -> Self {
        let records = load_all_records(None).unwrap_or_default();
        AgentSpawner {
            running: Vec::new(),
            records,
        }
    }

    pub fn spawn(
        &mut self,
        prompt: &str,
        doc_path: &Path,
        doc_title: &str,
        action: &str,
    ) -> Result<()> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let child = Command::new("claude")
            .args(["-p", prompt])
            .arg(doc_path)
            .args(["--session-id", &session_id])
            .args(["--allowedTools", "Read,Edit,Bash(lazyspec *)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let record = AgentRecord {
            session_id: session_id.clone(),
            doc_title: doc_title.to_string(),
            doc_path: doc_path.to_path_buf(),
            action: action.to_string(),
            status: AgentStatus::Running,
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
        };

        let _ = save_record(&record, None);
        self.records.push(record);
        self.running.push((session_id, child));
        Ok(())
    }

    pub fn poll_finished(&mut self) {
        let mut finished = Vec::new();

        self.running.retain_mut(|(session_id, child)| {
            match child.try_wait() {
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
            }
        });

        let now = chrono::Utc::now().to_rfc3339();
        for (session_id, status) in finished {
            let _ = update_record_status(&session_id, status.clone(), None);
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
}
