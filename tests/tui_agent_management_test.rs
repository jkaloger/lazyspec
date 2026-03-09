#![cfg(feature = "agent")]
mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::agent::{
    load_all_records, save_record, update_record_status, AgentRecord, AgentStatus,
};
use lazyspec::tui::app::{App, ViewMode};
use std::path::PathBuf;
use tempfile::TempDir;

fn press(app: &mut App, fixture: &TestFixture, key: KeyCode) {
    app.handle_key(key, KeyModifiers::NONE, fixture.root(), &fixture.config());
}

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

/// Put app into Agents mode and set up controlled records.
/// We set view_mode directly to avoid cycle_mode() reloading from disk,
/// then clear and populate records as needed.
fn setup_agents_mode(fixture: &TestFixture, records: Vec<AgentRecord>) -> App {
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());
    app.view_mode = ViewMode::Agents;
    app.agent_spawner.records = records;
    app.agent_selected_index = 0;
    app
}

// 1. Cycle through all modes with backtick, assert Agents appears after Graph
#[test]
fn test_agents_view_mode_in_cycle() {
    assert_eq!(ViewMode::Types.next(), ViewMode::Filters);
    assert_eq!(ViewMode::Filters.next(), ViewMode::Metrics);
    assert_eq!(ViewMode::Metrics.next(), ViewMode::Graph);
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

// 4. Empty state: app in Agents mode has no records
#[test]
fn test_agents_screen_empty_state() {
    let fixture = TestFixture::new();
    let app = setup_agents_mode(&fixture, vec![]);

    assert_eq!(app.view_mode, ViewMode::Agents);
    assert!(app.agent_spawner.records.is_empty());
}

// 5. j/k navigation updates agent_selected_index
#[test]
fn test_agents_screen_navigation() {
    let fixture = TestFixture::new();
    let records = vec![
        sample_record("s1", "Doc A", "docs/rfcs/a.md"),
        sample_record("s2", "Doc B", "docs/rfcs/b.md"),
        sample_record("s3", "Doc C", "docs/rfcs/c.md"),
    ];
    let mut app = setup_agents_mode(&fixture, records);

    assert_eq!(app.agent_selected_index, 0);

    press(&mut app, &fixture, KeyCode::Char('j'));
    assert_eq!(app.agent_selected_index, 1);

    press(&mut app, &fixture, KeyCode::Char('j'));
    assert_eq!(app.agent_selected_index, 2);

    // Should not go past last record
    press(&mut app, &fixture, KeyCode::Char('j'));
    assert_eq!(app.agent_selected_index, 2);

    press(&mut app, &fixture, KeyCode::Char('k'));
    assert_eq!(app.agent_selected_index, 1);

    press(&mut app, &fixture, KeyCode::Char('k'));
    assert_eq!(app.agent_selected_index, 0);

    // Should not go below 0
    press(&mut app, &fixture, KeyCode::Char('k'));
    assert_eq!(app.agent_selected_index, 0);
}

// 6. r key sets resume_request to the selected record's session_id (only for non-running agents)
#[test]
fn test_agents_screen_r_key_sets_resume() {
    let fixture = TestFixture::new();
    let mut record = sample_record("sess-resume", "Resume Doc", "docs/rfcs/r.md");
    record.status = AgentStatus::Complete;
    let mut app = setup_agents_mode(&fixture, vec![record]);

    press(&mut app, &fixture, KeyCode::Char('r'));
    assert_eq!(app.resume_request, Some("sess-resume".to_string()));
}

// 6b. r key does NOT resume a running agent
#[test]
fn test_agents_screen_r_key_blocked_while_running() {
    let fixture = TestFixture::new();
    let records = vec![sample_record("sess-running", "Running Doc", "docs/rfcs/r.md")];
    let mut app = setup_agents_mode(&fixture, records);

    press(&mut app, &fixture, KeyCode::Char('r'));
    assert_eq!(app.resume_request, None);
}

// 7. e key sets editor_request containing the doc's path
#[test]
fn test_agents_screen_e_key_opens_doc() {
    let fixture = TestFixture::new();
    let records = vec![sample_record("sess-edit", "Edit Doc", "docs/rfcs/e.md")];
    let mut app = setup_agents_mode(&fixture, records);

    press(&mut app, &fixture, KeyCode::Char('e'));
    assert!(app.editor_request.is_some());
    let path = app.editor_request.unwrap();
    assert!(
        path.ends_with("docs/rfcs/e.md"),
        "editor_request should contain the doc path, got: {:?}",
        path
    );
}
