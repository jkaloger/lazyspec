use crate::cli::json::doc_to_json;
use crate::engine::document::DocType;
use crate::engine::store::{SearchResult, Store};

fn filter_results<'a>(results: &mut Vec<SearchResult<'a>>, doc_type: Option<&str>) {
    if let Some(dt) = doc_type {
        let filter_type = match dt.to_lowercase().as_str() {
            "rfc" => Some(DocType::Rfc),
            "adr" => Some(DocType::Adr),
            "story" => Some(DocType::Story),
            "iteration" => Some(DocType::Iteration),
            _ => None,
        };
        if let Some(ft) = filter_type {
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
            json
        })
        .collect();
    serde_json::to_string_pretty(&items).unwrap()
}

pub fn run(store: &Store, query: &str, doc_type: Option<&str>, json: bool) {
    let mut results = store.search(query);
    filter_results(&mut results, doc_type);

    if json {
        println!("{}", json_output(&results));
    } else {
        if results.is_empty() {
            println!("No results for \"{}\"", query);
            return;
        }
        for r in &results {
            println!(
                "{:<40} {:<10} {:<12} [{}]",
                r.doc.title,
                r.doc.doc_type,
                r.doc.status,
                r.match_field,
            );
            println!("  {}", r.doc.path.display());
            println!("  ...{}...", r.snippet.trim());
            println!();
        }
    }
}

pub fn run_json(store: &Store, query: &str, doc_type: Option<&str>) -> String {
    let mut results = store.search(query);
    filter_results(&mut results, doc_type);
    json_output(&results)
}
