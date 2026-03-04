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
