use crate::cli::json::doc_to_json_with_family;
use crate::cli::resolve::resolve_shorthand_or_path;
use crate::cli::style::{bold, dim, separator, styled_status};
use crate::engine::fs::FileSystem;
use crate::engine::store::{ResolveError, Store};
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

pub fn run(store: &Store, id: &str, expand: bool, max_ref_lines: usize, fs: &dyn FileSystem) -> Result<()> {
    let doc = match resolve_shorthand_or_path(store, id) {
        Ok(doc) => doc,
        Err(ResolveError::Ambiguous { id, matches }) => {
            eprintln!("Ambiguous ID '{}' matches multiple documents:", id);
            for m in &matches {
                eprintln!("  {}", m.display());
            }
            eprintln!("Specify the full path to show a specific document.");
            return Ok(());
        }
        Err(ResolveError::NotFound(id)) => {
            return Err(anyhow::anyhow!("document not found: {}", id));
        }
    };

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

    let body = if expand {
        store.get_body_expanded(&doc.path, max_ref_lines, fs)?
    } else {
        store.get_body_raw(&doc.path, fs)?
    };
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

pub fn run_json(store: &Store, id: &str, expand: bool, max_ref_lines: usize, fs: &dyn FileSystem) -> Result<String> {
    let doc = match resolve_shorthand_or_path(store, id) {
        Ok(doc) => doc,
        Err(ResolveError::Ambiguous { id, matches }) => {
            let paths: Vec<String> = matches.iter().map(|m| m.to_string_lossy().to_string()).collect();
            let error = serde_json::json!({
                "error": "ambiguous_id",
                "id": id,
                "ambiguous_matches": paths,
            });
            return Ok(serde_json::to_string_pretty(&error)?);
        }
        Err(ResolveError::NotFound(id)) => {
            return Err(anyhow::anyhow!("document not found: {}", id));
        }
    };

    let mut json = doc_to_json_with_family(doc, store);
    let body = if expand {
        store.get_body_expanded(&doc.path, max_ref_lines, fs)?
    } else {
        store.get_body_raw(&doc.path, fs)?
    };
    json["body"] = serde_json::Value::String(body);

    Ok(serde_json::to_string_pretty(&json)?)
}
