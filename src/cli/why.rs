use crate::cli::style::{dim, doc_card};
use crate::engine::document::DocMeta;
use crate::engine::status_colors::StatusColors;
use crate::engine::store::Store;
use serde_json::Value;
use std::path::Path;

/// One result: the document and the glob of its that matched, in the shape
/// RFC-068 specifies. Deliberately narrower than [`crate::cli::json::doc_to_json`]
/// -- `why` answers "which spec applies here", so a caller wants the id to read
/// next and the pin that put it in the list, not the whole frontmatter.
fn entry(doc: &DocMeta, glob: &str) -> Value {
    serde_json::json!({
        "id": doc.id,
        "type": format!("{}", doc.doc_type).to_lowercase(),
        "title": doc.title,
        "status": format!("{}", doc.status),
        "reviewed": doc.reviewed,
        "glob": glob,
    })
}

pub fn run_json(store: &Store, path: &Path) -> String {
    let items: Vec<Value> = store
        .governing(path)
        .into_iter()
        .map(|(doc, glob)| entry(doc, glob))
        .collect();
    serde_json::to_string_pretty(&items).unwrap()
}

fn human_output(store: &Store, path: &Path) -> String {
    let matches = store.governing(path);
    if matches.is_empty() {
        return format!("No documents govern {}\n", path.display());
    }
    let colors = StatusColors::load(store.root()).unwrap_or_default();
    matches
        .into_iter()
        .map(|(doc, glob)| {
            format!(
                "{} {}\n",
                doc_card(
                    &colors,
                    &doc.title,
                    &doc.doc_type,
                    &doc.status,
                    doc.assignee.as_deref(),
                    &doc.path
                ),
                dim(&format!("[{}]", glob)),
            )
        })
        .collect()
}

pub fn run(store: &Store, path: &Path, json: bool) {
    if json {
        println!("{}", run_json(store, path));
    } else {
        print!("{}", human_output(store, path));
    }
}
