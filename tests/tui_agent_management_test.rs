#![cfg(feature = "agent")]
mod common;

use lazyspec::tui::agent::{
    load_all_records, save_record, update_record_status, AgentRecord, AgentStatus,
};
use lazyspec::tui::state::ViewMode;
use std::path::PathBuf;
use tempfile::TempDir;

fn sample_record(session_id: &str, title: &str, doc_path: &str) -> AgentRecord {
    AgentRecord {
        session_id: session_id.to_string(),
        doc_title: title.to_string(),
        doc_path: PathBuf::from(doc_path),
        action: "Expand document".to_string(),
        status: AgentStatus::Running,
        started_at: "2026-03-09T10:00:00Z".to_string(),
        finished_at: None,
    }
}

// 1. Cycle through all modes with backtick, assert Agents appears after Graph
#[test]
fn test_agents_view_mode_in_cycle() {
    assert_eq!(ViewMode::Types.next(), ViewMode::Filters);
    #[cfg(feature = "metrics")]
    {
        assert_eq!(ViewMode::Filters.next(), ViewMode::Metrics);
        assert_eq!(ViewMode::Metrics.next(), ViewMode::Graph);
    }
    #[cfg(not(feature = "metrics"))]
    assert_eq!(ViewMode::Filters.next(), ViewMode::Graph);
    assert_eq!(ViewMode::Graph.next(), ViewMode::Agents);
    assert_eq!(ViewMode::Agents.next(), ViewMode::Types);
}

// 2. AgentRecord round-trips through save/load with override_path
#[test]
fn test_agent_record_persistence() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let record = sample_record("sess-001", "My RFC", "docs/rfcs/001.md");
    save_record(&record, Some(dir)).unwrap();

    let loaded = load_all_records(Some(dir)).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].session_id, "sess-001");
    assert_eq!(loaded[0].doc_title, "My RFC");
    assert_eq!(loaded[0].doc_path, PathBuf::from("docs/rfcs/001.md"));
    assert_eq!(loaded[0].action, "Expand document");
    assert_eq!(loaded[0].status, AgentStatus::Running);
    assert_eq!(loaded[0].finished_at, None);
}

// 3. Save Running, update to Complete, reload, assert status and finished_at
#[test]
fn test_agent_record_status_update() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let record = sample_record("sess-update", "Update Test", "docs/rfcs/002.md");
    save_record(&record, Some(dir)).unwrap();

    update_record_status("sess-update", AgentStatus::Complete, Some(dir)).unwrap();

    let loaded = load_all_records(Some(dir)).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].status, AgentStatus::Complete);
    assert!(loaded[0].finished_at.is_some());
}
