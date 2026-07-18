use crate::engine::document::{AttrValue, DocMeta};
use crate::engine::store::Store;
use crate::engine::store_dispatch::{percent_complete, PushOutcome};
use serde_json::Value;

/// Fold a backend [`PushOutcome`] into a mutation's JSON object: always record
/// `synced`, and on a local-only push add a `warnings` array carrying the push
/// failure text (omitted when synced). Fixes BUG-006, where a git-ref push that
/// never reached the remote still printed a clean success to `--json` stdout.
pub fn merge_push_outcome(value: &mut Value, outcome: &PushOutcome) {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("synced".to_string(), Value::Bool(outcome.is_synced()));
        if let Some(warning) = outcome.warning() {
            obj.insert(
                "warnings".to_string(),
                Value::Array(vec![Value::String(warning.to_string())]),
            );
        }
    }
}

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
        "related": doc.related.iter().map(|r| {
            serde_json::json!({
                "type": format!("{}", r.rel_type),
                "target": r.target,
            })
        }).collect::<Vec<_>>(),
        "validate_ignore": doc.validate_ignore,
        "attributes": doc.attributes,
    });
    if let Some(pct) = computed_percent_complete(doc) {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("percent_complete".to_string(), Value::from(pct));
        }
    }
    value
}

pub fn doc_to_json_with_family(doc: &DocMeta, store: &Store) -> Value {
    let mut json = doc_to_json(doc);
    let obj = json.as_object_mut().unwrap();

    let child_paths = store.children_of(&doc.path);
    if !child_paths.is_empty() {
        let children: Vec<Value> = child_paths
            .iter()
            .filter_map(|cp| {
                store.get(cp).map(|child| {
                    serde_json::json!({
                        "path": child.path.to_string_lossy(),
                        "title": child.title,
                    })
                })
            })
            .collect();
        obj.insert("children".to_string(), Value::Array(children));
    }

    if let Some(parent_path) = store.parent_of(&doc.path) {
        if let Some(parent) = store.get(parent_path) {
            obj.insert(
                "parent".to_string(),
                serde_json::json!({
                    "path": parent.path.to_string_lossy(),
                    "title": parent.title,
                }),
            );
        }
    }

    if doc.virtual_doc {
        obj.insert("virtual_doc".to_string(), Value::Bool(true));
    }

    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::{DocType, Status};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn meta_with_counts(open: i64, closed: i64) -> DocMeta {
        let mut attributes: BTreeMap<String, AttrValue> = BTreeMap::new();
        attributes.insert("open_issues".to_string(), AttrValue::Int(open));
        attributes.insert("closed_issues".to_string(), AttrValue::Int(closed));
        DocMeta {
            path: PathBuf::from(".lazyspec/cache/milestone/MILESTONE-1.md"),
            title: "v1.0".to_string(),
            doc_type: DocType::new("milestone"),
            status: Status::new("in-progress"),
            author: "github".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes,
            id: "MILESTONE-1".to_string(),
        }
    }

    // AC5: a milestone doc carrying issue counts surfaces a computed
    // percent_complete in its JSON.
    #[test]
    fn milestone_json_includes_computed_percent_complete() {
        let json = doc_to_json(&meta_with_counts(7, 3));
        assert_eq!(json["percent_complete"], serde_json::json!(30));
    }

    // AC4: doc_to_json always carries the doc id, never null.
    #[test]
    fn doc_to_json_carries_id() {
        let mut meta = meta_with_counts(0, 0);
        meta.attributes.clear();
        meta.id = "ISSUE-42".to_string();
        let json = doc_to_json(&meta);
        assert_eq!(json["id"], serde_json::json!("ISSUE-42"));
        assert!(!json["id"].is_null());
    }

    // A non-milestone document (no counts) has no percent_complete key.
    #[test]
    fn ordinary_doc_has_no_percent_complete() {
        let mut meta = meta_with_counts(0, 0);
        meta.attributes.clear();
        let json = doc_to_json(&meta);
        assert!(json.get("percent_complete").is_none());
    }

    // BUG-006: a local-only push (unreachable remote) surfaces `synced: false`
    // and a `warnings` array carrying the push-failure text.
    #[test]
    fn merge_push_outcome_local_only_sets_synced_false_and_warnings() {
        let mut value = serde_json::json!({ "id": "ITERATION-1" });
        let outcome = PushOutcome::LocalOnly {
            warning: "could not push ITERATION-1 to origin".to_string(),
        };
        merge_push_outcome(&mut value, &outcome);

        assert_eq!(value["synced"], serde_json::json!(false));
        assert_eq!(
            value["warnings"],
            serde_json::json!(["could not push ITERATION-1 to origin"])
        );
    }

    // A synced push reports `synced: true` and omits the `warnings` key entirely.
    #[test]
    fn merge_push_outcome_synced_sets_synced_true_and_omits_warnings() {
        let mut value = serde_json::json!({ "id": "ITERATION-1" });
        merge_push_outcome(&mut value, &PushOutcome::Synced);

        assert_eq!(value["synced"], serde_json::json!(true));
        assert!(
            value.get("warnings").is_none(),
            "warnings must be omitted on a synced push, got: {value}"
        );
    }
}
