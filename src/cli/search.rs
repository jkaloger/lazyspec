use crate::engine::document::DocType;
use crate::engine::store::Store;

pub fn run(store: &Store, query: &str, doc_type: Option<&str>, json: bool) {
    let mut results = store.search(query);

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

    if json {
        let items: Vec<_> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "path": r.doc.path.to_string_lossy(),
                    "title": r.doc.title,
                    "type": format!("{}", r.doc.doc_type),
                    "status": format!("{}", r.doc.status),
                    "match_field": r.match_field,
                    "snippet": r.snippet,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
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
