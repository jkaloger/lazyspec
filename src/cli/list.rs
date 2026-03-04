use crate::engine::document::DocType;
use crate::engine::store::{Filter, Store};

pub fn run(store: &Store, doc_type: Option<&str>, status: Option<&str>, json: bool) {
    let filter = Filter {
        doc_type: doc_type.and_then(|t| match t.to_lowercase().as_str() {
            "rfc" => Some(DocType::Rfc),
            "adr" => Some(DocType::Adr),
            "spec" => Some(DocType::Spec),
            "plan" => Some(DocType::Plan),
            _ => None,
        }),
        status: status.and_then(|s| match s.to_lowercase().as_str() {
            "draft" => Some(crate::engine::document::Status::Draft),
            "review" => Some(crate::engine::document::Status::Review),
            "accepted" => Some(crate::engine::document::Status::Accepted),
            "rejected" => Some(crate::engine::document::Status::Rejected),
            "superseded" => Some(crate::engine::document::Status::Superseded),
            _ => None,
        }),
        ..Default::default()
    };

    let docs = store.list(&filter);

    if json {
        let items: Vec<_> = docs
            .iter()
            .map(|d| {
                serde_json::json!({
                    "path": d.path.to_string_lossy(),
                    "title": d.title,
                    "type": format!("{}", d.doc_type),
                    "status": format!("{}", d.status),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        for doc in docs {
            println!(
                "{:<40} {:<12} {}",
                doc.title,
                doc.status,
                doc.path.display()
            );
        }
    }
}
