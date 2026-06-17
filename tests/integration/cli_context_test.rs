use crate::common::TestFixture;

fn setup() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security]\nrelated: []\n---\n\nRFC body.\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-auth-impl.md",
        "---\ntitle: \"Auth Implementation\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [security]\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n---\n\nStory body.\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-auth-sprint.md",
        "---\ntitle: \"Auth Sprint 1\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-auth-impl.md\n---\n\nIteration body.\n",
    );
    fixture
}

#[test]
fn context_walks_full_chain() {
    let fixture = setup();
    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001", 1).unwrap();

    assert_eq!(resolved.nodes.len(), 3);
    assert_eq!(resolved.nodes[0].doc.title, "Auth Redesign");
    assert_eq!(resolved.nodes[1].doc.title, "Auth Implementation");
    assert_eq!(resolved.nodes[2].doc.title, "Auth Sprint 1");
    assert_eq!(resolved.target.title, "Auth Sprint 1");
}

#[test]
fn context_depth_default_matches_today() {
    // Depth 1 must reproduce the pre-iteration single-hop related set: the
    // STORY chain (RFC<-STORY) relates (via the RFC) to the Token Strategy ADR,
    // and every surfaced related doc is tagged distance == 1.
    let fixture = setup_with_related();
    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 1).unwrap();

    let titles: Vec<&str> = resolved
        .related
        .iter()
        .map(|r| r.doc.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Token Strategy"], "depth-1 related set");
    assert!(
        resolved.related.iter().all(|r| r.distance == 1),
        "every depth-1 related doc is one hop out"
    );
}

#[test]
fn context_standalone_document() {
    let fixture = setup();
    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "RFC-001", 1).unwrap();

    assert_eq!(resolved.nodes.len(), 1);
    assert_eq!(resolved.nodes[0].doc.title, "Auth Redesign");
    assert_eq!(resolved.target.title, "Auth Redesign");
}

#[test]
fn context_json_output() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "ITERATION-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let chain = parsed["chain"].as_array().unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0]["type"], "rfc");
    assert_eq!(chain[1]["type"], "story");
    assert_eq!(chain[2]["type"], "iteration");
    assert_eq!(chain[0]["title"], "Auth Redesign");
}

#[test]
fn context_human_output() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "ITERATION-001", 1).unwrap();

    assert!(output.contains("Auth Redesign"));
    assert!(output.contains("Auth Implementation"));
    assert!(output.contains("Auth Sprint 1"));
    assert!(output.contains("rfc"));
    assert!(output.contains("story"));
    assert!(output.contains("iteration"));
}

#[test]
fn context_not_found() {
    let fixture = setup();
    let store = fixture.store();
    let result = lazyspec::cli::context::resolve_chain(&store, "NONEXISTENT-999", 1);
    assert!(result.is_err());
}

fn setup_with_related() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security]\nrelated:\n- related to: docs/adrs/ADR-001-tokens.md\n---\n\nRFC body.\n",
    );
    fixture.write_doc(
        "docs/adrs/ADR-001-tokens.md",
        "---\ntitle: \"Token Strategy\"\ntype: adr\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nADR body.\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-auth-impl.md",
        "---\ntitle: \"Auth Implementation\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [security]\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n---\n\nStory body.\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-auth-sprint.md",
        "---\ntitle: \"Auth Sprint 1\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-auth-impl.md\n---\n\nIteration body.\n",
    );
    fixture
}

#[test]
fn forward_context_from_rfc() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nRFC body.\n",
    );
    fixture.write_story(
        "STORY-001-impl-a.md",
        "Impl A",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );
    fixture.write_story(
        "STORY-002-impl-b.md",
        "Impl B",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "RFC-001", 1).unwrap();

    assert_eq!(resolved.nodes.len(), 1);
    assert_eq!(resolved.forward.len(), 2);
    let forward_titles: Vec<&str> = resolved
        .forward
        .iter()
        .map(|f| f.doc.title.as_str())
        .collect();
    assert!(forward_titles.contains(&"Impl A"));
    assert!(forward_titles.contains(&"Impl B"));
}

#[test]
fn forward_context_from_story() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth Redesign", "accepted");
    fixture.write_story(
        "STORY-001-auth.md",
        "Auth Implementation",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );
    fixture.write_iteration(
        "ITERATION-001-sprint1.md",
        "Sprint 1",
        "draft",
        Some("docs/stories/STORY-001-auth.md"),
    );
    fixture.write_iteration(
        "ITERATION-002-sprint2.md",
        "Sprint 2",
        "draft",
        Some("docs/stories/STORY-001-auth.md"),
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 1).unwrap();

    assert_eq!(resolved.nodes.len(), 2);
    assert_eq!(resolved.nodes[0].doc.title, "Auth Redesign");
    assert_eq!(resolved.nodes[1].doc.title, "Auth Implementation");
    assert_eq!(resolved.forward.len(), 2);
    let forward_titles: Vec<&str> = resolved
        .forward
        .iter()
        .map(|f| f.doc.title.as_str())
        .collect();
    assert!(forward_titles.contains(&"Sprint 1"));
    assert!(forward_titles.contains(&"Sprint 2"));
}

#[test]
fn you_are_here_marker() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "STORY-001", 1).unwrap();

    let marker = "\u{2190} you are here";
    let marker_count = output.matches(marker).count();
    assert_eq!(
        marker_count, 1,
        "expected exactly one 'you are here' marker, found {}",
        marker_count
    );

    let marker_line = output.lines().find(|l| l.contains(marker)).unwrap();
    assert!(
        marker_line.contains("Auth Implementation"),
        "marker should be on the Story line, got: {}",
        marker_line
    );
    assert!(!marker_line.contains("Auth Redesign"));
    assert!(!marker_line.contains("Auth Sprint 1"));
}

#[test]
fn related_records_in_human_output() {
    let fixture = setup_with_related();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "STORY-001", 1).unwrap();

    assert!(
        output.contains("related"),
        "output should contain 'related' section header"
    );
    assert!(
        output.contains("Token Strategy"),
        "output should contain the related document title"
    );
}

#[test]
fn related_records_omitted_when_none() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "STORY-001", 1).unwrap();

    assert!(
        !output.contains("related"),
        "output should not contain 'related' when there are no related-to links"
    );
}

#[test]
fn json_related_field_present() {
    let fixture = setup_with_related();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "STORY-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let related = parsed["related"].as_array().unwrap();
    assert!(!related.is_empty(), "related array should be non-empty");
    let titles: Vec<&str> = related.iter().filter_map(|r| r["title"].as_str()).collect();
    assert!(
        titles.contains(&"Token Strategy"),
        "related should contain 'Token Strategy'"
    );
}

#[test]
fn json_related_empty() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "STORY-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let related = parsed["related"].as_array().unwrap();
    assert!(
        related.is_empty(),
        "related array should be empty when no related-to links exist"
    );
}

#[test]
fn no_forward_children_for_leaf() {
    let fixture = setup();
    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001", 1).unwrap();

    assert!(
        resolved.forward.is_empty(),
        "leaf node should have no forward children"
    );
}

fn node_titles<'a>(resolved: &'a lazyspec::cli::context::ResolvedContext<'a>) -> Vec<&'a str> {
    resolved
        .nodes
        .iter()
        .map(|n| n.doc.title.as_str())
        .collect()
}

#[test]
fn context_multi_parent_includes_all_ancestors() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-alpha.md",
        "---\ntitle: \"RFC Alpha\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-beta.md",
        "---\ntitle: \"RFC Beta\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-fan.md",
        "---\ntitle: \"Fan Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-alpha.md\n- implements: docs/rfcs/RFC-002-beta.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 1).unwrap();

    let titles = node_titles(&resolved);
    assert_eq!(resolved.nodes.len(), 3, "got: {:?}", titles);
    assert!(titles.contains(&"RFC Alpha"));
    assert!(titles.contains(&"RFC Beta"));
    assert!(titles.contains(&"Fan Story"));
    assert_eq!(resolved.target.title, "Fan Story");

    // The story's node records both parent edges.
    let story_node = resolved
        .nodes
        .iter()
        .find(|n| n.doc.title == "Fan Story")
        .unwrap();
    assert_eq!(story_node.parents.len(), 2, "got: {:?}", story_node.parents);
}

#[test]
fn context_diamond_ancestor_appears_once() {
    // GRANDPARENT <- PARENT_A, PARENT_B; both <- TARGET (diamond).
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-root.md",
        "---\ntitle: \"Root RFC\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-left.md",
        "---\ntitle: \"Left Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-right.md",
        "---\ntitle: \"Right Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-bottom.md",
        "---\ntitle: \"Bottom Iteration\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-left.md\n- implements: docs/stories/STORY-002-right.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001", 1).unwrap();

    let titles = node_titles(&resolved);
    assert_eq!(resolved.nodes.len(), 4, "got: {:?}", titles);
    assert_eq!(
        titles.iter().filter(|t| **t == "Root RFC").count(),
        1,
        "shared ancestor should appear exactly once; got: {:?}",
        titles
    );

    // Both parents record an edge to the shared grandparent.
    let root_path = std::path::PathBuf::from("docs/rfcs/RFC-001-root.md");
    let left_node = resolved
        .nodes
        .iter()
        .find(|n| n.doc.title == "Left Story")
        .unwrap();
    let right_node = resolved
        .nodes
        .iter()
        .find(|n| n.doc.title == "Right Story")
        .unwrap();
    assert!(
        left_node.parents.contains(&root_path),
        "left story should record edge to grandparent; got: {:?}",
        left_node.parents
    );
    assert!(
        right_node.parents.contains(&root_path),
        "right story should record edge to grandparent; got: {:?}",
        right_node.parents
    );

    // Root-first topo order: grandparent precedes both parents, which precede target.
    let pos = |title: &str| titles.iter().position(|t| *t == title).unwrap();
    assert!(pos("Root RFC") < pos("Left Story"));
    assert!(pos("Root RFC") < pos("Right Story"));
    assert!(pos("Left Story") < pos("Bottom Iteration"));
    assert!(pos("Right Story") < pos("Bottom Iteration"));
}

#[test]
fn context_human_tree_for_multi_parent() {
    // Diamond: Root RFC <- Left Story, Right Story; both <- Bottom Iteration.
    // Distinct titles avoid substring collisions for .matches().count().
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-root.md",
        "---\ntitle: \"Foundation Charter\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-left.md",
        "---\ntitle: \"Western Wing\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-right.md",
        "---\ntitle: \"Eastern Wing\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-bottom.md",
        "---\ntitle: \"Keystone Sprint\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-left.md\n- implements: docs/stories/STORY-002-right.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "ITERATION-001", 1).unwrap();

    // All node titles present.
    assert!(output.contains("Foundation Charter"), "got:\n{}", output);
    assert!(output.contains("Western Wing"), "got:\n{}", output);
    assert!(output.contains("Eastern Wing"), "got:\n{}", output);
    assert!(output.contains("Keystone Sprint"), "got:\n{}", output);

    // Exactly one 'you are here' marker, on the target (the iteration).
    let marker = "\u{2190} you are here";
    let marker_count = output.matches(marker).count();
    assert_eq!(
        marker_count, 1,
        "expected exactly one marker, found {}; output:\n{}",
        marker_count, output
    );
    let marker_line = output.lines().find(|l| l.contains(marker)).unwrap();
    assert!(
        marker_line.contains("Keystone Sprint"),
        "marker should be on the target (iteration) line, got: {}",
        marker_line
    );

    // Shared ancestor drawn exactly once (diamond-once).
    assert_eq!(
        output.matches("Foundation Charter").count(),
        1,
        "shared ancestor title should appear exactly once; output:\n{}",
        output
    );

    // Tree shape: descendants are indented deeper than the root. The root
    // card's title line has no leading whitespace; the target's does.
    let indent_of = |needle: &str| -> usize {
        let line = output
            .lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line for {}; output:\n{}", needle, output));
        line.len() - line.trim_start().len()
    };
    assert_eq!(
        indent_of("Foundation Charter"),
        0,
        "root should be unindented; output:\n{}",
        output
    );
    assert!(
        indent_of("Keystone Sprint") > indent_of("Western Wing"),
        "target should be indented deeper than its parent story; output:\n{}",
        output
    );
    assert!(
        indent_of("Western Wing") > indent_of("Foundation Charter"),
        "story should be indented deeper than the root; output:\n{}",
        output
    );
}

#[test]
fn context_json_forward_populated() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nRFC body.\n",
    );
    fixture.write_story(
        "STORY-001-impl-a.md",
        "Impl A",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );
    fixture.write_story(
        "STORY-002-impl-b.md",
        "Impl B",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );

    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "RFC-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let forward = parsed["forward"].as_array().unwrap();
    assert_eq!(forward.len(), 2, "forward should list both implementors");
    let titles: Vec<&str> = forward.iter().filter_map(|f| f["title"].as_str()).collect();
    assert!(titles.contains(&"Impl A"));
    assert!(titles.contains(&"Impl B"));
}

#[test]
fn context_json_forward_empty() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "ITERATION-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let forward = parsed["forward"].as_array().unwrap();
    assert!(
        forward.is_empty(),
        "leaf iteration should have an empty (present) forward array"
    );
}

#[test]
fn context_json_edges_reconstructable() {
    // Diamond: root RFC <- two stories <- one iteration.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-root.md",
        "---\ntitle: \"Root RFC\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-left.md",
        "---\ntitle: \"Left Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-right.md",
        "---\ntitle: \"Right Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-bottom.md",
        "---\ntitle: \"Bottom Iteration\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-left.md\n- implements: docs/stories/STORY-002-right.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "ITERATION-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let chain = parsed["chain"].as_array().unwrap();
    assert_eq!(chain.len(), 4);

    // Every chain element carries the implements_in_context edge field.
    for elem in chain {
        assert!(
            elem["implements_in_context"].is_array(),
            "each chain element must carry implements_in_context; got: {}",
            elem
        );
    }

    let edges_for = |title: &str| -> Vec<String> {
        chain.iter().find(|e| e["title"] == title).unwrap()["implements_in_context"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect()
    };

    // The iteration lists both story paths.
    let bottom_edges = edges_for("Bottom Iteration");
    assert_eq!(bottom_edges.len(), 2, "got: {:?}", bottom_edges);
    assert!(bottom_edges.iter().any(|p| p.contains("STORY-001-left")));
    assert!(bottom_edges.iter().any(|p| p.contains("STORY-002-right")));

    // Each story lists the root RFC.
    let left_edges = edges_for("Left Story");
    assert_eq!(left_edges.len(), 1, "got: {:?}", left_edges);
    assert!(left_edges[0].contains("RFC-001-root"));
    let right_edges = edges_for("Right Story");
    assert_eq!(right_edges.len(), 1, "got: {:?}", right_edges);
    assert!(right_edges[0].contains("RFC-001-root"));

    // Root has no in-graph parents.
    assert!(edges_for("Root RFC").is_empty());

    // target points at the requested doc's path.
    let target = parsed["target"].as_str().unwrap();
    assert!(
        target.contains("ITERATION-001-bottom"),
        "target should be the requested doc path; got: {}",
        target
    );
}

#[test]
fn context_cycle_terminates() {
    // A implements B, B implements A. resolve_chain must not hang and must
    // contain each document exactly once (topological order is undefined).
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-a.md",
        "---\ntitle: \"Doc A\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-002-b.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-b.md",
        "---\ntitle: \"Doc B\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-a.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "RFC-001", 1).unwrap();

    let mut titles = node_titles(&resolved);
    titles.sort();
    assert_eq!(
        titles,
        vec!["Doc A", "Doc B"],
        "cycle should yield each node exactly once"
    );
}

#[test]
fn context_human_tree_renders_every_node_in_cyclic_multiparent() {
    // A multi-parent graph that is also cyclic has no root (every node has an
    // in-graph parent). The tree render must still draw every node exactly once
    // rather than silently dropping the rootless component.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-a.md",
        "---\ntitle: \"Cycle Alpha\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-002-b.md\n- implements: docs/rfcs/RFC-003-c.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-b.md",
        "---\ntitle: \"Cycle Beta\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-a.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-003-c.md",
        "---\ntitle: \"Cycle Gamma\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-a.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "RFC-001", 1).unwrap();

    assert!(output.contains("Cycle Alpha"), "got: {}", output);
    assert!(output.contains("Cycle Beta"), "got: {}", output);
    assert!(output.contains("Cycle Gamma"), "got: {}", output);

    let marker = "\u{2190} you are here";
    assert_eq!(
        output.matches(marker).count(),
        1,
        "target should carry exactly one marker; got: {}",
        output
    );
}

#[test]
fn context_duplicate_implements_deduped() {
    // A document that declares the same `implements` target twice must record
    // that parent edge once, so it resolves as a normal single-parent chain.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-root.md",
        "---\ntitle: \"Root Spec\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-dup.md",
        "---\ntitle: \"Dup Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-root.md\n- implements: docs/rfcs/RFC-001-root.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 1).unwrap();

    assert_eq!(resolved.nodes.len(), 2, "got: {:?}", node_titles(&resolved));
    let story_node = resolved
        .nodes
        .iter()
        .find(|n| n.doc.title == "Dup Story")
        .unwrap();
    assert_eq!(
        story_node.parents.len(),
        1,
        "duplicate implements edge should be deduped; got: {:?}",
        story_node.parents
    );
}

#[test]
fn context_depth_two_surfaces_adr_via_rfc() {
    // STORY relates-to RFC (1 hop); ADR relates-to RFC (so RFC has a reverse
    // link from the ADR). At depth 2 the ADR surfaces with distance 2, reached
    // through the RFC path.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/stories/STORY-001-hub.md",
        "---\ntitle: \"Hub Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-spec.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-spec.md",
        "---\ntitle: \"Spec RFC\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/adrs/ADR-001-choice.md",
        "---\ntitle: \"Choice ADR\"\ntype: adr\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-spec.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 2).unwrap();

    let adr = resolved
        .related
        .iter()
        .find(|r| r.doc.title == "Choice ADR")
        .expect("ADR should surface at depth 2");
    assert_eq!(adr.distance, 2, "ADR is two hops from the chain");
    assert_eq!(
        adr.via,
        std::path::PathBuf::from("docs/rfcs/RFC-001-spec.md"),
        "ADR is reached through the RFC"
    );

    // The RFC itself is one hop out.
    let rfc = resolved
        .related
        .iter()
        .find(|r| r.doc.title == "Spec RFC")
        .expect("RFC should surface at depth 1");
    assert_eq!(rfc.distance, 1);
}

#[test]
fn context_depth_bounds_traversal() {
    // Chain (STORY) -A-> RFC -B-> ADR -C-> SPEC, all via related-to links.
    // SPEC is three hops out: absent at depth 2, present at depth 3.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/stories/STORY-001-hub.md",
        "---\ntitle: \"Hub Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-hop1.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-hop1.md",
        "---\ntitle: \"Hop One\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- related to: docs/adrs/ADR-001-hop2.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/adrs/ADR-001-hop2.md",
        "---\ntitle: \"Hop Two\"\ntype: adr\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- related to: docs/specs/SPEC-001-hop3.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/specs/SPEC-001-hop3.md",
        "---\ntitle: \"Hop Three\"\ntype: spec\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );

    let store = fixture.store();

    let at_two = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 2).unwrap();
    let two_titles: Vec<&str> = at_two
        .related
        .iter()
        .map(|r| r.doc.title.as_str())
        .collect();
    assert!(
        two_titles.contains(&"Hop One") && two_titles.contains(&"Hop Two"),
        "depth 2 reaches hops 1 and 2; got: {:?}",
        two_titles
    );
    assert!(
        !two_titles.contains(&"Hop Three"),
        "nothing beyond depth 2 may appear; got: {:?}",
        two_titles
    );

    let at_three = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 3).unwrap();
    let hop3 = at_three
        .related
        .iter()
        .find(|r| r.doc.title == "Hop Three")
        .expect("Hop Three should surface at depth 3");
    assert_eq!(hop3.distance, 3);
}

#[test]
fn context_json_related_tagged() {
    // STORY relates-to RFC (1 hop). ADR relates-to RFC, so the ADR surfaces at
    // distance 2 reached through the RFC. Each related entry must carry
    // relation/distance/via.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/stories/STORY-001-hub.md",
        "---\ntitle: \"Hub Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-spec.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-spec.md",
        "---\ntitle: \"Spec RFC\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/adrs/ADR-001-choice.md",
        "---\ntitle: \"Choice ADR\"\ntype: adr\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-spec.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "STORY-001", 2).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let related = parsed["related"].as_array().unwrap();
    assert!(!related.is_empty(), "related should be populated");

    // Every related entry carries the three tag fields.
    for entry in related {
        assert!(
            entry["relation"].is_string(),
            "relation key present: {}",
            entry
        );
        assert!(
            entry["distance"].is_u64(),
            "distance key present: {}",
            entry
        );
        assert!(entry["via"].is_string(), "via key present: {}", entry);
        assert_eq!(entry["relation"], "related-to");
    }

    let rfc = related
        .iter()
        .find(|e| e["title"] == "Spec RFC")
        .expect("RFC at distance 1");
    assert_eq!(rfc["distance"], 1);

    let adr = related
        .iter()
        .find(|e| e["title"] == "Choice ADR")
        .expect("ADR at distance 2");
    assert_eq!(adr["distance"], 2);
    assert_eq!(
        adr["via"].as_str().unwrap(),
        "docs/rfcs/RFC-001-spec.md",
        "ADR reached through the RFC path"
    );
}

#[test]
fn context_json_forward_tagged() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nRFC body.\n",
    );
    fixture.write_story(
        "STORY-001-impl-a.md",
        "Impl A",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );
    fixture.write_story(
        "STORY-002-impl-b.md",
        "Impl B",
        "draft",
        Some("docs/rfcs/RFC-001-auth.md"),
    );

    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "RFC-001", 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let forward = parsed["forward"].as_array().unwrap();
    assert_eq!(forward.len(), 2);
    for entry in forward {
        assert_eq!(entry["relation"], "implements", "got: {}", entry);
        assert_eq!(entry["distance"], 1, "forward is one hop: {}", entry);
        assert_eq!(
            entry["via"].as_str().unwrap(),
            "docs/rfcs/RFC-001-auth.md",
            "forward reached through the target path: {}",
            entry
        );
    }
}

#[test]
fn context_related_shortest_distance() {
    // The TARGET (a doc reachable both at 1 hop directly from the chain and at 2
    // hops via a sibling) must be recorded once with distance 1.
    //
    // STORY -> NEAR (1 hop), STORY -> SIBLING (1 hop), SIBLING -> NEAR (so NEAR
    // is also reachable in 2 hops). First discovery (distance 1) wins.
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/stories/STORY-001-hub.md",
        "---\ntitle: \"Hub Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-near.md\n- related to: docs/rfcs/RFC-002-sibling.md\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-near.md",
        "---\ntitle: \"Near Doc\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated: []\n---\n\nbody\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-sibling.md",
        "---\ntitle: \"Sibling Doc\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: []\nrelated:\n- related to: docs/rfcs/RFC-001-near.md\n---\n\nbody\n",
    );

    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001", 3).unwrap();

    let near: Vec<&lazyspec::cli::context::RelatedRef> = resolved
        .related
        .iter()
        .filter(|r| r.doc.title == "Near Doc")
        .collect();
    assert_eq!(
        near.len(),
        1,
        "doc reachable at two distances is recorded once"
    );
    assert_eq!(near[0].distance, 1, "shortest hop wins");
}
