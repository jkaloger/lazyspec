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
    let chain = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001").unwrap();

    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].title, "Auth Redesign");
    assert_eq!(chain[1].title, "Auth Implementation");
    assert_eq!(chain[2].title, "Auth Sprint 1");
}

#[test]
fn context_standalone_document() {
    let fixture = setup();
    let store = fixture.store();
    let chain = lazyspec::cli::context::resolve_chain(&store, "RFC-001").unwrap();

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].title, "Auth Redesign");
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
