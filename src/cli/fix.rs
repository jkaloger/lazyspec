mod config;
mod conflicts;
mod fields;
mod output;
mod relations;
pub mod renumber;

use std::path::Path;

use serde::Serialize;

use crate::cli::RenumberFormat;
use crate::engine::config::Config;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use config::collect_config_fixes;
use conflicts::collect_conflict_fixes;
use fields::collect_field_fixes;
use output::{format_config_human, format_human};
use relations::collect_relation_fixes;
use renumber::collect_renumber_output;

#[derive(Debug, Serialize)]
struct FixOutput {
    field_fixes: Vec<FieldFixResult>,
    conflict_fixes: Vec<ConflictFixResult>,
    relation_fixes: Vec<RelationFixResult>,
}

#[derive(Debug, Serialize)]
struct RelationFixResult {
    path: String,
    replacements: Vec<(String, String)>,
    written: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct RenumberFixResult {
    pub old_path: String,
    pub new_path: String,
    pub old_id: String,
    pub new_id: String,
    pub references_updated: Vec<ReferenceUpdate>,
    pub written: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ExternalReference {
    pub file: String,
    pub old_name: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
struct RenumberOutput {
    format: String,
    doc_type: Option<String>,
    dry_run: bool,
    changes: Vec<RenumberFixResult>,
    external_references: Vec<ExternalReference>,
}

#[derive(Debug, Serialize)]
struct FieldFixResult {
    path: String,
    fields_added: Vec<String>,
    written: bool,
}

/// Outcome of `fix --config`: which standard relationships/rules were missing
/// (and thus added) and whether the file was written.
#[derive(Debug, Serialize)]
pub struct ConfigFixResult {
    relationships_added: Vec<String>,
    rules_added: Vec<String>,
    written: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ReferenceUpdate {
    pub file: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Serialize)]
struct ConflictFixResult {
    old_path: String,
    new_path: String,
    old_id: String,
    new_id: String,
    references_updated: Vec<ReferenceUpdate>,
    written: bool,
}

pub fn run(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    json: bool,
    fs: &dyn FileSystem,
) -> i32 {
    let output = plan_field_and_conflict_fixes(root, store, config, paths, dry_run, fs);
    let has_fixes = !output.field_fixes.iter().all(|r| r.fields_added.is_empty())
        || !output.conflict_fixes.is_empty()
        || !output.relation_fixes.is_empty();

    if json {
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        println!("{}", json_str);
    } else {
        let human = format_human(&output, dry_run);
        if !human.is_empty() {
            print!("{}", human);
        }
    }

    if has_fixes {
        0
    } else {
        1
    }
}

/// Entry point for `fix --config`: inject the missing standard relationships
/// and rules into `.lazyspec.toml`. Config-only scope — no documents are
/// touched. Returns the process exit code.
pub fn run_config(root: &Path, dry_run: bool, json: bool, fs: &dyn FileSystem) -> i32 {
    let result = match collect_config_fixes(root, dry_run, fs) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return 1;
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        let human = format_config_human(&result, dry_run);
        if !human.is_empty() {
            print!("{}", human);
        }
    }

    0
}

pub fn run_config_json(root: &Path, dry_run: bool, fs: &dyn FileSystem) -> String {
    let result = collect_config_fixes(root, dry_run, fs).unwrap();
    serde_json::to_string_pretty(&result).unwrap()
}

pub fn run_config_human(root: &Path, dry_run: bool, fs: &dyn FileSystem) -> String {
    let result = collect_config_fixes(root, dry_run, fs).unwrap();
    format_config_human(&result, dry_run)
}

#[allow(clippy::too_many_arguments)]
pub fn run_renumber(
    root: &Path,
    store: &Store,
    config: &Config,
    format: &RenumberFormat,
    doc_type: Option<&str>,
    dry_run: bool,
    json: bool,
    fs: &dyn FileSystem,
) -> i32 {
    let output = collect_renumber_output(root, store, config, format, doc_type, dry_run, fs);

    if json {
        let wrapper = serde_json::json!({ "renumber": output });
        println!("{}", serde_json::to_string_pretty(&wrapper).unwrap());
    } else {
        for c in &output.changes {
            if dry_run {
                println!("Would rename {} -> {}", c.old_path, c.new_path);
            } else {
                println!("Renamed {} -> {}", c.old_path, c.new_path);
            }
            for r in &c.references_updated {
                if dry_run {
                    println!(
                        "  Would update ref in {}: {} -> {}",
                        r.file, r.old_value, r.new_value
                    );
                } else {
                    println!(
                        "  Updated ref in {}: {} -> {}",
                        r.file, r.old_value, r.new_value
                    );
                }
            }
        }
        if output.changes.is_empty() {
            let type_filter = doc_type
                .map(|t| format!(" (type: {})", t))
                .unwrap_or_default();
            println!("No documents to renumber{}", type_filter);
        }
        if !output.external_references.is_empty() {
            println!(
                "Warning: {} external references found that could not be auto-updated",
                output.external_references.len()
            );
            for ext in &output.external_references {
                println!("  {}:{} references {}", ext.file, ext.line, ext.old_name);
            }
        }
    }

    0
}

pub fn run_json(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    fs: &dyn FileSystem,
) -> String {
    let output = plan_field_and_conflict_fixes(root, store, config, paths, dry_run, fs);
    serde_json::to_string_pretty(&output).unwrap()
}

pub fn run_renumber_json(
    root: &Path,
    store: &Store,
    config: &Config,
    format: &RenumberFormat,
    doc_type: Option<&str>,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> String {
    let output = collect_renumber_output(root, store, config, format, doc_type, dry_run, fs);
    let wrapper = serde_json::json!({ "renumber": output });
    serde_json::to_string_pretty(&wrapper).unwrap()
}

pub fn run_human(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    fs: &dyn FileSystem,
) -> String {
    let output = plan_field_and_conflict_fixes(root, store, config, paths, dry_run, fs);
    format_human(&output, dry_run)
}

fn plan_field_and_conflict_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    fs: &dyn FileSystem,
) -> FixOutput {
    let field_fixes = collect_field_fixes(root, store, config, paths, dry_run, fs);
    let conflict_fixes = collect_conflict_fixes(root, store, config, dry_run, fs);
    let relation_fixes = collect_relation_fixes(root, store, dry_run, fs);
    FixOutput {
        field_fixes,
        conflict_fixes,
        relation_fixes,
    }
}
