mod common;

use common::TestFixture;
use lazyspec::engine::config::{Config, StatusBarConfig};
use lazyspec::tui::state::{App, ViewMode};
use lazyspec::tui::views::status_bar::*;

fn make_app(fixture: &TestFixture) -> App {
    let store = fixture.store();
    App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    )
}

fn fixture_with_docs() -> (TestFixture, App) {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/001-first.md",
        "---\ntitle: First\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );
    let app = make_app(&fixture);
    (fixture, app)
}

#[test]
fn mode_component_returns_bold_span_with_mode_name() {
    let (_fixture, app) = fixture_with_docs();
    let span = mode_component(&app).expect("mode_component should return Some");
    assert!(
        span.content.contains("Types"),
        "expected 'Types', got '{}'",
        span.content
    );
}

#[test]
fn doc_count_component_returns_count_when_nonzero() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/001-first.md",
        "---\ntitle: First\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-01\ntags: []\n---\nBody\n",
    );
    fixture.write_doc(
        "docs/rfcs/002-second.md",
        "---\ntitle: Second\ntype: rfc\nauthor: test\nstatus: draft\ndate: 2025-01-02\ntags: []\n---\nBody\n",
    );
    let mut app = make_app(&fixture);
    app.build_doc_tree();

    let span =
        doc_count_component(&app).expect("doc_count_component should return Some when docs exist");
    assert!(
        span.content.contains("docs"),
        "expected 'docs' in '{}' ",
        span.content
    );
}

#[test]
fn doc_count_component_returns_none_when_empty() {
    let fixture = TestFixture::new();
    let app = make_app(&fixture);
    assert!(
        doc_count_component(&app).is_none(),
        "doc_count_component should return None when doc_tree is empty"
    );
}

#[test]
fn warnings_component_returns_none_when_zero() {
    let (_fixture, app) = fixture_with_docs();
    assert!(
        warnings_component(&app).is_none(),
        "warnings_component should return None with no warnings"
    );
}

#[test]
fn warnings_component_returns_yellow_span_when_nonzero() {
    let (_fixture, mut app) = fixture_with_docs();
    app.validation_warnings = vec!["some warning".to_string()];

    let span = warnings_component(&app).expect("should return Some with warnings");
    assert!(
        span.content.contains("1"),
        "expected count '1', got '{}'",
        span.content
    );
    assert_eq!(
        span.style.fg,
        Some(ratatui::style::Color::Yellow),
        "warnings should be yellow"
    );
}

#[test]
fn errors_component_returns_none_when_zero() {
    let (_fixture, app) = fixture_with_docs();
    assert!(
        errors_component(&app).is_none(),
        "errors_component should return None with no errors"
    );
}

#[test]
fn errors_component_returns_red_span_when_nonzero() {
    let (_fixture, mut app) = fixture_with_docs();
    app.validation_errors = vec!["err1".to_string(), "err2".to_string()];

    let span = errors_component(&app).expect("should return Some with errors");
    assert!(
        span.content.contains("2"),
        "expected count '2', got '{}'",
        span.content
    );
    assert_eq!(
        span.style.fg,
        Some(ratatui::style::Color::Red),
        "errors should be red"
    );
}

#[test]
fn version_component_always_returns_some() {
    let (_fixture, app) = fixture_with_docs();
    let span = version_component(&app).expect("version_component should always return Some");
    assert!(
        span.content.starts_with("lazyspec v"),
        "expected version string, got '{}'",
        span.content
    );
}

#[test]
fn help_hint_component_always_returns_some() {
    let (_fixture, app) = fixture_with_docs();
    let span = help_hint_component(&app).expect("help_hint_component should always return Some");
    assert_eq!(span.content.as_ref(), "? help");
    assert_eq!(
        span.style.fg,
        Some(ratatui::style::Color::DarkGray),
        "help hint should be DarkGray"
    );
}

#[test]
fn default_components_wire_correctly() {
    let components = StatusBarComponents::default();
    assert_eq!(components.left.len(), 3, "left should have 3 components");
    assert_eq!(
        components.center.len(),
        2,
        "center should have 2 components"
    );
    assert_eq!(components.right.len(), 4, "right should have 4 components");
}

#[test]
fn git_branch_component_returns_none_when_no_branch() {
    let fixture = TestFixture::new();
    let app = make_app(&fixture);
    assert!(
        git_branch_component(&app).is_none(),
        "git_branch_component should return None when git_branch is None"
    );
}

#[test]
fn git_branch_component_returns_span_when_branch_set() {
    let fixture = TestFixture::new();
    let mut app = make_app(&fixture);
    app.git_branch = Some("main".to_string());

    let span = git_branch_component(&app).expect("should return Some when branch is set");
    assert!(
        span.content.contains("main"),
        "expected 'main', got '{}'",
        span.content
    );
    assert_eq!(
        span.style.fg,
        Some(ratatui::style::Color::Cyan),
        "branch should be cyan"
    );
}

#[test]
fn search_component_returns_query_in_search_mode() {
    let (_fixture, mut app) = fixture_with_docs();
    app.search_mode = true;
    app.search_query = "hello".to_string();

    let span = search_component(&app).expect("should return Some in search mode with query");
    assert!(
        span.content.contains("/hello"),
        "expected '/hello', got '{}'",
        span.content
    );
    assert_eq!(
        span.style.fg,
        Some(ratatui::style::Color::Yellow),
        "search should be yellow"
    );
}

#[test]
fn search_component_returns_none_when_not_searching() {
    let (_fixture, app) = fixture_with_docs();
    assert!(
        search_component(&app).is_none(),
        "search_component should return None when not in search mode"
    );
}

#[test]
fn type_filter_component_returns_type_in_types_mode() {
    let (_fixture, app) = fixture_with_docs();
    assert_eq!(
        app.view_mode,
        ViewMode::Types,
        "app should default to Types mode"
    );

    let span = type_filter_component(&app).expect("should return Some in Types mode");
    let type_name = app.current_type().to_string();
    assert!(
        span.content.contains(&type_name),
        "expected '{}', got '{}'",
        type_name,
        span.content
    );
}

#[test]
fn type_filter_component_returns_none_in_other_modes() {
    let (_fixture, mut app) = fixture_with_docs();
    app.view_mode = ViewMode::Graph;

    assert!(
        type_filter_component(&app).is_none(),
        "type_filter_component should return None in Graph mode"
    );
}

#[test]
fn config_round_trip_through_toml() {
    let toml_with_statusbar = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[tui.statusbar]
enabled = true
left = ["mode", "doc_count"]
center = ["warnings"]
right = ["version"]
"#;
    let config = Config::parse(toml_with_statusbar).unwrap();
    assert!(config.ui.statusbar.enabled);
    assert_eq!(
        config.ui.statusbar.left,
        Some(vec!["mode".to_string(), "doc_count".to_string()])
    );
    assert_eq!(
        config.ui.statusbar.center,
        Some(vec!["warnings".to_string()])
    );
    assert_eq!(config.ui.statusbar.right, Some(vec!["version".to_string()]));

    // Without [tui.statusbar] section, defaults apply
    let toml_without = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;
    let config2 = Config::parse(toml_without).unwrap();
    assert!(config2.ui.statusbar.enabled);
    assert!(config2.ui.statusbar.left.is_none());
    assert!(config2.ui.statusbar.center.is_none());
    assert!(config2.ui.statusbar.right.is_none());
}

#[test]
fn enabled_false_in_toml() {
    let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[tui.statusbar]
enabled = false
"#;
    let config = Config::parse(toml_str).unwrap();
    assert!(!config.ui.statusbar.enabled);
}

#[test]
fn empty_zone_array_preserved() {
    let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[tui.statusbar]
left = []
"#;
    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.ui.statusbar.left, Some(vec![]));
    assert!(config.ui.statusbar.center.is_none());
    assert!(config.ui.statusbar.right.is_none());
}

#[test]
fn partial_zone_definition_leaves_others_as_none() {
    let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[tui.statusbar]
right = ["help_hint"]
"#;
    let config = Config::parse(toml_str).unwrap();
    assert!(config.ui.statusbar.left.is_none());
    assert!(config.ui.statusbar.center.is_none());
    assert_eq!(
        config.ui.statusbar.right,
        Some(vec!["help_hint".to_string()])
    );
}

#[test]
fn default_config_produces_default_components() {
    let config = StatusBarConfig::default();
    let (components, warnings) = StatusBarComponents::from_config(&config);
    let defaults = StatusBarComponents::default();
    assert_eq!(components.left.len(), defaults.left.len());
    assert_eq!(components.center.len(), defaults.center.len());
    assert_eq!(components.right.len(), defaults.right.len());
    assert!(
        warnings.is_empty(),
        "no warnings expected, got {:?}",
        warnings
    );
}

#[test]
fn custom_left_zone_overrides_default() {
    let config = StatusBarConfig {
        left: Some(vec!["mode".into()]),
        center: None,
        right: None,
        ..StatusBarConfig::default()
    };
    let (components, warnings) = StatusBarComponents::from_config(&config);
    assert_eq!(components.left.len(), 1);
    assert_eq!(components.center.len(), 2);
    assert_eq!(components.right.len(), 4);
    assert!(warnings.is_empty());
}

#[test]
fn empty_zone_array_produces_empty_zone() {
    let config = StatusBarConfig {
        left: Some(vec![]),
        ..StatusBarConfig::default()
    };
    let (components, _warnings) = StatusBarComponents::from_config(&config);
    assert!(components.left.is_empty());
}

#[test]
fn invalid_component_name_skipped_with_warning() {
    let config = StatusBarConfig {
        left: Some(vec!["mode".into(), "bogus".into()]),
        ..StatusBarConfig::default()
    };
    let (components, warnings) = StatusBarComponents::from_config(&config);
    assert_eq!(components.left.len(), 1);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].contains("bogus"),
        "warning should mention 'bogus', got '{}'",
        warnings[0]
    );
}
