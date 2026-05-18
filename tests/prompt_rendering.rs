mod common;

use std::fs;

use common::TestFixture;
use lazyspec::engine::prompt::{
    iterations_implementing, prior_iterations, DocSummary, MinijinjaPromptRenderer, PromptRenderer,
};

fn doc_fixture() -> DocSummary {
    DocSummary {
        id: "STORY-X".to_string(),
        title: "Real Title".to_string(),
        body: "Body text".to_string(),
        status: "in_progress".to_string(),
        assignees: vec!["alice".to_string(), "bob".to_string()],
    }
}

#[test]
fn render_prompt_against_real_builder_template() {
    let fixture = TestFixture::new();
    let prompt_dir = fixture.root().join(".lazyspec/prompts");
    fs::create_dir_all(&prompt_dir).unwrap();
    let prompt_path = prompt_dir.join("builder.md");

    // Minimal valid template using the same variables as the shipped
    // .lazyspec/prompts/builder.md.
    let template = "\
ID: {{ doc.id }}
Title: {{ doc.title }}
Status: {{ doc.status }}
Assignees: {% for a in doc.assignees %}{{ a }}{% if not loop.last %}, {% endif %}{% endfor %}

{{ doc.body }}

{% if attempt is none %}FIRST{% else %}TURN {{ attempt }}{% endif %}

{% if prior_iterations %}{% for it in prior_iterations %}- {{ it }}
{% endfor %}{% else %}NO PRIOR{% endif %}
";
    fs::write(&prompt_path, template).unwrap();

    let renderer = MinijinjaPromptRenderer { prompt_path };
    let out = renderer.render(&doc_fixture(), None, &[]).unwrap();

    assert!(!out.is_empty(), "rendered output should not be empty");
    assert!(out.contains("STORY-X"), "doc id missing from output: {out}");
    assert!(out.contains("Real Title"), "title missing: {out}");
    assert!(out.contains("alice"), "assignee missing: {out}");
    assert!(out.contains("FIRST"), "attempt-none branch missing: {out}");
    assert!(
        out.contains("NO PRIOR"),
        "prior_iterations branch missing: {out}"
    );
}

#[test]
fn prior_iterations_across_real_store_load() {
    let fixture = TestFixture::new();
    fixture.write_story("STORY-X-test.md", "Test Story", "draft", None);
    fixture.write_iteration("ITERATION-001-a.md", "A", "draft", Some("STORY-X"));
    fixture.write_iteration("ITERATION-002-b.md", "B", "draft", Some("STORY-X"));
    fixture.write_iteration("ITERATION-003-c.md", "C", "draft", Some("STORY-Y"));

    let store = fixture.store();

    let current = iterations_implementing(&store, "STORY-X");
    assert_eq!(
        current,
        vec!["ITERATION-001".to_string(), "ITERATION-002".to_string()],
        "iterations_implementing should return STORY-X children sorted"
    );

    let snapshot = vec!["ITERATION-001".to_string()];
    let prior = prior_iterations(&current, &snapshot);
    assert_eq!(prior, vec!["ITERATION-002".to_string()]);
}
