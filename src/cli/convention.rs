use crate::cli::json::doc_to_json;
use crate::engine::config::Config;
use crate::engine::document::DocType;
use crate::engine::fs::FileSystem;
use crate::engine::store::{Filter, Store};
use anyhow::Result;

fn find_convention_type(config: &Config) -> Option<&str> {
    config
        .documents
        .types
        .iter()
        .find(|t| t.singleton)
        .map(|t| t.name.as_str())
}

fn find_dictum_type_names(config: &Config, convention_type: &str) -> Vec<String> {
    config
        .documents
        .types
        .iter()
        .filter(|t| t.parent_type.as_deref() == Some(convention_type))
        .map(|t| t.name.clone())
        .collect()
}

pub fn run_human(
    store: &Store,
    config: &Config,
    preamble: bool,
    tags: Option<&str>,
    fs: &dyn FileSystem,
) -> Result<String> {
    let convention_type_name = match find_convention_type(config) {
        Some(name) => name,
        None => return Ok(String::new()),
    };

    let conventions = store.list(&Filter {
        doc_type: Some(DocType::new(convention_type_name)),
        ..Filter::default()
    });
    let convention = match conventions.first() {
        Some(c) => c,
        None => return Ok(String::new()),
    };

    let mut output = store.get_body_raw(&convention.path, fs)?;

    if preamble {
        return Ok(output);
    }

    let dictum_type_names = find_dictum_type_names(config, convention_type_name);
    let requested_tags: Vec<&str> = tags
        .map(|t| t.split(',').map(|s| s.trim()).collect())
        .unwrap_or_default();

    let mut dicta = Vec::new();
    for type_name in &dictum_type_names {
        let docs = store.list(&Filter {
            doc_type: Some(DocType::new(type_name)),
            ..Filter::default()
        });
        dicta.extend(docs);
    }

    if !requested_tags.is_empty() {
        dicta.retain(|doc| {
            doc.tags
                .iter()
                .any(|t| requested_tags.contains(&t.as_str()))
        });
    }

    dicta.sort_by(|a, b| a.path.cmp(&b.path));

    for doc in &dicta {
        output.push_str(&format!("\n## {}\n\n", doc.title));
        let body = store.get_body_raw(&doc.path, fs)?;
        output.push_str(&body);
    }

    Ok(output)
}

pub fn run_json(
    store: &Store,
    config: &Config,
    preamble: bool,
    tags: Option<&str>,
    fs: &dyn FileSystem,
) -> Result<String> {
    let convention_type_name = match find_convention_type(config) {
        Some(name) => name,
        None => {
            let output = serde_json::json!({"convention": null, "dicta": []});
            return Ok(serde_json::to_string_pretty(&output)?);
        }
    };

    let conventions = store.list(&Filter {
        doc_type: Some(DocType::new(convention_type_name)),
        ..Filter::default()
    });
    let convention = match conventions.first() {
        Some(c) => c,
        None => {
            let output = serde_json::json!({"convention": null, "dicta": []});
            return Ok(serde_json::to_string_pretty(&output)?);
        }
    };

    let mut conv_json = doc_to_json(convention);
    let conv_body = store.get_body_raw(&convention.path, fs)?;
    conv_json["body"] = serde_json::Value::String(conv_body);

    let dicta_json = if preamble {
        vec![]
    } else {
        let dictum_type_names = find_dictum_type_names(config, convention_type_name);
        let requested_tags: Vec<&str> = tags
            .map(|t| t.split(',').map(|s| s.trim()).collect())
            .unwrap_or_default();

        let mut dicta = Vec::new();
        for type_name in &dictum_type_names {
            let docs = store.list(&Filter {
                doc_type: Some(DocType::new(type_name)),
                ..Filter::default()
            });
            dicta.extend(docs);
        }

        if !requested_tags.is_empty() {
            dicta.retain(|doc| {
                doc.tags
                    .iter()
                    .any(|t| requested_tags.contains(&t.as_str()))
            });
        }

        dicta.sort_by(|a, b| a.path.cmp(&b.path));

        let mut result = Vec::new();
        for doc in &dicta {
            let mut j = doc_to_json(doc);
            let body = store.get_body_raw(&doc.path, fs)?;
            j["body"] = serde_json::Value::String(body);
            result.push(j);
        }
        result
    };

    let output = serde_json::json!({
        "convention": conv_json,
        "dicta": dicta_json,
    });

    Ok(serde_json::to_string_pretty(&output)?)
}
