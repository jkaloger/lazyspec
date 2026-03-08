use crate::engine::document::DocMeta;
use crate::engine::store::Store;
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
        "validate_ignore": doc.validate_ignore,
    })
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
