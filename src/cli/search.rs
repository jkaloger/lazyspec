use crate::cli::json::doc_to_json;
use crate::cli::style::{dim, doc_card};
use crate::engine::document::DocType;
use crate::engine::fs::FileSystem;
use crate::engine::status_colors::StatusColors;
use crate::engine::store::{SearchResult, Store};

fn filter_results<'a>(results: &mut Vec<SearchResult<'a>>, doc_type: Option<&str>) {
    if let Some(dt) = doc_type {
        if let Ok(ft) = dt.parse::<DocType>() {
            results.retain(|r| r.doc.doc_type == ft);
        }
    }
}

fn json_output(results: &[SearchResult]) -> String {
    let items: Vec<_> = results
        .iter()
        .map(|r| {
            let mut json = doc_to_json(r.doc);
            json["match_field"] = serde_json::Value::String(r.match_field.to_string());
            json["snippet"] = serde_json::Value::String(r.snippet.clone());
            json["score"] = serde_json::Value::from(r.score);
            json
        })
        .collect();
    serde_json::to_string_pretty(&items).unwrap()
}

// `results` arrives already score-descending from `Store::search`; this only
// formats, it never re-sorts or caps.
fn human_output(store: &Store, query: &str, results: &[SearchResult]) -> String {
    if results.is_empty() {
        return format!("No results for \"{}\"\n", query);
    }
    let colors = StatusColors::load(store.root()).unwrap_or_default();
    let mut output = String::new();
    for r in results {
        output.push_str(&format!(
            "{} {}\n",
            doc_card(
                &colors,
                &r.doc.title,
                &r.doc.doc_type,
                &r.doc.status,
                r.doc.assignee.as_deref(),
                &r.doc.path
            ),
            dim(&format!("[{}]", r.match_field)),
        ));
        output.push_str(&format!(
            "  {}\n",
            dim(&format!("...{}...", r.snippet.trim()))
        ));
        output.push('\n');
    }
    output
}

pub fn run(store: &Store, query: &str, doc_type: Option<&str>, json: bool, fs: &dyn FileSystem) {
    let mut results = store.search(query, fs);
    filter_results(&mut results, doc_type);

    if json {
        println!("{}", json_output(&results));
    } else {
        print!("{}", human_output(store, query, &results));
    }
}

pub fn run_json(store: &Store, query: &str, doc_type: Option<&str>, fs: &dyn FileSystem) -> String {
    let mut results = store.search(query, fs);
    filter_results(&mut results, doc_type);
    json_output(&results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use crate::engine::fs::RealFileSystem;
    use tempfile::TempDir;

    fn write_doc(
        root: &std::path::Path,
        dir: &str,
        filename: &str,
        doc_type: &str,
        title: &str,
        body: &str,
    ) {
        let full_dir = root.join(dir);
        std::fs::create_dir_all(&full_dir).unwrap();
        std::fs::write(
            full_dir.join(filename),
            format!(
                "---\ntitle: \"{}\"\ntype: {}\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n{}\n",
                title, doc_type, body
            ),
        )
        .unwrap();
    }

    fn write_rfc(root: &std::path::Path, filename: &str, title: &str, body: &str) {
        write_doc(root, "docs/rfcs", filename, "rfc", title, body);
    }

    fn load_store(root: &std::path::Path) -> Store {
        Store::load(root, &Config::default()).unwrap()
    }

    // AC1: `search --json` includes a numeric `score` field per result.
    #[test]
    fn json_output_includes_numeric_score() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_rfc(root, "RFC-001-fuzzy.md", "fuzzy matcher", "body");
        let store = load_store(root);

        let output = run_json(&store, "fuzzy", None, &RealFileSystem);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 1);
        assert!(parsed[0]["score"].is_number());
        assert!(parsed[0]["score"].as_u64().unwrap() > 0);
    }

    // AC2: the `--json` array is ordered score-descending.
    #[test]
    fn run_json_orders_results_by_score_descending() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Non-contiguous, weaker match at the earlier-sorting path so ordering
        // can only come from score, not the path tie-break.
        write_rfc(root, "RFC-001-weak.md", "xfxuxzxzxyx", "body");
        write_rfc(root, "RFC-002-strong.md", "fuzzy matcher", "body");
        let store = load_store(root);

        let output = run_json(&store, "fuzzy", None, &RealFileSystem);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["title"], "fuzzy matcher");
        let s0 = parsed[0]["score"].as_u64().unwrap();
        let s1 = parsed[1]["score"].as_u64().unwrap();
        assert!(s0 > s1);
    }

    // AC3: human-readable mode prints results in the same score-descending order.
    #[test]
    fn human_output_orders_results_by_score_descending() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_rfc(root, "RFC-001-weak.md", "xfxuxzxzxyx", "body");
        write_rfc(root, "RFC-002-strong.md", "fuzzy matcher", "body");
        let store = load_store(root);

        let mut results = store.search("fuzzy", &RealFileSystem);
        filter_results(&mut results, None);
        let out = human_output(&store, "fuzzy", &results);

        let strong_idx = out.find("fuzzy matcher").expect("strong match printed");
        let weak_idx = out.find("xfxuxzxzxyx").expect("weak match printed");
        assert!(
            strong_idx < weak_idx,
            "higher-scoring result should print first"
        );
    }

    // AC4: `--type` filters to the requested type and the survivors stay
    // score-descending.
    #[test]
    fn type_filter_keeps_score_descending_order() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_rfc(root, "RFC-001-weak.md", "xfxuxzxzxyx", "body");
        write_rfc(root, "RFC-002-strong.md", "fuzzy matcher", "body");
        write_doc(
            root,
            "docs/adrs",
            "ADR-001-fuzzy.md",
            "adr",
            "fuzzy adr",
            "body",
        );
        let store = load_store(root);

        let output = run_json(&store, "fuzzy", Some("rfc"), &RealFileSystem);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().all(|d| d["type"] == "rfc"));
        let s0 = parsed[0]["score"].as_u64().unwrap();
        let s1 = parsed[1]["score"].as_u64().unwrap();
        assert!(s0 >= s1);
    }

    // AC5: the CLI applies no result cap; every matching document is present.
    #[test]
    fn run_json_applies_no_result_cap() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        for i in 0..30 {
            write_rfc(
                root,
                &format!("RFC-{:03}-fuzzy.md", i),
                &format!("fuzzy item {}", i),
                "body",
            );
        }
        let store = load_store(root);

        let output = run_json(&store, "fuzzy", None, &RealFileSystem);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed.len(), 30, "every match should be present, no cap");
    }

    // AC6: `match_field`/`snippet` keep reporting the pre-scoring values.
    #[test]
    fn match_field_and_snippet_unchanged_by_scoring() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_rfc(
            root,
            "RFC-001-fuzzy.md",
            "unrelated title",
            "a fuzzy body match here",
        );
        let store = load_store(root);

        let output = run_json(&store, "fuzzy", None, &RealFileSystem);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed[0]["match_field"], "body");
        assert!(parsed[0]["snippet"].as_str().unwrap().contains("fuzzy"));
        assert!(parsed[0]["score"].is_number());
    }
}
