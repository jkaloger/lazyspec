//! The `show --json` document shape, shared by the CLI's renders and the
//! payload a hook reads on stdin (RFC-075), so the two cannot drift apart.

use crate::engine::document::{AttrValue, DocMeta};
use crate::engine::store_dispatch::percent_complete;
use serde_json::Value;

/// Read a milestone's `open_issues`/`closed_issues` count attributes (set when a
/// milestone document is materialized) and compute progress. `None` for any doc
/// without both counts -- i.e. every non-milestone document.
fn computed_percent_complete(doc: &DocMeta) -> Option<u8> {
    let as_u64 = |k: &str| match doc.attributes.get(k) {
        Some(AttrValue::Int(n)) if *n >= 0 => Some(*n as u64),
        _ => None,
    };
    let open = as_u64("open_issues")?;
    let closed = as_u64("closed_issues")?;
    percent_complete(open, closed)
}

pub fn doc_to_json(doc: &DocMeta) -> Value {
    let mut value = serde_json::json!({
        "id": doc.id,
        "path": doc.path.to_string_lossy(),
        "title": doc.title,
        "type": format!("{}", doc.doc_type).to_lowercase(),
        "status": format!("{}", doc.status),
        "author": doc.author,
        "date": doc.date.to_string(),
        "tags": doc.tags,
        "assignee": doc.assignee,
        "provenance": doc.provenance,
        "governs": doc.governs,
        "reviewed": doc.reviewed,
        "related": doc.related.iter().map(|r| {
            serde_json::json!({
                "type": format!("{}", r.rel_type),
                "target": r.target,
            })
        }).collect::<Vec<_>>(),
        "validate_ignore": doc.validate_ignore,
        "attributes": doc.attributes,
        // RFC-074 AC2/AC6: always present, empty for a document that is not a
        // bundle, so `show`/`list`/`context`/`create --json` never differ on
        // whether a caller has to branch on their absence.
        "parts": doc.parts.iter().map(|p| {
            serde_json::json!({
                "name": p.name,
                "path": p.path.to_string_lossy(),
            })
        }).collect::<Vec<_>>(),
        "sidecars": doc.sidecars.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>(),
    });
    if let Some(pct) = computed_percent_complete(doc) {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("percent_complete".to_string(), Value::from(pct));
        }
    }
    value
}
