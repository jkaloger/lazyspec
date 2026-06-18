use crate::common::TestFixture;
use lazyspec::engine::context::resolve_chain;
use lazyspec::tui::state::{App, PreviewTab};
use lazyspec::tui::views::draw;
use ratatui::{backend::TestBackend, Terminal};

fn setup_app_with_relations() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-001-test.md", "Test RFC", "accepted");
    fixture.write_story(
        "STORY-001-test.md",
        "Test Story",
        "draft",
        Some("docs/rfcs/RFC-001-test.md"),
    );
    fixture.write_story(
        "STORY-002-test.md",
        "Test Story Two",
        "draft",
        Some("docs/rfcs/RFC-001-test.md"),
    );
    fixture.write_iteration(
        "ITER-001-test.md",
        "Test Iter",
        "draft",
        Some("docs/stories/STORY-001-test.md"),
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
fn test_toggle_preview_tab() {
    let (_fixture, mut app) = setup_app_with_relations();
    assert_eq!(app.preview_tab, PreviewTab::Preview);

    app.toggle_preview_tab();
    assert_eq!(app.preview_tab, PreviewTab::Relations);
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_toggle_preview_tab_back() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.toggle_preview_tab();
    app.toggle_preview_tab();
    assert_eq!(app.preview_tab, PreviewTab::Preview);
}

#[test]
fn test_toggle_preview_tab_resets_relation() {
    let (_fixture, mut app) = setup_app_with_relations();
    app.selected_relation = 1;

    app.toggle_preview_tab();
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_relation_count() {
    let (_fixture, mut app) = setup_app_with_relations();

    // RFC at index 0 has relations (stories implement it)
    app.selected_type = 0;
    app.selected_doc = 0;
    assert!(app.relation_count() > 0);

    // ADR type (index 3) has no docs, so relation_count is 0
    app.selected_type = 3;
    app.build_doc_tree();
    app.selected_doc = 0;
    assert_eq!(app.relation_count(), 0);
}

#[test]
fn test_move_relation_down() {
    let (_fixture, mut app) = setup_app_with_relations();

    // RFC has 2+ relations (two stories implement it)
    app.selected_type = 0;
    app.selected_doc = 0;
    let count = app.relation_count();
    assert!(
        count >= 2,
        "RFC should have at least 2 relations, got {count}"
    );

    app.selected_relation = 0;
    app.move_relation_down();
    assert_eq!(app.selected_relation, 1);
}

#[test]
fn test_move_relation_down_clamps() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.selected_type = 0;
    app.selected_doc = 0;
    let count = app.relation_count();
    assert!(count > 0);

    app.selected_relation = count - 1;
    app.move_relation_down();
    assert_eq!(app.selected_relation, count - 1);
}

#[test]
fn test_move_relation_up() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.selected_type = 0;
    app.selected_doc = 0;
    let count = app.relation_count();
    assert!(
        count >= 2,
        "RFC should have at least 2 relations, got {count}"
    );

    app.selected_relation = 1;
    app.move_relation_up();
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_move_relation_up_clamps() {
    let (_fixture, mut app) = setup_app_with_relations();

    app.selected_type = 0;
    app.selected_doc = 0;
    app.selected_relation = 0;
    app.move_relation_up();
    assert_eq!(app.selected_relation, 0);
}

#[test]
fn test_navigate_to_relation() {
    let (_fixture, mut app) = setup_app_with_relations();

    // Start at the RFC
    app.selected_type = 0;
    app.selected_doc = 0;
    app.preview_tab = PreviewTab::Relations;

    let count = app.relation_count();
    assert!(count > 0, "RFC should have relations");

    app.selected_relation = 0;
    app.navigate_to_relation();

    // Should have navigated to the related doc (a Story, type index 1)
    assert_eq!(app.selected_type, 1, "should navigate to Story type");
    assert_eq!(app.preview_tab, PreviewTab::Preview);
    assert_eq!(app.selected_relation, 0);
}

/// Navigation maps each `selected_relation` index 1:1 onto `relation_items`
/// even when the flattened order spans all three sections (chain -> children ->
/// related) for a MULTI-PARENT doc -- the exact case the old single-parent walk
/// got wrong. For every index, `navigate_to_relation` must land on the doc that
/// `relation_items[i]` names. We read the expectation from the SAME vector the
/// navigation indexes, so the test is self-consistent regardless of the engine's
/// internal section ordering, and at most one child keeps it deterministic.
#[test]
fn navigate_to_relation_lands_on_relation_items_index_for_multiparent() {
    let fixture = TestFixture::new();
    // Two parents -> 2-element chain (topological/path-stable order).
    fixture.write_story("STORY-001-a.md", "Parent A", "accepted", None);
    fixture.write_story("STORY-002-b.md", "Parent B", "accepted", None);
    // Target implements both parents AND is related-to an RFC.
    fixture.write_doc(
        "docs/iterations/ITER-001-target.md",
        "---\ntitle: \"Target\"\ntype: iteration\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-a.md\n- implements: docs/stories/STORY-002-b.md\n- related to: docs/rfcs/RFC-001-rel.md\n---\n",
    );
    // Exactly one forward child (keeps the children section deterministic).
    fixture.write_iteration(
        "ITER-002-child.md",
        "Child",
        "draft",
        Some("docs/iterations/ITER-001-target.md"),
    );
    // The related-to neighbour.
    fixture.write_rfc("RFC-001-rel.md", "Related RFC", "accepted");

    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    let target_path = std::path::PathBuf::from("docs/iterations/ITER-001-target.md");
    let target_doc = app.store.get(&target_path).unwrap();
    let expected_items = app.relation_items(target_doc);
    assert!(
        expected_items.len() >= 4,
        "fixture must span chain (2) + children (1) + related (1), got {} items: {:?}",
        expected_items.len(),
        expected_items,
    );

    for (i, expected) in expected_items.iter().cloned().enumerate() {
        // Re-select the target before each navigation: navigate_to_relation
        // moves selection, resets selected_relation to 0, and flips the tab.
        select_relations_for(&mut app, "docs/iterations/ITER-001-target.md");
        app.selected_relation = i;

        app.navigate_to_relation();

        let landed = app
            .selected_doc_meta()
            .expect("a doc is selected after navigation");
        assert_eq!(
            landed.path, expected,
            "selected_relation={i} should navigate to relation_items[{i}] ({expected:?}), but landed on {:?}",
            landed.path,
        );
        assert_eq!(
            app.preview_tab,
            PreviewTab::Preview,
            "tab resets to Preview"
        );
        assert_eq!(app.selected_relation, 0, "selected_relation resets to 0");
    }
}

#[test]
fn test_navigate_to_relation_no_doc() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    let before_type = app.selected_type;
    let before_doc = app.selected_doc;

    app.navigate_to_relation();

    assert_eq!(app.selected_type, before_type);
    assert_eq!(app.selected_doc, before_doc);
}

fn buffer_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buf = terminal.backend().buffer();
    let mut lines = Vec::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        lines.push(line);
    }
    lines
}

fn buffer_contains(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
    buffer_lines(terminal).iter().any(|l| l.contains(needle))
}

/// Select the document at `doc_path` (relative to the fixture root, matching
/// how the store keys docs) and switch to the relations tab, so a `draw`
/// renders `render_relationship_sections`.
fn select_relations_for(app: &mut App, doc_path: &str) {
    let rel = std::path::Path::new(doc_path);
    let doc = app
        .store
        .get(rel)
        .unwrap_or_else(|| panic!("doc not in store: {doc_path}"));
    let doc_type = doc.doc_type.clone();
    let type_idx = app
        .doc_types
        .iter()
        .position(|t| *t == doc_type)
        .expect("doc type registered");
    app.selected_type = type_idx;
    app.build_doc_tree();
    let doc_idx = app
        .doc_tree
        .iter()
        .position(|n| n.path == rel)
        .expect("doc in tree");
    app.selected_doc = doc_idx;
    app.preview_tab = PreviewTab::Relations;
}

/// Render parity (data layer): the renderer sources its three sections from
/// `relation_sections`, and the navigable list (`relation_items`) flattens the
/// same `relation_sections` in chain -> children -> related order. Asserting the
/// flatten identity guarantees the rendered list and the navigable list are the
/// same items in the same order. The renderer reads these via the private
/// `panels` module, so this is the strongest equivalence the public API exposes.
#[test]
fn relation_items_flattens_sections_in_render_order() {
    let (_fixture, app) = setup_app_with_relations();
    let doc = app
        .store
        .get(std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .unwrap();

    let sections = app.relation_sections(doc);
    let mut expected: Vec<_> = sections.chain.clone();
    expected.extend(sections.children.clone());
    expected.extend(sections.related.clone());

    assert_eq!(
        app.relation_items(doc),
        expected,
        "navigable list must equal chain ++ children ++ related (render order)"
    );
}

/// The related set the renderer shows (`relation_sections.related`) is exactly
/// the engine `resolve_chain(..)` related set at depth 1 -- the same resolution
/// the `context` command uses.
#[test]
fn relation_sections_related_matches_engine_resolution() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "accepted");
    fixture.write_adr(
        "ADR-001-test.md",
        "Test ADR",
        "accepted",
        Some("docs/rfcs/RFC-001-test.md"),
    );
    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    let doc = app
        .store
        .get(std::path::Path::new("docs/rfcs/RFC-001-test.md"))
        .unwrap();

    let sections = app.relation_sections(doc);
    let resolved = resolve_chain(&app.store, &doc.id, 1).unwrap();
    let engine_related: Vec<_> = resolved
        .related
        .iter()
        .map(|r| r.doc.path.clone())
        .collect();

    assert_eq!(
        sections.related, engine_related,
        "rendered related set must equal engine resolve_chain related set"
    );
    assert!(
        sections
            .related
            .contains(&std::path::PathBuf::from("docs/adrs/ADR-001-test.md")),
        "ADR related-to RFC should appear in the related section"
    );
}

/// A doc whose `implements` lineage has two parents shows BOTH in the chain
/// section (the engine BFS walks all parents; the old inline walk took only one).
#[test]
fn multi_parent_lineage_shows_both_parents() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-one.md", "RFC One", "accepted");
    fixture.write_rfc("RFC-002-two.md", "RFC Two", "accepted");
    // A story that implements two RFCs at once.
    fixture.write_doc(
        "docs/stories/STORY-001-multi.md",
        "---\ntitle: \"Multi Parent\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-one.md\n- implements: docs/rfcs/RFC-002-two.md\n---\n",
    );
    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    let doc = app
        .store
        .get(std::path::Path::new("docs/stories/STORY-001-multi.md"))
        .unwrap();
    let sections = app.relation_sections(doc);

    assert!(
        sections
            .chain
            .contains(&std::path::PathBuf::from("docs/rfcs/RFC-001-one.md")),
        "chain should include first parent, got {:?}",
        sections.chain
    );
    assert!(
        sections
            .chain
            .contains(&std::path::PathBuf::from("docs/rfcs/RFC-002-two.md")),
        "chain should include second parent, got {:?}",
        sections.chain
    );
}

/// Empty-state regression: a doc with no relations renders "No relations."
#[test]
fn render_empty_state_shows_no_relations() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-lonely.md", "Lonely RFC", "accepted");
    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    select_relations_for(&mut app, "docs/rfcs/RFC-001-lonely.md");

    let backend = TestBackend::new(200, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let config = fixture.config();
    terminal.draw(|f| draw(f, &mut app, &config)).unwrap();

    assert!(
        buffer_contains(&terminal, "No relations."),
        "doc with no relations should render the empty state"
    );
}

/// Single-parent + direct related-to regression: the rendered relations tab
/// shows the implementing children and the related doc titles, grouped under
/// section headers.
#[test]
fn render_single_parent_shows_expected_sections() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-base.md", "Base RFC", "accepted");
    fixture.write_story(
        "STORY-001-child.md",
        "Child Story",
        "draft",
        Some("docs/rfcs/RFC-001-base.md"),
    );
    fixture.write_adr(
        "ADR-001-rel.md",
        "Related ADR",
        "accepted",
        Some("docs/rfcs/RFC-001-base.md"),
    );
    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    select_relations_for(&mut app, "docs/rfcs/RFC-001-base.md");

    let backend = TestBackend::new(200, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let config = fixture.config();
    terminal.draw(|f| draw(f, &mut app, &config)).unwrap();

    assert!(
        buffer_contains(&terminal, "children"),
        "single-parent doc with an implementing child should show the children header"
    );
    assert!(
        buffer_contains(&terminal, "Child Story"),
        "the implementing child's title should appear"
    );
    assert!(
        buffer_contains(&terminal, "related"),
        "should show the related header"
    );
    assert!(
        buffer_contains(&terminal, "Related ADR"),
        "the related ADR's title should appear"
    );
    assert!(
        !buffer_contains(&terminal, "No relations."),
        "a doc with relations should not show the empty state"
    );
}
