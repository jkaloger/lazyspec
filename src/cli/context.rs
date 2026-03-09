use crate::cli::json::doc_to_json_with_family;
use crate::cli::style::{bold, dim, styled_status};
use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::Store;
use anyhow::Result;
use console::colors_enabled;
use std::collections::HashSet;
use std::path::PathBuf;

pub struct ResolvedContext<'a> {
    pub chain: Vec<&'a DocMeta>,
    pub target_index: usize,
    pub forward: Vec<&'a DocMeta>,
    pub related: Vec<&'a DocMeta>,
}

pub fn resolve_chain<'a>(store: &'a Store, id: &str) -> Result<ResolvedContext<'a>> {
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

    let target_index = chain.iter().position(|d| d.path == doc.path).unwrap_or(0);

    // Forward context: find docs whose `implements` points at the target
    let target_path = &doc.path;
    let forward: Vec<&DocMeta> = store
        .reverse_links
        .get(target_path)
        .map(|links| {
            links
                .iter()
                .filter(|(rel_type, _)| *rel_type == RelationType::Implements)
                .filter_map(|(_, source_path)| store.get(source_path))
                .collect()
        })
        .unwrap_or_default();

    // Related: collect RelatedTo links from all chain documents, deduplicated
    let chain_paths: HashSet<&PathBuf> = chain.iter().map(|d| &d.path).collect();
    let mut seen = HashSet::new();
    let mut related = Vec::new();

    for chain_doc in &chain {
        // Forward RelatedTo links from this doc
        if let Some(fwd) = store.forward_links.get(&chain_doc.path) {
            for (rel_type, target) in fwd {
                if *rel_type == RelationType::RelatedTo
                    && !chain_paths.contains(target)
                    && seen.insert(target.clone())
                {
                    if let Some(resolved) = store.get(target) {
                        related.push(resolved);
                    }
                }
            }
        }
        // Reverse RelatedTo links pointing at this doc
        if let Some(rev) = store.reverse_links.get(&chain_doc.path) {
            for (rel_type, source) in rev {
                if *rel_type == RelationType::RelatedTo
                    && !chain_paths.contains(source)
                    && seen.insert(source.clone())
                {
                    if let Some(resolved) = store.get(source) {
                        related.push(resolved);
                    }
                }
            }
        }
    }

    Ok(ResolvedContext {
        chain,
        target_index,
        forward,
        related,
    })
}

pub fn run_json(store: &Store, id: &str) -> Result<String> {
    let resolved = resolve_chain(store, id)?;
    let chain: Vec<_> = resolved.chain.iter().map(|d| doc_to_json_with_family(d, store)).collect();
    let related: Vec<_> = resolved.related.iter().map(|d| doc_to_json_with_family(d, store)).collect();
    let output = serde_json::json!({ "chain": chain, "related": related });
    Ok(serde_json::to_string_pretty(&output)?)
}

fn mini_card(doc: &DocMeta, marker: bool) -> String {
    let title = &doc.title;
    let doc_type = format!("{}", doc.doc_type).to_lowercase();
    let shorthand = doc.id.to_uppercase();
    let status = &doc.status;
    let status_str = format!("{}", status);
    let line2_plain = format!("{} {} [{}]", shorthand, doc_type, status_str);
    let content_width = title.len().max(line2_plain.len()) + 2;
    let marker_suffix = if marker { "  \u{2190} you are here" } else { "" };

    if !colors_enabled() {
        let border = "-".repeat(content_width);
        return format!(
            "+{}+\n| {:<width$}|{}\n| {:<width$}|\n+{}+",
            border,
            format!("{} ", title),
            marker_suffix,
            format!("{} ", line2_plain),
            border,
            width = content_width - 1,
        );
    }

    let styled_marker = if marker {
        format!("  {}", dim("\u{2190} you are here"))
    } else {
        String::new()
    };
    let top = format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(content_width));
    let pad1 = " ".repeat(content_width - 1 - title.len());
    let mid1 = format!("\u{2502} {}{}\u{2502}{}", bold(title), pad1, styled_marker);
    let pad2 = " ".repeat(content_width - 1 - line2_plain.len());
    let line2_styled = format!("{} {} [{}]", shorthand, doc_type, styled_status(status));
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
    let resolved = resolve_chain(store, id)?;
    let mut output = String::new();

    for (i, doc) in resolved.chain.iter().enumerate() {
        if i > 0 {
            output.push_str(&chain_connector());
            output.push('\n');
        }
        output.push_str(&mini_card(doc, i == resolved.target_index));
        output.push('\n');

        let child_paths = store.children_of(&doc.path);
        if !child_paths.is_empty() {
            let children: Vec<_> = child_paths
                .iter()
                .filter_map(|cp| store.get(cp))
                .collect();
            for (j, child) in children.iter().enumerate() {
                let connector = if j == children.len() - 1 { "\u{2514}\u{2500}" } else { "\u{251c}\u{2500}" };
                let shorthand = child.id.to_uppercase();
                let title = &child.title;
                let status_display = if colors_enabled() {
                    styled_status(&child.status)
                } else {
                    format!("{}", child.status)
                };
                output.push_str(&format!("  {} {} {} [{}]\n", connector, shorthand, title, status_display));
            }
        }
    }

    if !resolved.forward.is_empty() {
        output.push_str(&chain_connector());
        output.push('\n');
        for (j, child) in resolved.forward.iter().enumerate() {
            let connector = if j == resolved.forward.len() - 1 {
                "\u{2514}\u{2500}"
            } else {
                "\u{251c}\u{2500}"
            };
            let shorthand = child.id.to_uppercase();
            let title = &child.title;
            let status_display = if colors_enabled() {
                styled_status(&child.status)
            } else {
                format!("{}", child.status)
            };
            output.push_str(&format!("  {} {} {} [{}]\n", connector, shorthand, title, status_display));
        }
    }

    if !resolved.related.is_empty() {
        output.push('\n');
        if colors_enabled() {
            output.push_str(&format!("{}\n", dim("\u{2500}\u{2500}\u{2500} related \u{2500}\u{2500}\u{2500}")));
        } else {
            output.push_str("--- related ---\n");
        }
        for rel_doc in &resolved.related {
            let shorthand = rel_doc.id.to_uppercase();
            let status_display = if colors_enabled() {
                styled_status(&rel_doc.status)
            } else {
                format!("{}", rel_doc.status)
            };
            output.push_str(&format!(
                "  {}  {} [{}]\n",
                shorthand, rel_doc.title, status_display
            ));
        }
    }

    Ok(output)
}
