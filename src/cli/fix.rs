mod output;
pub mod renumber;

use std::path::Path;

use serde::Serialize;

use crate::cli::RenumberFormat;
use crate::engine::config::Config;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use crate::engine::git_ref::GitRefOps;
use crate::engine::git_store::commit_if_git_backed;
use crate::engine::ops::fix::{
    collect_config_fixes, collect_governs_fixes, plan_field_and_conflict_fixes,
};
pub use crate::engine::ops::fix::{ConfigFixResult, GovernsFixResult, ReferenceUpdate};

use output::{format_config_human, format_governs_human, format_human};
use renumber::collect_renumber_output;

#[derive(Debug, Serialize, Clone)]
pub struct RenumberFixResult {
    pub old_path: String,
    pub new_path: String,
    pub old_id: String,
    pub new_id: String,
    pub references_updated: Vec<ReferenceUpdate>,
    pub written: bool,
    /// Why the rename did not reach the document, when it failed. `None` for a
    /// dry run and for a rename that landed.
    pub error: Option<String>,
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

/// Commit every document a fix reached when it sits in a `git` type's clone
/// (STORY-282 AC2). An `Err` is a rejected push with the clone rolled back, so
/// nothing the plan reports as `written` still is; the caller prints the error
/// in place of the plan (DICTUM-006).
fn commit_written<'a>(
    root: &Path,
    config: &Config,
    git: &dyn GitRefOps,
    paths: impl IntoIterator<Item = &'a str>,
    message: &str,
) -> anyhow::Result<()> {
    for path in paths {
        commit_if_git_backed(root, config, Path::new(path), git, message)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    json: bool,
    git: &dyn GitRefOps,
    fs: &dyn FileSystem,
) -> i32 {
    let output = plan_field_and_conflict_fixes(root, store, config, paths, dry_run, fs);
    if let Err(e) = commit_written(root, config, git, output.written_paths(), "fix") {
        eprintln!("error: {e:#}");
        return 1;
    }
    let has_fixes = !output.field_fixes.iter().all(|r| r.fields_added.is_empty())
        || !output.conflict_fixes.is_empty()
        || !output.relation_fixes.is_empty()
        || !output.status_fixes.is_empty();

    if json {
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        println!("{}", json_str);
    } else {
        let human = format_human(&output, dry_run);
        if !human.is_empty() {
            print!("{}", human);
        }
    }

    // A rewrite that could not reach its document is not a successful run, even
    // though the plan named fixes to make. Every collector, not just relations:
    // a run that failed to write is a failure whichever repair it was making.
    let failed = output.field_fixes.iter().any(|r| r.error.is_some())
        || output.conflict_fixes.iter().any(|r| r.error.is_some())
        || output.relation_fixes.iter().any(|r| r.error.is_some())
        || output.status_fixes.iter().any(|r| r.error.is_some());

    if has_fixes && !failed {
        0
    } else {
        1
    }
}

/// Entry point for `fix --config`: inject the missing standard relationships
/// and edges into `.lazyspec.toml`, and translate a retired `[[rules]]` table
/// into `[[edges]]`. Config-only scope — no documents are touched. Returns the
/// process exit code.
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

/// Entry point for `fix --governs`: rewrite every rotted `governs` glob to the
/// `suggested_glob` its `governs-no-match` finding carried. Returns the process
/// exit code.
///
/// 0 when nothing needed repairing -- a repo with no rotted pin is the healthy
/// case, not a failure. Bare `fix` reports "nothing to fix" as 1; this sub-mode
/// is what a repair script runs unconditionally, so it does not. 1 when a
/// rewrite could not reach its document, matching `run_config`: the caller asked
/// for a repair and did not get one.
pub fn run_governs(
    root: &Path,
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    dry_run: bool,
    json: bool,
    fs: &dyn FileSystem,
) -> i32 {
    let rewrites = collect_governs_fixes(root, store, git, dry_run, fs);
    let written = rewrites
        .iter()
        .filter(|r| r.written)
        .map(|r| r.path.as_str());
    if let Err(e) = commit_written(root, config, git, written, "fix governs") {
        eprintln!("error: {e:#}");
        return 1;
    }

    if json {
        println!("{}", governs_json(&rewrites));
    } else {
        let human = format_governs_human(&rewrites, dry_run);
        if !human.is_empty() {
            print!("{}", human);
        }
    }

    if rewrites.iter().any(|r| r.error.is_some()) {
        1
    } else {
        0
    }
}

fn governs_json(rewrites: &[GovernsFixResult]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ "governs": rewrites })).unwrap()
}

pub fn run_governs_json(
    root: &Path,
    store: &Store,
    git: &dyn GitRefOps,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> String {
    governs_json(&collect_governs_fixes(root, store, git, dry_run, fs))
}

pub fn run_governs_human(
    root: &Path,
    store: &Store,
    git: &dyn GitRefOps,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> String {
    let rewrites = collect_governs_fixes(root, store, git, dry_run, fs);
    format_governs_human(&rewrites, dry_run)
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
    git: &dyn GitRefOps,
    fs: &dyn FileSystem,
) -> i32 {
    let output = collect_renumber_output(root, store, config, format, doc_type, dry_run, fs);
    let written = output.changes.iter().filter(|c| c.written).flat_map(|c| {
        std::iter::once(c.new_path.as_str())
            .chain(c.references_updated.iter().map(|u| u.file.as_str()))
    });
    if let Err(e) = commit_written(root, config, git, written, "fix renumber") {
        eprintln!("error: {e:#}");
        return 1;
    }

    if json {
        let wrapper = serde_json::json!({ "renumber": output });
        println!("{}", serde_json::to_string_pretty(&wrapper).unwrap());
    } else {
        for c in &output.changes {
            if let Some(e) = &c.error {
                println!(
                    "error: could not rename {} -> {}: {}",
                    c.old_path, c.new_path, e
                );
                continue;
            }
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

    // A rename that could not reach its document is a failed run, like every
    // other `fix` sub-mode.
    if output.changes.iter().any(|c| c.error.is_some()) {
        1
    } else {
        0
    }
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
