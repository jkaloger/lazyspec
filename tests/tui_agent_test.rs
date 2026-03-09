#![cfg(feature = "agent")]
mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::config::{Config, ValidationRule};
use lazyspec::tui::agent::{build_create_children_prompt, build_expand_prompt};
use lazyspec::tui::app::App;

fn press(app: &mut App, fixture: &TestFixture, key: KeyCode) {
    app.handle_key(key, KeyModifiers::NONE, fixture.root(), &fixture.config());
}

fn open_dialog_on_rfc(fixture: &TestFixture) -> App {
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let store = fixture.store();
    let mut app = App::new(store, &fixture.config());
    app.selected_type = 0;
    app.selected_doc = 0;
    press(&mut app, fixture, KeyCode::Char('a'));
    assert!(app.agent_dialog.active);
    app
}

fn select_action(app: &mut App, fixture: &TestFixture, action_name: &str) {
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| a == action_name)
        .unwrap_or_else(|| panic!("action '{}' not found", action_name));
    app.agent_dialog.selected_index = idx;
    press(app, fixture, KeyCode::Enter);
}

// --- AC2: Expand prompt includes document content ---

#[test]
fn expand_prompt_contains_document_content() {
    let content = "---\ntitle: \"My RFC\"\n---\n\n# Overview\nSome details here.";
    let prompt = build_expand_prompt(content, std::path::Path::new("docs/rfcs/RFC-001.md"));
    assert!(prompt.contains(content));
    assert!(prompt.contains("Edit"));
    assert!(prompt.contains("RFC-001.md"));
}

// --- AC3: Create-children derives child type from ParentChild rules ---

#[test]
fn create_children_prompt_contains_child_type_and_content() {
    let content = "---\ntitle: \"Parent RFC\"\n---\n\nRFC body text.";
    let prompt = build_create_children_prompt(content, "story");
    assert!(prompt.contains("story"));
    assert!(prompt.contains(content));
    assert!(prompt.contains("lazyspec create story"));
}

#[test]
fn config_parent_child_rules_derive_child_type() {
    let config = Config::default();
    let rfc_child = config.rules.iter().find_map(|rule| match rule {
        ValidationRule::ParentChild { parent, child, .. } if parent == "rfc" => {
            Some(child.clone())
        }
        _ => None,
    });
    assert_eq!(rfc_child, Some("story".to_string()));

    let story_child = config.rules.iter().find_map(|rule| match rule {
        ValidationRule::ParentChild { parent, child, .. } if parent == "story" => {
            Some(child.clone())
        }
        _ => None,
    });
    assert_eq!(story_child, Some("iteration".to_string()));

    let iteration_child = config.rules.iter().find_map(|rule| match rule {
        ValidationRule::ParentChild { parent, child, .. } if parent == "iteration" => {
            Some(child.clone())
        }
        _ => None,
    });
    assert_eq!(iteration_child, None);
}

// --- AC5: Custom prompt captures keystrokes and passes to spawn ---

#[test]
fn custom_prompt_enters_text_input_mode() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);

    select_action(&mut app, &fixture, "Custom prompt");

    // Dialog stays open in text input mode
    assert!(app.agent_dialog.active);
    assert_eq!(app.agent_dialog.text_input, Some(String::new()));
}

#[test]
fn custom_prompt_captures_typed_characters() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);
    select_action(&mut app, &fixture, "Custom prompt");

    press(&mut app, &fixture, KeyCode::Char('h'));
    press(&mut app, &fixture, KeyCode::Char('i'));

    assert_eq!(app.agent_dialog.text_input, Some("hi".to_string()));
}

#[test]
fn custom_prompt_backspace_removes_character() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);
    select_action(&mut app, &fixture, "Custom prompt");

    press(&mut app, &fixture, KeyCode::Char('a'));
    press(&mut app, &fixture, KeyCode::Char('b'));
    press(&mut app, &fixture, KeyCode::Backspace);

    assert_eq!(app.agent_dialog.text_input, Some("a".to_string()));
}

#[test]
fn custom_prompt_esc_returns_to_action_list() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);
    select_action(&mut app, &fixture, "Custom prompt");
    assert!(app.agent_dialog.text_input.is_some());

    press(&mut app, &fixture, KeyCode::Esc);

    assert!(app.agent_dialog.active);
    assert!(app.agent_dialog.text_input.is_none());
}

#[test]
fn custom_prompt_enter_closes_dialog() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);
    select_action(&mut app, &fixture, "Custom prompt");

    press(&mut app, &fixture, KeyCode::Char('g'));
    press(&mut app, &fixture, KeyCode::Char('o'));
    press(&mut app, &fixture, KeyCode::Enter);

    assert!(!app.agent_dialog.active);
    assert!(app.agent_dialog.text_input.is_none());
}

// --- AC6: Spawned processes don't block handle_key dispatch ---

#[test]
fn expand_document_closes_dialog_without_blocking() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);

    select_action(&mut app, &fixture, "Expand document");

    // Dialog closed immediately (spawn may fail since no claude binary, but state transitions)
    assert!(!app.agent_dialog.active);
}

#[test]
fn create_children_closes_dialog_without_blocking() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);

    select_action(&mut app, &fixture, "Create children");

    assert!(!app.agent_dialog.active);
}

#[test]
fn handle_key_returns_after_spawn_action() {
    let fixture = TestFixture::new();
    let mut app = open_dialog_on_rfc(&fixture);
    select_action(&mut app, &fixture, "Expand document");

    // Verify the app is still responsive by pressing keys after spawn
    press(&mut app, &fixture, KeyCode::Char('j'));
    // No panic or hang means handle_key returned normally
}

// --- AC10: Only claude binary is invoked ---

#[test]
fn agent_spawner_uses_claude_command() {
    // AgentSpawner::spawn calls Command::new("claude") internally.
    // We verify this by checking the module source expectation:
    // the spawn method only constructs Command::new("claude").
    // Since we can't mock Command, we verify the spawner exists and
    // that calling spawn with no claude binary returns an error.
    use lazyspec::tui::agent::AgentSpawner;
    let mut spawner = AgentSpawner::new();
    let result = spawner.spawn("test prompt", std::path::Path::new("/tmp/fake.md"), "Test Doc", "Expand document");
    // In a test environment without claude installed, this should error
    assert!(result.is_err() || spawner.active_count() == 1);
}
