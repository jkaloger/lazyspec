mod cascade;
mod config;
mod conflicts;
mod fields;
mod relations;

pub use cascade::cascade_references;
pub use config::collect_config_fixes;

use std::path::Path;

use serde::Serialize;

use crate::engine::config::Config;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use conflicts::collect_conflict_fixes;
use fields::collect_field_fixes;
use relations::collect_relation_fixes;

#[derive(Debug, Serialize)]
pub struct FixOutput {
    pub field_fixes: Vec<FieldFixResult>,
    pub conflict_fixes: Vec<ConflictFixResult>,
    pub relation_fixes: Vec<RelationFixResult>,
}

#[derive(Debug, Serialize)]
pub struct RelationFixResult {
    pub path: String,
    pub replacements: Vec<(String, String)>,
    /// Duplicate `(rel_type, target)` pairs dropped from the doc's `related`
    /// sequence (defect-C cleanup). Kept distinct from `replacements` so the
    /// `--json` output surfaces path->id migrations and dedup separately.
    pub deduped: Vec<(String, String)>,
    pub written: bool,
}

#[derive(Debug, Serialize)]
pub struct FieldFixResult {
    pub path: String,
    pub fields_added: Vec<String>,
    pub written: bool,
}

/// Outcome of `fix --config`: which standard relationships/rules were missing
/// (and thus added) and whether the file was written.
#[derive(Debug, Serialize)]
pub struct ConfigFixResult {
    pub relationships_added: Vec<String>,
    pub rules_added: Vec<String>,
    pub lifecycles_added: Vec<String>,
    pub written: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ReferenceUpdate {
    pub file: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Serialize)]
pub struct ConflictFixResult {
    pub old_path: String,
    pub new_path: String,
    pub old_id: String,
    pub new_id: String,
    pub references_updated: Vec<ReferenceUpdate>,
    pub written: bool,
}

pub fn plan_field_and_conflict_fixes(
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
