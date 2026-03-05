use crate::cli::json::doc_to_json;
use crate::cli::style::{doc_card, type_header};
use crate::engine::document::DocType;
use crate::engine::store::Store;

pub fn run_json(store: &Store) -> String {
    let docs: Vec<_> = store.all_docs().iter().map(|d| doc_to_json(d)).collect();

    let result = store.validate_full();
    let errors: Vec<_> = result.errors.iter().map(|e| format!("{}", e)).collect();
    let warnings: Vec<_> = result.warnings.iter().map(|w| format!("{}", w)).collect();

    serde_json::to_string_pretty(&serde_json::json!({
        "documents": docs,
        "validation": {
            "errors": errors,
            "warnings": warnings,
        }
    }))
    .unwrap()
}

pub fn run_human(store: &Store) -> String {
    let mut all_docs = store.all_docs();
    if all_docs.is_empty() {
        return String::new();
    }

    all_docs.sort_by(|a, b| a.path.cmp(&b.path));

    let mut output = String::new();
    let type_order = [DocType::Rfc, DocType::Story, DocType::Iteration, DocType::Adr];
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
                doc_card(&doc.title, &doc.doc_type, &doc.status, &doc.path)
            ));
        }
    }

    output
}
