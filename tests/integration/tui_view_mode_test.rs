use crate::common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::tui::state::{App, PreviewTab, ViewMode};

fn setup_app_with_docs() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/001-first.md",
        "---\ntitle: First RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );
    fixture.write_doc(
        "docs/rfcs/002-second.md",
        "---\ntitle: Second RFC\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-02\ntags: []\n---\nBody\n",
    );

    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    (fixture, app)
}

#[test]
fn test_app_defaults_to_types_mode() {
    let (_fixture, app) = setup_app_with_docs();
    assert_eq!(app.view_mode, ViewMode::Types);
}

#[test]
fn test_view_mode_next_cycles() {
    assert_eq!(ViewMode::Types.next(), ViewMode::Filters);
    #[cfg(feature = "metrics")]
    {
        assert_eq!(ViewMode::Filters.next(), ViewMode::Metrics);
        assert_eq!(ViewMode::Metrics.next(), ViewMode::Graph);
    }
    #[cfg(not(feature = "metrics"))]
    assert_eq!(ViewMode::Filters.next(), ViewMode::Graph);
    assert_eq!(ViewMode::Graph.next(), ViewMode::Settings);
    #[cfg(feature = "agent")]
    {
        assert_eq!(ViewMode::Settings.next(), ViewMode::Agents);
        assert_eq!(ViewMode::Agents.next(), ViewMode::Types);
    }
    #[cfg(not(feature = "agent"))]
    assert_eq!(ViewMode::Settings.next(), ViewMode::Types);
}

#[test]
fn test_backtick_cycles_mode() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.view_mode, ViewMode::Types);

    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.view_mode, ViewMode::Filters);

    #[cfg(feature = "metrics")]
    {
        app.handle_key(
            KeyCode::Char('`'),
            KeyModifiers::NONE,
            fixture.root(),
            &fixture.config(),
        );
        assert_eq!(app.view_mode, ViewMode::Metrics);
    }
    #[cfg(not(feature = "metrics"))]
    {
        app.handle_key(
            KeyCode::Char('`'),
            KeyModifiers::NONE,
            fixture.root(),
            &fixture.config(),
        );
        assert_eq!(app.view_mode, ViewMode::Graph);
    }
}

#[test]
fn test_types_mode_navigation_unchanged() {
    let (fixture, mut app) = setup_app_with_docs();
    assert_eq!(app.view_mode, ViewMode::Types);

    // j moves selected doc down
    assert_eq!(app.selected_doc, 0);
    app.handle_key(
        KeyCode::Char('j'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.selected_doc, 1);
    assert_eq!(app.view_mode, ViewMode::Types);

    // k moves selected doc up
    app.handle_key(
        KeyCode::Char('k'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.selected_doc, 0);
    assert_eq!(app.view_mode, ViewMode::Types);

    // l switches type
    let before_type = app.selected_type;
    app.handle_key(
        KeyCode::Char('l'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_ne!(app.selected_type, before_type);
    assert_eq!(app.view_mode, ViewMode::Types);

    // h switches type back
    app.handle_key(
        KeyCode::Char('h'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.selected_type, before_type);
    assert_eq!(app.view_mode, ViewMode::Types);

    // Enter toggles fullscreen
    app.handle_key(
        KeyCode::Enter,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert!(app.fullscreen_doc);
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(
        KeyCode::Esc,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    // Tab toggles preview tab
    assert_eq!(app.preview_tab, PreviewTab::Preview);
    app.handle_key(
        KeyCode::Tab,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.preview_tab, PreviewTab::Relations);
    assert_eq!(app.view_mode, ViewMode::Types);
}

#[test]
fn test_backtick_ignored_in_modal_states() {
    let (fixture, mut app) = setup_app_with_docs();

    // Search mode
    app.enter_search();
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(
        KeyCode::Esc,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    // Fullscreen mode
    app.enter_fullscreen();
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(
        KeyCode::Esc,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    // Create form mode
    app.open_create_form();
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(
        KeyCode::Esc,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    // Delete confirm mode
    app.open_delete_confirm();
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.view_mode, ViewMode::Types);
    app.handle_key(
        KeyCode::Esc,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
}

#[test]
fn test_settings_left_right_moves_category() {
    let (fixture, mut app) = setup_app_with_docs();
    let key = |app: &mut App, c: KeyCode| {
        app.handle_key(c, KeyModifiers::NONE, fixture.root(), &fixture.config());
    };

    key(&mut app, KeyCode::Char('5'));
    assert_eq!(app.view_mode, ViewMode::Settings);
    assert_eq!(app.settings_category, 0);

    // l/Right walks across every category, including past the collection
    // categories (Document Types is index 1) that used to trap navigation.
    let last = App::settings_categories().len() - 1;
    for expected in 1..=last {
        key(&mut app, KeyCode::Char('l'));
        assert_eq!(app.settings_category, expected);
    }
    // No wrap past the end.
    key(&mut app, KeyCode::Char('l'));
    assert_eq!(app.settings_category, last);

    // h walks back to the first category; no wrap past the start.
    for expected in (0..last).rev() {
        key(&mut app, KeyCode::Char('h'));
        assert_eq!(app.settings_category, expected);
    }
    key(&mut app, KeyCode::Char('h'));
    assert_eq!(app.settings_category, 0);
}

#[test]
fn test_settings_arrow_keys_move_category() {
    let (fixture, mut app) = setup_app_with_docs();
    let key = |app: &mut App, c: KeyCode| {
        app.handle_key(c, KeyModifiers::NONE, fixture.root(), &fixture.config());
    };

    key(&mut app, KeyCode::Char('5'));
    key(&mut app, KeyCode::Right);
    assert_eq!(app.settings_category, 1);
    key(&mut app, KeyCode::Left);
    assert_eq!(app.settings_category, 0);
}

#[test]
fn test_settings_jk_move_field_cursor_in_field_view() {
    let (fixture, mut app) = setup_app_with_docs();
    let key = |app: &mut App, c: KeyCode| {
        app.handle_key(c, KeyModifiers::NONE, fixture.root(), &fixture.config());
    };

    key(&mut app, KeyCode::Char('5'));
    // General (cat 0) is a field-view with 3 fields.
    assert_eq!(app.settings_category, 0);
    assert_eq!(app.settings_field, 0);

    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.settings_field, 1);
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.settings_field, 2);
    // Clamped at the last field (3 fields => max index 2); no overrun.
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.settings_field, 2);

    key(&mut app, KeyCode::Char('k'));
    assert_eq!(app.settings_field, 1);
    // Category does not move while j/k drive the field cursor.
    assert_eq!(app.settings_category, 0);
}

#[test]
fn test_settings_hl_reset_field_cursor() {
    let (fixture, mut app) = setup_app_with_docs();
    let key = |app: &mut App, c: KeyCode| {
        app.handle_key(c, KeyModifiers::NONE, fixture.root(), &fixture.config());
    };

    key(&mut app, KeyCode::Char('5'));
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(app.settings_field, 1);

    // Moving category resets the field cursor (and entry/drill).
    key(&mut app, KeyCode::Char('l'));
    assert_eq!(app.settings_category, 1);
    assert_eq!(app.settings_field, 0);
    assert_eq!(app.settings_entry, 0);
    assert_eq!(app.settings_drill, None);
}
