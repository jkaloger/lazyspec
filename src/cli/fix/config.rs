use std::path::Path;

use serde::Serialize;

use crate::engine::config::{
    default_rules, starter_relationships, Config, RelationshipDef, ValidationRule,
};
use crate::engine::fs::FileSystem;

use super::ConfigFixResult;

/// Wrapper used solely to serialize the missing `[[relationships]]` blocks as an
/// array-of-tables, so the emitted text matches what the strict load path reads.
#[derive(Serialize)]
struct RelationshipsDoc {
    relationships: Vec<RelationshipDef>,
}

/// Wrapper used solely to serialize the missing `[[rules]]` blocks.
#[derive(Serialize)]
struct RulesDoc {
    rules: Vec<ValidationRule>,
}

fn rule_name(rule: &ValidationRule) -> &str {
    match rule {
        ValidationRule::ParentChild { name, .. } => name,
        ValidationRule::RelationExistence { name, .. } => name,
    }
}

/// Plan (and optionally apply) the config migration: append the standard
/// relationships/rules that the existing `.lazyspec.toml` is missing.
///
/// Append-only by design: the existing file is preserved byte-for-byte and only
/// the missing `[[relationships]]` / `[[rules]]` blocks are appended. This keeps
/// every user section (`[github]`, `[coordination]`, comments, ordering) intact
/// and is idempotent — when nothing is missing the file is left untouched.
pub(super) fn collect_config_fixes(
    root: &Path,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> anyhow::Result<ConfigFixResult> {
    let path = root.join(".lazyspec.toml");
    let existing = fs.read_to_string(&path)?;

    // Lenient read: tolerate a missing [[relationships]] block.
    let config = Config::parse_lenient(&existing)?;

    let existing_rel_names: Vec<&str> = config
        .relationships
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    let missing_relationships: Vec<RelationshipDef> = starter_relationships()
        .into_iter()
        .filter(|r| !existing_rel_names.contains(&r.name.as_str()))
        .collect();

    let existing_rule_names: Vec<&str> = config.rules.iter().map(rule_name).collect();
    let missing_rules: Vec<ValidationRule> = default_rules()
        .into_iter()
        .filter(|r| !existing_rule_names.contains(&rule_name(r)))
        .collect();

    let relationships_added: Vec<String> = missing_relationships
        .iter()
        .map(|r| r.name.clone())
        .collect();
    let rules_added: Vec<String> = missing_rules
        .iter()
        .map(|r| rule_name(r).to_string())
        .collect();

    let nothing_missing = missing_relationships.is_empty() && missing_rules.is_empty();

    let written = if dry_run || nothing_missing {
        false
    } else {
        let appended = append_blocks(&existing, &missing_relationships, &missing_rules)?;
        fs.write(&path, &appended)?;
        true
    };

    Ok(ConfigFixResult {
        relationships_added,
        rules_added,
        written,
    })
}

/// Append the missing blocks to the existing config text, separated by a blank
/// line, leaving the original content untouched.
fn append_blocks(
    existing: &str,
    relationships: &[RelationshipDef],
    rules: &[ValidationRule],
) -> anyhow::Result<String> {
    let mut out = existing.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }

    if !relationships.is_empty() {
        let block = toml::to_string(&RelationshipsDoc {
            relationships: relationships.to_vec(),
        })?;
        out.push('\n');
        out.push_str(&block);
    }

    if !rules.is_empty() {
        let block = toml::to_string(&RulesDoc {
            rules: rules.to_vec(),
        })?;
        out.push('\n');
        out.push_str(&block);
    }

    Ok(out)
}
