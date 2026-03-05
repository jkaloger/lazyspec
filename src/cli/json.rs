use crate::engine::document::DocMeta;
use serde_json::Value;

pub fn doc_to_json(doc: &DocMeta) -> Value {
    serde_json::json!({
        "path": doc.path.to_string_lossy(),
        "title": doc.title,
        "type": format!("{}", doc.doc_type).to_lowercase(),
        "status": format!("{}", doc.status),
        "author": doc.author,
        "date": doc.date.to_string(),
        "tags": doc.tags,
        "related": doc.related.iter().map(|r| {
            serde_json::json!({
                "type": format!("{}", r.rel_type),
                "target": r.target,
            })
        }).collect::<Vec<_>>(),
    })
}
