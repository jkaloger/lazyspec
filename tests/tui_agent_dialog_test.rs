mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::app::App;

fn press(app: &mut App, fixture: &TestFixture, key: KeyCode) {
    app.handle_key(key, KeyModifiers::NONE, fixture.root(), &fixture.config());
}

// AC1: selecting a doc and pressing `a` opens the agent dialog with actions
#[test]
fn test_a_key_opens_dialog() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());

    app.selected_type = 0;
    app.selected_doc = 0;
    press(&mut app, &fixture, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    assert!(!app.agent_dialog.actions.is_empty());
}

// AC8: pressing `a` on an empty doc list does nothing
#[test]
fn test_a_key_empty_list() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());

    app.selected_type = 0;
    press(&mut app, &fixture, KeyCode::Char('a'));

    assert!(!app.agent_dialog.active);
}

// AC7: pressing Esc closes the dialog without spawning an agent
#[test]
fn test_esc_closes_dialog() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());

    app.selected_type = 0;
    app.selected_doc = 0;
    press(&mut app, &fixture, KeyCode::Char('a'));
    assert!(app.agent_dialog.active);

    press(&mut app, &fixture, KeyCode::Esc);
    assert!(!app.agent_dialog.active);
}

// AC9: unhandled keys are ignored while dialog is open
#[test]
fn test_unhandled_key_ignored() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());

    app.selected_type = 0;
    app.selected_doc = 0;
    press(&mut app, &fixture, KeyCode::Char('a'));
    assert!(app.agent_dialog.active);

    let index_before = app.agent_dialog.selected_index;
    let actions_before = app.agent_dialog.actions.clone();

    press(&mut app, &fixture, KeyCode::Char('x'));

    assert!(app.agent_dialog.active);
    assert_eq!(app.agent_dialog.selected_index, index_before);
    assert_eq!(app.agent_dialog.actions, actions_before);
}

// AC4: iteration docs have no child types, so "Create children" should not appear
#[test]
fn test_no_create_children_for_iteration() {
    let fixture = TestFixture::new();
    fixture.write_iteration("ITER-001-test.md", "Test Iteration", "draft", None);
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());

    // Find the iteration type index
    let iter_idx = app
        .doc_types
        .iter()
        .position(|dt| dt.to_string() == "iteration")
        .expect("iteration type should exist");
    app.selected_type = iter_idx;
    app.build_doc_tree();
    app.selected_doc = 0;

    press(&mut app, &fixture, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    assert!(
        !app.agent_dialog.actions.iter().any(|a| a == "Create children"),
        "iteration should not have 'Create children' action"
    );
}

// AC4: RFC docs have child types (stories), so "Create children" should appear
#[test]
fn test_create_children_for_rfc() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());

    app.selected_type = 0;
    app.selected_doc = 0;
    press(&mut app, &fixture, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    assert!(
        app.agent_dialog.actions.iter().any(|a| a == "Create children"),
        "RFC should have 'Create children' action"
    );
}
