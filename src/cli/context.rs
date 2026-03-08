use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{bold, dim, styled_status};
use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::Store;
use anyhow::Result;
use console::colors_enabled;
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
    let items: Vec<_> = chain.iter().map(|d| doc_to_json_with_family(d, store)).collect();
    let output = serde_json::json!({ "chain": items });
    Ok(serde_json::to_string_pretty(&output)?)
}

fn mini_card(doc: &DocMeta) -> String {
    let title = &doc.title;
    let doc_type = format!("{}", doc.doc_type).to_lowercase();
    let status = &doc.status;
    let status_str = format!("{}", status);
    let line2_plain = format!("{} [{}]", doc_type, status_str);
    let content_width = title.len().max(line2_plain.len()) + 2;

    if !colors_enabled() {
        let border = "-".repeat(content_width);
        return format!(
            "+{}+\n| {:<width$}|\n| {:<width$}|\n+{}+",
            border,
            format!("{} ", title),
            format!("{} ", line2_plain),
            border,
            width = content_width - 1,
        );
    }

    let top = format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(content_width));
    let pad1 = " ".repeat(content_width - 1 - title.len());
    let mid1 = format!("\u{2502} {}{}\u{2502}", bold(title), pad1);
    let pad2 = " ".repeat(content_width - 1 - line2_plain.len());
    let line2_styled = format!("{} [{}]", doc_type, styled_status(status));
    let mid2 = format!("\u{2502} {}{}\u{2502}", line2_styled, pad2);
    let bot = format!("\u{2570}{}\u{256f}", "\u{2500}".repeat(content_width));
    format!("{}\n{}\n{}\n{}", top, mid1, mid2, bot)
}

fn chain_connector() -> String {
    if colors_enabled() {
        format!("  {}", dim("\u{2502}"))
    } else {
        "  \u{2193}".to_string()
    }
}

pub fn run_human(store: &Store, id: &str) -> Result<String> {
    let chain = resolve_chain(store, id)?;
    let mut output = String::new();

    for (i, doc) in chain.iter().enumerate() {
        if i > 0 {
            output.push_str(&chain_connector());
            output.push('\n');
        }
        output.push_str(&mini_card(doc));
        output.push('\n');

        let child_paths = store.children_of(&doc.path);
        if !child_paths.is_empty() {
            let children: Vec<_> = child_paths
                .iter()
                .filter_map(|cp| store.get(cp))
                .collect();
            for (j, child) in children.iter().enumerate() {
                let connector = if j == children.len() - 1 { "\u{2514}\u{2500}" } else { "\u{251c}\u{2500}" };
                let title = &child.title;
                let path = child.path.to_string_lossy();
                output.push_str(&format!("  {} {}  ({})\n", connector, title, path));
            }
        }
    }

    Ok(output)
}
