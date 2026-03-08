use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{bold, dim, separator, styled_status};
use crate::engine::store::Store;
use anyhow::Result;
use console::colors_enabled;

fn title_box(title: &str) -> String {
    if !colors_enabled() {
        return format!("# {}", title);
    }

    let padded = format!(" {} ", title);
    let width = padded.len();
    let top = format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(width));
    let mid = format!("\u{2502}{}\u{2502}", bold(&padded));
    let bot = format!("\u{2570}{}\u{256f}", "\u{2500}".repeat(width));
    format!("{}\n{}\n{}", top, mid, bot)
}

pub fn run(store: &Store, id: &str) -> Result<()> {
    let doc = store
        .resolve_shorthand(id)
        .ok_or_else(|| anyhow::anyhow!("document not found: {}", id))?;

    println!("{}", title_box(&doc.title));
    println!(
        "{} {}  {} {}  {} {}",
        dim("Type:"),
        bold(&doc.doc_type.to_string()),
        dim("Status:"),
        styled_status(&doc.status),
        dim("Author:"),
        bold(&doc.author),
    );
    if !doc.tags.is_empty() {
        println!("{} {}", dim("Tags:"), doc.tags.join(", "));
    }
    if let Some(parent_path) = store.parent_of(&doc.path) {
        if let Some(parent) = store.get(parent_path) {
            println!(
                "{} {} {}",
                dim("Parent:"),
                bold(&parent.title),
                dim(&parent.path.to_string_lossy()),
            );
        }
    }
    println!("{}", separator());

    let body = store.get_body(&doc.path)?;
    println!("{}", body);

    let child_paths = store.children_of(&doc.path);
    if !child_paths.is_empty() {
        println!();
        println!("{}", dim("Children:"));
        for cp in child_paths {
            if let Some(child) = store.get(cp) {
                let parent_dir = cp.parent().and_then(|p| p.file_name()).unwrap_or_default();
                let file_stem = cp.file_stem().unwrap_or_default();
                let qualified_shorthand =
                    format!("{}/{}", parent_dir.to_string_lossy(), file_stem.to_string_lossy());
                println!("  - {}  ({})", child.title, qualified_shorthand);
            }
        }
    }

    Ok(())
}

pub fn run_json(store: &Store, id: &str) -> Result<String> {
    let doc = store
        .resolve_shorthand(id)
        .ok_or_else(|| anyhow::anyhow!("document not found: {}", id))?;

    let mut json = doc_to_json_with_family(doc, store);
    let body = store.get_body(&doc.path)?;
    json["body"] = serde_json::Value::String(body);

    Ok(serde_json::to_string_pretty(&json)?)
}
