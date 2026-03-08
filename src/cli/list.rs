use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{dim, doc_card};
use crate::engine::store::{Filter, Store};

fn build_filter(doc_type: Option<&str>, status: Option<&str>) -> Filter {
    Filter {
        doc_type: doc_type.and_then(|t| t.parse().ok()),
        status: status.and_then(|s| s.parse().ok()),
        ..Default::default()
    }
}

pub fn run(store: &Store, doc_type: Option<&str>, status: Option<&str>, json: bool) {
    let docs = store.list(&build_filter(doc_type, status));

    if json {
        let output = json_output(&docs, store);
        println!("{}", output);
    } else {
        for doc in docs {
            let card = doc_card(&doc.title, &doc.doc_type, &doc.status, &doc.path);
            if let Some(parent_path) = store.parent_of(&doc.path) {
                let parent_title = store
                    .get(parent_path)
                    .map(|p| p.title.as_str())
                    .unwrap_or("unknown");
                println!("{}  {}", card, dim(&format!("(child of {})", parent_title)));
            } else {
                println!("{}", card);
            }
        }
    }
}

pub fn run_json(store: &Store, doc_type: Option<&str>, status: Option<&str>) -> String {
    let docs = store.list(&build_filter(doc_type, status));
    json_output(&docs, store)
}

fn json_output(docs: &[&crate::engine::document::DocMeta], store: &Store) -> String {
    let items: Vec<_> = docs.iter().map(|d| doc_to_json_with_family(d, store)).collect();
    serde_json::to_string_pretty(&items).unwrap()
}
