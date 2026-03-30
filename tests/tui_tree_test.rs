mod common;

use common::TestFixture;
use lazyspec::tui::state::App;

const PARENT_FRONTMATTER: &str = "\
---
title: Parent RFC
type: rfc
status: draft
author: test
date: 2026-01-01
tags: []
---
";

const CHILD_A_FRONTMATTER: &str = "\
---
title: Child A
type: rfc
status: draft
author: test
date: 2026-01-01
tags: []
---
";

const CHILD_B_FRONTMATTER: &str = "\
---
title: Child B
type: rfc
status: draft
author: test
date: 2026-01-01
tags: []
---
";

fn setup_parent_with_children() -> (TestFixture, App) {
    let fixture = TestFixture::new();

    fixture.write_subfolder_doc("docs/rfcs/RFC-001-parent", PARENT_FRONTMATTER);
    fixture.write_child_doc(
        "docs/rfcs/RFC-001-parent",
        "child-a.md",
        CHILD_A_FRONTMATTER,
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-001-parent",
        "child-b.md",
        CHILD_B_FRONTMATTER,
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
fn test_tree_building_expanded_collapsed_state() {
    let (_fixture, mut app) = setup_parent_with_children();

    // Collapsed by default: only parent visible
    assert_eq!(app.doc_tree.len(), 1);
    assert_eq!(app.doc_tree[0].title, "Parent RFC");
    assert_eq!(app.doc_tree[0].depth, 0);
    assert!(app.doc_tree[0].is_parent);

    // Expand: parent + two children
    let parent_path = app.doc_tree[0].path.clone();
    app.toggle_expanded(&parent_path);

    assert_eq!(app.doc_tree.len(), 3);
    assert_eq!(app.doc_tree[0].depth, 0);
    assert!(app.doc_tree[0].is_parent);
    assert_eq!(app.doc_tree[1].depth, 1);
    assert_eq!(app.doc_tree[2].depth, 1);

    // Collapse again: only parent
    app.toggle_expanded(&parent_path);

    assert_eq!(app.doc_tree.len(), 1);
    assert_eq!(app.doc_tree[0].title, "Parent RFC");
}

#[test]
fn test_standalone_documents_unaffected() {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-010-standalone.md",
        "---\ntitle: Standalone\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_subfolder_doc("docs/rfcs/RFC-001-parent", PARENT_FRONTMATTER);
    fixture.write_child_doc(
        "docs/rfcs/RFC-001-parent",
        "child-a.md",
        CHILD_A_FRONTMATTER,
    );

    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    // Collapsed: parent + standalone, both at depth 0
    assert_eq!(app.doc_tree.len(), 2);
    let standalone = app
        .doc_tree
        .iter()
        .find(|n| n.title == "Standalone")
        .unwrap();
    assert_eq!(standalone.depth, 0);
    assert!(!standalone.is_parent);

    let parent = app
        .doc_tree
        .iter()
        .find(|n| n.title == "Parent RFC")
        .unwrap();
    assert!(parent.is_parent);

    // Expand: child only appears under parent
    let parent_path = parent.path.clone();
    app.toggle_expanded(&parent_path);

    assert_eq!(app.doc_tree.len(), 3);
    let standalone = app
        .doc_tree
        .iter()
        .find(|n| n.title == "Standalone")
        .unwrap();
    assert_eq!(standalone.depth, 0);
    assert!(!standalone.is_parent);

    let children: Vec<_> = app.doc_tree.iter().filter(|n| n.depth == 1).collect();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].title, "Child A");
}

#[test]
fn test_virtual_parent_rendering() {
    let fixture = TestFixture::new();

    // No index.md in the folder -> virtual parent
    fixture.write_child_doc(
        "docs/rfcs/RFC-002-virtual",
        "part-one.md",
        CHILD_A_FRONTMATTER,
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-002-virtual",
        "part-two.md",
        CHILD_B_FRONTMATTER,
    );

    let store = fixture.store();
    let app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );

    // Virtual parent should exist
    assert_eq!(app.doc_tree.len(), 1);
    assert!(app.doc_tree[0].is_virtual);
    assert!(app.doc_tree[0].is_parent);
}

#[test]
fn test_collapse_moves_selection_to_parent() {
    let (_fixture, mut app) = setup_parent_with_children();

    let parent_path = app.doc_tree[0].path.clone();
    app.toggle_expanded(&parent_path);

    // Select second child (index 2)
    app.selected_doc = 2;
    assert_eq!(app.doc_tree[app.selected_doc].depth, 1);

    // Simulate left/h collapse: walk back to find parent, move selection, toggle collapse
    let mut parent_idx = app.selected_doc;
    for i in (0..app.selected_doc).rev() {
        if app.doc_tree[i].depth == 0 {
            parent_idx = i;
            break;
        }
    }
    app.selected_doc = parent_idx;
    let collapse_path = app.doc_tree[parent_idx].path.clone();
    app.toggle_expanded(&collapse_path);
    app.clamp_selected_doc();

    assert_eq!(app.selected_doc, 0);
    assert_eq!(app.doc_tree[app.selected_doc].title, "Parent RFC");
    assert_eq!(app.doc_tree.len(), 1);
}

#[test]
fn test_child_documents_not_duplicated_at_top_level() {
    let (_fixture, mut app) = setup_parent_with_children();

    let parent_path = app.doc_tree[0].path.clone();
    app.toggle_expanded(&parent_path);

    assert_eq!(app.doc_tree.len(), 3);

    // Only one depth-0 entry (the parent)
    let top_level: Vec<_> = app.doc_tree.iter().filter(|n| n.depth == 0).collect();
    assert_eq!(top_level.len(), 1);
    assert_eq!(top_level[0].title, "Parent RFC");

    // Children appear exactly once each
    let child_a_count = app.doc_tree.iter().filter(|n| n.title == "Child A").count();
    let child_b_count = app.doc_tree.iter().filter(|n| n.title == "Child B").count();
    assert_eq!(child_a_count, 1);
    assert_eq!(child_b_count, 1);
}
