use crate::cli::json::doc_to_json;
use crate::cli::style::doc_card;
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
        let output = json_output(&docs);
        println!("{}", output);
    } else {
        for doc in docs {
            println!(
                "{}",
                doc_card(&doc.title, &doc.doc_type, &doc.status, &doc.path)
            );
        }
    }
}

pub fn run_json(store: &Store, doc_type: Option<&str>, status: Option<&str>) -> String {
    let docs = store.list(&build_filter(doc_type, status));
    json_output(&docs)
}

fn json_output(docs: &[&crate::engine::document::DocMeta]) -> String {
    let items: Vec<_> = docs.iter().map(|d| doc_to_json(d)).collect();
    serde_json::to_string_pretty(&items).unwrap()
}
