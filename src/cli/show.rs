use crate::cli::json::doc_to_json;
use crate::engine::store::Store;
use anyhow::Result;

pub fn run(store: &Store, id: &str) -> Result<()> {
    let doc = store
        .resolve_shorthand(id)
        .ok_or_else(|| anyhow::anyhow!("document not found: {}", id))?;

    println!("# {}", doc.title);
    println!(
        "Type: {} | Status: {} | Author: {}",
        doc.doc_type, doc.status, doc.author
    );
    println!("Date: {} | Tags: {}", doc.date, doc.tags.join(", "));
    println!();

    let body = store.get_body(&doc.path)?;
    println!("{}", body);

    Ok(())
}

pub fn run_json(store: &Store, id: &str) -> Result<String> {
    let doc = store
        .resolve_shorthand(id)
        .ok_or_else(|| anyhow::anyhow!("document not found: {}", id))?;

    let mut json = doc_to_json(doc);
    let body = store.get_body(&doc.path)?;
    json["body"] = serde_json::Value::String(body);

    Ok(serde_json::to_string_pretty(&json)?)
}
