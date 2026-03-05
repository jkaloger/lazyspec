use crate::cli::json::doc_to_json;
use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::Store;
use anyhow::Result;
use std::path::PathBuf;

pub fn resolve_chain<'a>(store: &'a Store, id: &str) -> Result<Vec<&'a DocMeta>> {
    let doc = store
        .resolve_shorthand(id)
        .ok_or_else(|| anyhow::anyhow!("document not found: {}", id))?;

    let mut chain = vec![doc];

    loop {
        let current = chain[0];
        let parent = current.related.iter().find_map(|rel| {
            if rel.rel_type == RelationType::Implements {
                store.get(&PathBuf::from(&rel.target))
            } else {
                None
            }
        });

        match parent {
            Some(p) => chain.insert(0, p),
            None => break,
        }
    }

    Ok(chain)
}

pub fn run_json(store: &Store, id: &str) -> Result<String> {
    let chain = resolve_chain(store, id)?;
    let items: Vec<_> = chain.iter().map(|d| doc_to_json(d)).collect();
    let output = serde_json::json!({ "chain": items });
    Ok(serde_json::to_string_pretty(&output)?)
}

pub fn run_human(store: &Store, id: &str) -> Result<String> {
    let chain = resolve_chain(store, id)?;
    let mut output = String::new();

    for (i, doc) in chain.iter().enumerate() {
        if i > 0 {
            output.push_str("  ↓\n");
        }
        output.push_str(&format!(
            "{} ({}) [{}] {}\n",
            doc.title,
            format!("{}", doc.doc_type).to_lowercase(),
            doc.status,
            doc.path.display()
        ));
    }

    Ok(output)
}
