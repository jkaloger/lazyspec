use crate::common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::config::TypeDef;
use lazyspec::engine::document::DocType;
use lazyspec::engine::store::Store;
use lazyspec::tui::state::{App, ViewMode};

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
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
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

/// Build a fixture deliberately richer than `setup_graph_fixture`: it adds a
/// multi-parent iteration (implements BOTH stories — a diamond) plus a
/// cross-cutting `related-to` link between the two stories, so a single rebuild
/// exercises root ordering, child ordering, diamond back-references, AND
/// `related` annotation ordering all at once. Returns the freshly written
/// `TestFixture` so a second independent `Store` can be loaded from the same
/// files.
fn write_deterministic_forest_fixture() -> TestFixture {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-001-auth.md", "Auth RFC", "accepted");
    fixture.write_rfc("RFC-002-standalone.md", "Standalone RFC", "draft");

    // STORY-001 and STORY-002 both implement RFC-001 (siblings under a shared
    // root) and are related-to each other — a cross-cutting link that must
    // surface as a `related` annotation on both, pinning annotation ordering.
    fixture.write_doc(
        "docs/stories/STORY-001-login.md",
        "---\ntitle: \"Login Story\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n- related to: docs/stories/STORY-002-signup.md\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-signup.md",
        "---\ntitle: \"Signup Story\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n- related to: docs/stories/STORY-001-login.md\n---\n",
    );

    // ITER-001 implements BOTH stories: the diamond. It is drawn in full under
    // the first story encountered and as a back-reference under the second,
    // pinning back-reference ordering.
    fixture.write_doc(
        "docs/iterations/ITER-001-login-impl.md",
        "---\ntitle: \"Login Iteration\"\ntype: iteration\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-login.md\n- implements: docs/stories/STORY-002-signup.md\n---\n",
    );

    fixture
}

fn app_from_fixture(fixture: &TestFixture) -> App {
    App::new(
        fixture.store(),
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    )
}

/// STORY-123 AC4: "Given the same store contents, when the graph view rebuilds
/// twice, then the node ordering is identical between rebuilds (deterministic
/// ordering is preserved)."
///
/// Determinism is guaranteed by construction in `flatten_forest`/`resolve_forest`
/// (path sorts for roots and children, `BTreeSet` for the `related` annotation
/// set). This test self-documents that guarantee. It captures the full ordered
/// node identity sequence `(path, depth, reference, related)` so it also pins
/// annotation determinism, then asserts:
///   1. two `rebuild_graph()` calls on the SAME instance produce identical
///      sequences, and
///   2. a SECOND independent `App` over a freshly-loaded `Store` from the SAME
///      fixture files produces the identical sequence — this catches
///      nondeterminism that a same-process rebuild can mask (e.g. relying on a
///      `HashMap` happening to iterate in a stable order within one process).
#[test]
fn test_rebuild_graph_is_deterministic_across_rebuilds() {
    type NodeSeq = Vec<(std::path::PathBuf, usize, bool, Vec<String>)>;
    let seq = |app: &App| -> NodeSeq {
        app.graph_nodes
            .iter()
            .map(|n| (n.path.clone(), n.depth, n.reference, n.related.clone()))
            .collect()
    };

    let fixture = write_deterministic_forest_fixture();

    let mut app = app_from_fixture(&fixture);
    app.rebuild_graph();
    let first = seq(&app);

    // The fixture must actually exercise ordering: multiple roots, a diamond
    // back-reference, and a related-to annotation. Guard against the fixture
    // silently degenerating into a trivial chain.
    assert!(
        first.len() >= 6,
        "fixture should produce >= 6 nodes (incl. a back-reference), got {}",
        first.len()
    );
    assert!(
        first.iter().any(|(_, _, reference, _)| *reference),
        "fixture should contain a diamond back-reference"
    );
    assert!(
        first.iter().any(|(_, _, _, related)| !related.is_empty()),
        "fixture should contain a related-to annotation to pin its ordering"
    );

    app.rebuild_graph();
    let second = seq(&app);
    assert_eq!(
        first, second,
        "same-instance rebuild must reproduce the identical node sequence"
    );

    // Second, independent App over a freshly-loaded Store from the SAME files.
    let mut other_app = app_from_fixture(&fixture);
    other_app.rebuild_graph();
    let independent = seq(&other_app);
    assert_eq!(
        first, independent,
        "an independent App over the same fixture files must produce the identical \
         node sequence (catches HashMap-iteration-order nondeterminism)"
    );
}

#[test]
fn test_rebuild_graph_roots_have_no_incoming_implements() {
    let (_fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();

    let roots: Vec<_> = app.graph_nodes.iter().filter(|n| n.depth == 0).collect();
    assert_eq!(roots.len(), 2);

    for root in &roots {
        assert_eq!(
            root.doc_type,
            DocType::new(DocType::RFC),
            "root should be an RFC"
        );
    }
}

#[test]
fn test_graph_navigate_j_k() {
    let (fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();
    app.view_mode = ViewMode::Graph;
    app.graph_selected = 0;

    app.handle_key(
        KeyCode::Char('j'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.graph_selected, 1);

    app.handle_key(
        KeyCode::Char('k'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.graph_selected, 0);

    app.handle_key(
        KeyCode::Char('k'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.graph_selected, 0, "should clamp at 0");
}

#[test]
fn test_graph_navigate_g_and_shift_g() {
    let (fixture, mut app) = setup_graph_fixture();
    app.rebuild_graph();
    app.view_mode = ViewMode::Graph;
    app.graph_selected = 0;

    app.handle_key(
        KeyCode::Char('G'),
        KeyModifiers::SHIFT,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(
        app.graph_selected,
        app.graph_nodes.len() - 1,
        "G should jump to last node"
    );

    app.handle_key(
        KeyCode::Char('g'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
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
        .position(|n| {
            n.doc_type
                == lazyspec::engine::document::DocType::new(
                    lazyspec::engine::document::DocType::STORY,
                )
        })
        .expect("should have a story node");

    let story_path = app.graph_nodes[story_idx].path.clone();
    app.graph_selected = story_idx;

    app.handle_key(
        KeyCode::Enter,
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );

    assert_eq!(
        app.view_mode,
        ViewMode::Types,
        "should switch to Types mode"
    );
    assert_eq!(app.selected_type, 1, "Story is at index 1 in doc_types");

    let selected_doc = app.selected_doc_meta().expect("should have a selected doc");
    assert_eq!(
        selected_doc.path, story_path,
        "should select the correct story"
    );
}

#[test]
fn test_graph_rebuilds_on_mode_switch() {
    let (fixture, mut app) = setup_graph_fixture();

    // Cycle from Types -> Filters -> [Metrics] -> Graph
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

    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    assert_eq!(app.view_mode, ViewMode::Graph);
    assert!(
        !app.graph_nodes.is_empty(),
        "graph should be populated on entering Graph mode"
    );

    let first_count = app.graph_nodes.len();

    // Cycle away: Graph -> Agents -> Types (or Graph -> Types without agent feature)
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    #[cfg(feature = "agent")]
    {
        assert_eq!(app.view_mode, ViewMode::Agents);
        app.handle_key(
            KeyCode::Char('`'),
            KeyModifiers::NONE,
            fixture.root(),
            &fixture.config(),
        );
    }
    assert_eq!(app.view_mode, ViewMode::Types);

    // Cycle back: Types -> Filters -> [Metrics] -> Graph
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
    #[cfg(feature = "metrics")]
    app.handle_key(
        KeyCode::Char('`'),
        KeyModifiers::NONE,
        fixture.root(),
        &fixture.config(),
    );
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
            subdirectory: false,
            store: Default::default(),
            singleton: false,
            parent_type: None,
        },
        TypeDef {
            name: "task".into(),
            plural: "tasks".into(),
            dir: "docs/tasks".into(),
            prefix: "TASK".into(),
            icon: None,
            numbering: Default::default(),
            subdirectory: false,
            store: Default::default(),
            singleton: false,
            parent_type: None,
        },
    ];
    let store = Store::load(fixture.root(), &config).unwrap();
    let app = App::new(
        store,
        &config,
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    assert_eq!(app.doc_types.len(), 2);
    assert_eq!(app.doc_types[0], DocType::new("epic"));
    assert_eq!(app.doc_types[1], DocType::new("task"));
    assert_eq!(app.type_icons["epic"], "⚡");
    assert_eq!(app.type_icons["task"], "■"); // second fallback glyph (index 1)
}
