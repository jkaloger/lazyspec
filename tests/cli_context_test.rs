mod common;

use common::TestFixture;

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
    let resolved = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001").unwrap();

    assert_eq!(resolved.chain.len(), 3);
    assert_eq!(resolved.chain[0].title, "Auth Redesign");
    assert_eq!(resolved.chain[1].title, "Auth Implementation");
    assert_eq!(resolved.chain[2].title, "Auth Sprint 1");
    assert_eq!(resolved.target_index, 2);
}

#[test]
fn context_standalone_document() {
    let fixture = setup();
    let store = fixture.store();
    let resolved = lazyspec::cli::context::resolve_chain(&store, "RFC-001").unwrap();

    assert_eq!(resolved.chain.len(), 1);
    assert_eq!(resolved.chain[0].title, "Auth Redesign");
    assert_eq!(resolved.target_index, 0);
}

#[test]
fn context_json_output() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "ITERATION-001").unwrap();
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
    let output = lazyspec::cli::context::run_human(&store, "ITERATION-001").unwrap();

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
    let result = lazyspec::cli::context::resolve_chain(&store, "NONEXISTENT-999");
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
    let resolved = lazyspec::cli::context::resolve_chain(&store, "RFC-001").unwrap();

    assert_eq!(resolved.chain.len(), 1);
    assert_eq!(resolved.forward.len(), 2);
    let forward_titles: Vec<&str> = resolved.forward.iter().map(|d| d.title.as_str()).collect();
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
    let resolved = lazyspec::cli::context::resolve_chain(&store, "STORY-001").unwrap();

    assert_eq!(resolved.chain.len(), 2);
    assert_eq!(resolved.chain[0].title, "Auth Redesign");
    assert_eq!(resolved.chain[1].title, "Auth Implementation");
    assert_eq!(resolved.forward.len(), 2);
    let forward_titles: Vec<&str> = resolved.forward.iter().map(|d| d.title.as_str()).collect();
    assert!(forward_titles.contains(&"Sprint 1"));
    assert!(forward_titles.contains(&"Sprint 2"));
}

#[test]
fn you_are_here_marker() {
    let fixture = setup();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_human(&store, "STORY-001").unwrap();

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
    let output = lazyspec::cli::context::run_human(&store, "STORY-001").unwrap();

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
    let output = lazyspec::cli::context::run_human(&store, "STORY-001").unwrap();

    assert!(
        !output.contains("related"),
        "output should not contain 'related' when there are no related-to links"
    );
}

#[test]
fn json_related_field_present() {
    let fixture = setup_with_related();
    let store = fixture.store();
    let output = lazyspec::cli::context::run_json(&store, "STORY-001").unwrap();
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
    let output = lazyspec::cli::context::run_json(&store, "STORY-001").unwrap();
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
    let resolved = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001").unwrap();

    assert!(
        resolved.forward.is_empty(),
        "leaf node should have no forward children"
    );
}
