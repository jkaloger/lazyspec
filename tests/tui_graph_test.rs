mod common;

use common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::config::TypeDef;
use lazyspec::engine::document::DocType;
use lazyspec::engine::store::Store;
use lazyspec::tui::app::{App, ViewMode};

fn setup_graph_fixture() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-001-auth.md", "Auth RFC", "accepted");
    fixture.write_rfc("RFC-002-standalone.md", "Standalone RFC", "draft");

    fixture.write_story(
        "STORY-001-login.md",
        "Login Story",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );
    fixture.write_story(
        "STORY-002-signup.md",
        "Signup Story",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );
    fixture.write_iteration(
        "ITER-001-login-impl.md",
        "Login Iteration",
        "draft",
        Some("docs/stories/STORY-001-login.md"),
    );

    let store = fixture.store();
    let app = App::new(store, &fixture.config(), ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));
    (fixture, app)
}

#[test]
fn test_rebuild_graph_builds_forest() {
    let (_fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();

    assert_eq!(
        app.graph_nodes.len(),
        5,
        "expected 5 graph nodes, got {}",
        app.graph_nodes.len()
    );

    let depth_0: Vec<_> = app.graph_nodes.iter().filter(|n| n.depth == 0).collect();
    let depth_1: Vec<_> = app.graph_nodes.iter().filter(|n| n.depth == 1).collect();
    let depth_2: Vec<_> = app.graph_nodes.iter().filter(|n| n.depth == 2).collect();

    assert_eq!(depth_0.len(), 2, "expected 2 roots");
    assert_eq!(depth_1.len(), 2, "expected 2 depth-1 nodes");
    assert_eq!(depth_2.len(), 1, "expected 1 depth-2 node");
}

#[test]
fn test_rebuild_graph_roots_have_no_incoming_implements() {
    let (_fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();

    let roots: Vec<_> = app.graph_nodes.iter().filter(|n| n.depth == 0).collect();
    assert_eq!(roots.len(), 2);

    for root in &roots {
        assert_eq!(root.doc_type, DocType::new(DocType::RFC), "root should be an RFC");
    }
}

#[test]
fn test_graph_navigate_j_k() {
    let (fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();
    app.view_mode = ViewMode::Graph;
    app.graph_selected = 0;

    app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.graph_selected, 1);

    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.graph_selected, 0);

    app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.graph_selected, 0, "should clamp at 0");
}

#[test]
fn test_graph_navigate_g_and_shift_g() {
    let (fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();
    app.view_mode = ViewMode::Graph;
    app.graph_selected = 0;

    app.handle_key(KeyCode::Char('G'), KeyModifiers::SHIFT, fixture.root(), &fixture.config());
    assert_eq!(
        app.graph_selected,
        app.graph_nodes.len() - 1,
        "G should jump to last node"
    );

    app.handle_key(KeyCode::Char('g'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.graph_selected, 0, "g should jump to first node");
}

#[test]
fn test_graph_enter_jumps_to_types_mode() {
    let (fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();
    app.view_mode = ViewMode::Graph;

    // Find the index of the first Story node in the graph
    let story_idx = app
        .graph_nodes
        .iter()
        .position(|n| n.doc_type == lazyspec::engine::document::DocType::new(lazyspec::engine::document::DocType::STORY))
        .expect("should have a story node");

    let story_path = app.graph_nodes[story_idx].path.clone();
    app.graph_selected = story_idx;

    app.handle_key(KeyCode::Enter, KeyModifiers::NONE, fixture.root(), &fixture.config());

    assert_eq!(app.view_mode, ViewMode::Types, "should switch to Types mode");
    assert_eq!(app.selected_type, 1, "Story is at index 1 in doc_types");

    let selected_doc = app.selected_doc_meta().expect("should have a selected doc");
    assert_eq!(selected_doc.path, story_path, "should select the correct story");
}

#[test]
fn test_graph_rebuilds_on_mode_switch() {
    let (fixture, mut app) = setup_graph_fixture();

    // Cycle from Types -> Filters -> Metrics -> Graph
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Filters);

    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Metrics);

    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Graph);
    assert!(!app.graph_nodes.is_empty(), "graph should be populated on entering Graph mode");

    let first_count = app.graph_nodes.len();

    // Cycle away: Graph -> Agents -> Types (or Graph -> Types without agent feature)
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    #[cfg(feature = "agent")]
    {
        assert_eq!(app.view_mode, ViewMode::Agents);
        app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    }
    assert_eq!(app.view_mode, ViewMode::Types);

    // Cycle back: Types -> Filters -> Metrics -> Graph
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    app.handle_key(KeyCode::Char('`'), KeyModifiers::NONE, fixture.root(), &fixture.config());
    assert_eq!(app.view_mode, ViewMode::Graph);
    assert_eq!(
        app.graph_nodes.len(),
        first_count,
        "graph should be rebuilt with same count"
    );
}

#[test]
fn custom_types_populate_doc_types_and_icons() {
    let fixture = TestFixture::new();
    let mut config = fixture.config();
    config.documents.types = vec![
        TypeDef {
            name: "epic".into(),
            plural: "epics".into(),
            dir: "docs/epics".into(),
            prefix: "EPIC".into(),
            icon: Some("⚡".into()),
            numbering: Default::default(),
        },
        TypeDef {
            name: "task".into(),
            plural: "tasks".into(),
            dir: "docs/tasks".into(),
            prefix: "TASK".into(),
            icon: None,
            numbering: Default::default(),
        },
    ];
    let store = Store::load(fixture.root(), &config).unwrap();
    let app = App::new(store, &config, ratatui_image::picker::Picker::halfblocks(), Box::new(lazyspec::engine::fs::RealFileSystem));

    assert_eq!(app.doc_types.len(), 2);
    assert_eq!(app.doc_types[0], DocType::new("epic"));
    assert_eq!(app.doc_types[1], DocType::new("task"));
    assert_eq!(app.type_icons["epic"], "⚡");
    assert_eq!(app.type_icons["task"], "■"); // second fallback glyph (index 1)
}
