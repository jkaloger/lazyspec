use crate::cli::json::doc_to_json;
use crate::cli::show::fetch_comments_for_doc;
use crate::cli::style::doc_card;
use crate::cli::style::type_header;
use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType};
use crate::engine::gh::GhIssueReader;
use crate::engine::status_colors::StatusColors;
use crate::engine::store::Store;
use std::path::Path;

pub fn run_json(store: &Store, config: &Config, root: &Path, gh: &dyn GhIssueReader) -> String {
    let docs: Vec<_> = store
        .all_docs()
        .iter()
        .map(|d| {
            let mut json = doc_to_json(d);
            json["comments"] =
                serde_json::Value::Array(fetch_comments_for_doc(d, config, root, gh));
            json
        })
        .collect();

    let result = store.validate_full(config);
    let errors: Vec<_> = result.errors.iter().map(|e| format!("{}", e)).collect();
    let warnings: Vec<_> = result.warnings.iter().map(|w| format!("{}", w)).collect();

    let parse_errors: Vec<_> = store
        .parse_errors()
        .iter()
        .map(|pe| serde_json::json!({ "path": pe.path.display().to_string(), "error": pe.error }))
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({
        "documents": docs,
        "validation": {
            "errors": errors,
            "warnings": warnings,
        },
        "parse_errors": parse_errors,
    }))
    .unwrap()
}

pub fn run_human(store: &Store) -> String {
    let mut all_docs = store.all_docs();
    if all_docs.is_empty() {
        return String::new();
    }

    all_docs.sort_by(|a, b| DocMeta::sort_by_date(a, b));

    let colors = StatusColors::load(store.root()).unwrap_or_default();
    let mut output = String::new();
    let type_order = [
        DocType::new(DocType::RFC),
        DocType::new(DocType::STORY),
        DocType::new(DocType::ITERATION),
        DocType::new(DocType::ADR),
    ];
    let mut first = true;

    for dt in &type_order {
        let group: Vec<_> = all_docs.iter().filter(|d| &d.doc_type == dt).collect();
        if group.is_empty() {
            continue;
        }

        if !first {
            output.push('\n');
        }
        first = false;

        output.push_str(&type_header(dt));
        output.push('\n');
        for doc in &group {
            output.push_str(&format!(
                "  {}\n",
                doc_card(&colors, &doc.title, &doc.doc_type, &doc.status, &doc.path)
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, StoreBackend, TypeDef};
    use crate::engine::gh::test_support::MockGhClient;
    use crate::engine::gh::GhComment;
    use crate::engine::issue_map::IssueMap;
    use tempfile::TempDir;

    fn github_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![TypeDef::test_fixture("story", StoreBackend::GithubIssues)];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config
    }

    fn write_cache_doc(root: &std::path::Path, id: &str) {
        let dir = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&dir).unwrap();
        let content =
            "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-06-25\ntags: []\n---\nBody.\n";
        std::fs::write(dir.join(format!("{}.md", id)), content).unwrap();
    }

    // AC2: status --json surfaces each fetched comment per document with
    // author/body/timestamp.
    #[test]
    fn status_json_includes_comments_per_doc() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_cache_doc(root, "STORY-001-my-story");

        let config = github_config();
        let store = Store::load(root, &config).unwrap();

        let mut map = IssueMap::load(root).unwrap();
        for doc in store.all_docs() {
            map.insert(doc.id.clone(), 7, "ts", "");
        }
        map.save(root).unwrap();

        let gh = MockGhClient::new().with_comments(vec![
            GhComment {
                author: "alice".to_string(),
                body: "first".to_string(),
                timestamp: "2026-06-01T00:00:00Z".to_string(),
            },
            GhComment {
                author: "bob".to_string(),
                body: "second".to_string(),
                timestamp: "2026-06-02T00:00:00Z".to_string(),
            },
        ]);

        let out = run_json(&store, &config, root, &gh);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let docs = value["documents"].as_array().unwrap();
        assert!(!docs.is_empty());
        let comments = docs[0]["comments"].as_array().unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0]["author"], "alice");
        assert_eq!(comments[0]["body"], "first");
        assert_eq!(comments[0]["timestamp"], "2026-06-01T00:00:00Z");
    }

    // Wiring: filesystem-backed docs always carry an (empty) comments array and
    // never trigger a fetch.
    #[test]
    fn status_json_filesystem_doc_has_empty_comments() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let mut config = Config::default();
        let mut td = TypeDef::test_fixture("story", StoreBackend::Filesystem);
        td.dir = "docs/stories".to_string();
        config.documents.types = vec![td];

        let dir = root.join("docs/stories");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("STORY-001-my-story.md"),
            "---\ntitle: S\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-06-25\ntags: []\n---\nBody.\n",
        )
        .unwrap();

        let store = Store::load(root, &config).unwrap();
        let gh = MockGhClient::new();
        let out = run_json(&store, &config, root, &gh);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let docs = value["documents"].as_array().unwrap();
        assert!(!docs.is_empty());
        assert!(docs[0]["comments"].as_array().unwrap().is_empty());
        assert_eq!(gh.comments_call_count.get(), 0);
    }
}
