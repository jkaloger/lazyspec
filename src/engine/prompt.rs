use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::engine::document::{DocType, RelationType};
use crate::engine::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct DocSummary {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub assignees: Vec<String>,
}

pub fn render_prompt(
    template: &str,
    doc: &DocSummary,
    attempt: Option<u32>,
    prior_iterations: &[String],
) -> Result<String> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    env.add_template("prompt", template)?;
    let tmpl = env.get_template("prompt")?;
    tmpl.render(minijinja::context! {
        doc => doc,
        attempt => attempt,
        prior_iterations => prior_iterations,
    })
    .map_err(Into::into)
}

pub trait PromptRenderer: Send + Sync {
    fn render(
        &self,
        doc: &DocSummary,
        attempt: Option<u32>,
        prior_iterations: &[String],
    ) -> Result<String>;
}

pub struct MinijinjaPromptRenderer {
    pub prompt_path: PathBuf,
}

impl PromptRenderer for MinijinjaPromptRenderer {
    fn render(
        &self,
        doc: &DocSummary,
        attempt: Option<u32>,
        prior_iterations: &[String],
    ) -> Result<String> {
        let template = std::fs::read_to_string(&self.prompt_path)
            .with_context(|| format!("read prompt {}", self.prompt_path.display()))?;
        render_prompt(&template, doc, attempt, prior_iterations)
    }
}

/// Iteration ids currently in the store that implement `doc_id`.
/// Result is sorted lexically for determinism.
pub fn iterations_implementing(store: &Store, doc_id: &str) -> Vec<String> {
    let iteration_type = DocType::new(DocType::ITERATION);
    let mut ids: Vec<String> = store
        .all_docs()
        .into_iter()
        .filter(|d| d.doc_type == iteration_type)
        .filter(|d| {
            d.related
                .iter()
                .any(|r| r.rel_type == RelationType::Implements && r.target == doc_id)
        })
        .map(|d| d.id.clone())
        .collect();
    ids.sort();
    ids
}

/// Set diff: current iterations not in the session-start snapshot.
/// Order preserved from `current` (which is expected to be sorted).
pub fn prior_iterations(current: &[String], snapshot: &[String]) -> Vec<String> {
    let snap: HashSet<&str> = snapshot.iter().map(String::as_str).collect();
    current
        .iter()
        .filter(|id| !snap.contains(id.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn doc_fixture() -> DocSummary {
        DocSummary {
            id: "STORY-X".to_string(),
            title: "t".to_string(),
            body: "b".to_string(),
            status: "draft".to_string(),
            assignees: vec!["alice".to_string()],
        }
    }

    #[test]
    fn render_substitutes_doc_fields() {
        let template = "id={{ doc.id }} title={{ doc.title }} body={{ doc.body }} \
                        status={{ doc.status }} assignees={{ doc.assignees[0] }}";
        let out = render_prompt(template, &doc_fixture(), None, &[]).unwrap();
        assert!(out.contains("id=STORY-X"), "got: {out}");
        assert!(out.contains("title=t"), "got: {out}");
        assert!(out.contains("body=b"), "got: {out}");
        assert!(out.contains("status=draft"), "got: {out}");
        assert!(out.contains("assignees=alice"), "got: {out}");
    }

    #[test]
    fn render_substitutes_attempt_some() {
        let out = render_prompt("{{ attempt }}", &doc_fixture(), Some(3), &[]).unwrap();
        assert!(out.contains('3'), "got: {out}");
    }

    #[test]
    fn render_attempt_none_branch_renders_first() {
        let template = "{% if attempt is none %}FIRST{% else %}CONT{% endif %}";
        let out = render_prompt(template, &doc_fixture(), None, &[]).unwrap();
        assert_eq!(out, "FIRST");
    }

    #[test]
    fn render_attempt_some_branch_renders_cont() {
        let template = "{% if attempt is none %}FIRST{% else %}CONT{% endif %}";
        let out = render_prompt(template, &doc_fixture(), Some(2), &[]).unwrap();
        assert_eq!(out, "CONT");
    }

    #[test]
    fn render_prior_iterations_loop() {
        let template = "{% for it in prior_iterations %}{{ it }}|{% endfor %}";
        let prior = vec!["ITER-1".to_string(), "ITER-2".to_string()];
        let out = render_prompt(template, &doc_fixture(), None, &prior).unwrap();
        assert_eq!(out, "ITER-1|ITER-2|");
    }

    #[test]
    fn render_prior_iterations_empty_branch_renders_none() {
        let template = "{% if prior_iterations %}HAS{% else %}NONE{% endif %}";
        let out = render_prompt(template, &doc_fixture(), None, &[]).unwrap();
        assert_eq!(out, "NONE");
    }

    #[test]
    fn render_prior_iterations_non_empty_branch_renders_has() {
        let template = "{% if prior_iterations %}HAS{% else %}NONE{% endif %}";
        let prior = vec!["ITER-1".to_string()];
        let out = render_prompt(template, &doc_fixture(), None, &prior).unwrap();
        assert_eq!(out, "HAS");
    }

    #[test]
    fn render_fails_on_undefined_variable() {
        let err = render_prompt("{{ ghost }}", &doc_fixture(), None, &[]).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("undefined"),
            "error should indicate undefined value, got: {msg}"
        );
    }

    #[test]
    fn render_fails_on_syntax_error() {
        let result = render_prompt("{{ unterminated", &doc_fixture(), None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn minijinja_renderer_reads_template_from_disk() {
        let dir = TempDir::new().unwrap();
        let prompt_path = dir.path().join("builder.md");
        fs::write(&prompt_path, "id={{ doc.id }}").unwrap();
        let renderer = MinijinjaPromptRenderer { prompt_path };
        let out = renderer.render(&doc_fixture(), None, &[]).unwrap();
        assert!(out.contains("id=STORY-X"), "got: {out}");
    }

    #[test]
    fn minijinja_renderer_returns_err_when_template_missing() {
        let dir = TempDir::new().unwrap();
        let prompt_path = dir.path().join("missing.md");
        let renderer = MinijinjaPromptRenderer { prompt_path };
        let err = renderer.render(&doc_fixture(), None, &[]).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("read prompt"),
            "error should mention 'read prompt', got: {msg}"
        );
    }

    #[test]
    fn minijinja_renderer_returns_err_on_strict_undefined() {
        let dir = TempDir::new().unwrap();
        let prompt_path = dir.path().join("builder.md");
        fs::write(&prompt_path, "{{ ghost }}").unwrap();
        let renderer = MinijinjaPromptRenderer { prompt_path };
        let result = renderer.render(&doc_fixture(), None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn prior_iterations_excludes_snapshot() {
        let current = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let snapshot = vec!["A".to_string()];
        assert_eq!(prior_iterations(&current, &snapshot), vec!["B", "C"]);
    }

    #[test]
    fn prior_iterations_empty_when_current_equals_snapshot() {
        let current = vec!["A".to_string(), "B".to_string()];
        let snapshot = vec!["A".to_string(), "B".to_string()];
        assert!(prior_iterations(&current, &snapshot).is_empty());
    }

    #[test]
    fn prior_iterations_returns_all_when_snapshot_empty() {
        let current = vec!["A".to_string(), "B".to_string()];
        assert_eq!(prior_iterations(&current, &[]), vec!["A", "B"]);
    }

    #[test]
    fn prior_iterations_returns_empty_when_current_empty() {
        let snapshot = vec!["A".to_string()];
        assert!(prior_iterations(&[], &snapshot).is_empty());
        assert!(prior_iterations(&[], &[]).is_empty());
    }

    fn write_iteration(root: &std::path::Path, id: &str, implements: &str) {
        let path = root.join(format!("docs/iterations/{}-foo.md", id));
        let content = format!(
            "---\ntitle: \"{id}\"\ntype: iteration\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: {implements}\n---\nbody\n"
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }

    fn write_story(root: &std::path::Path, id: &str) {
        let path = root.join(format!("docs/stories/{}-foo.md", id));
        let content = format!(
            "---\ntitle: \"{id}\"\ntype: story\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\nbody\n"
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }

    #[test]
    fn iterations_implementing_filters_by_type_and_relation() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write_iteration(root, "ITERATION-001", "STORY-X");
        write_iteration(root, "ITERATION-002", "STORY-Y");
        write_iteration(root, "ITERATION-003", "STORY-X");
        write_story(root, "STORY-Z");

        let config = crate::engine::config::Config::default();
        let store = Store::load(root, &config).unwrap();

        assert_eq!(
            iterations_implementing(&store, "STORY-X"),
            vec!["ITERATION-001", "ITERATION-003"]
        );
    }

    #[test]
    fn iterations_implementing_empty_when_no_match() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write_iteration(root, "ITERATION-001", "STORY-X");
        write_iteration(root, "ITERATION-002", "STORY-Y");

        let config = crate::engine::config::Config::default();
        let store = Store::load(root, &config).unwrap();

        assert!(iterations_implementing(&store, "STORY-Z").is_empty());
    }

    #[test]
    fn iterations_implementing_sorted_output() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write_iteration(root, "ITERATION-099", "STORY-X");
        write_iteration(root, "ITERATION-007", "STORY-X");
        write_iteration(root, "ITERATION-042", "STORY-X");

        let config = crate::engine::config::Config::default();
        let store = Store::load(root, &config).unwrap();

        assert_eq!(
            iterations_implementing(&store, "STORY-X"),
            vec!["ITERATION-007", "ITERATION-042", "ITERATION-099"]
        );
    }

    #[test]
    fn minijinja_renderer_rereads_template_each_call() {
        let dir = TempDir::new().unwrap();
        let prompt_path = dir.path().join("builder.md");
        fs::write(&prompt_path, "v1 {{ doc.id }}").unwrap();
        let renderer = MinijinjaPromptRenderer {
            prompt_path: prompt_path.clone(),
        };
        let out1 = renderer.render(&doc_fixture(), None, &[]).unwrap();
        assert!(out1.contains("v1"), "got: {out1}");

        fs::write(&prompt_path, "v2 {{ doc.id }}").unwrap();
        let out2 = renderer.render(&doc_fixture(), None, &[]).unwrap();
        assert!(out2.contains("v2"), "got: {out2}");
        assert!(!out2.contains("v1"), "stale template cached, got: {out2}");
    }
}
