mod cascade;
mod config;
mod conflicts;
mod fields;
mod relations;
mod status;

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
use status::collect_status_fixes;

#[derive(Debug, Serialize)]
pub struct FixOutput {
    pub field_fixes: Vec<FieldFixResult>,
    pub conflict_fixes: Vec<ConflictFixResult>,
    pub relation_fixes: Vec<RelationFixResult>,
    pub status_fixes: Vec<StatusFixResult>,
}

/// Repair of a document whose frontmatter `status` is not one of its type's
/// lifecycle states: rewritten to `lifecycle.states[0]` so it re-enters its
/// lifecycle and gains legal transitions again.
#[derive(Debug, Serialize)]
pub struct StatusFixResult {
    pub path: String,
    pub old_status: String,
    pub new_status: String,
    pub written: bool,
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

/// Outcome of `fix --config`: what the run adds, what the RFC-067 edge
/// migration takes away, and whether the file was written.
///
/// The two halves never name the same thing. `*_added` is what the file was
/// missing; `*_removed` is what the translating rewrite deletes from it, read
/// off the source alone (ADR-032). `edges_written` names the `[[edges]]` rows
/// the translation produces, which include the standard constraints seeded
/// through it — those appear in `rules_added` too, because the constraint is
/// what was missing and the row is how it is now spelled.
#[derive(Debug, Serialize)]
pub struct ConfigFixResult {
    pub relationships_added: Vec<String>,
    pub rules_added: Vec<String>,
    pub lifecycles_added: Vec<String>,
    pub edges_written: Vec<String>,
    pub rules_removed: Vec<String>,
    pub traversal_removed: Vec<String>,
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
    let status_fixes = collect_status_fixes(root, store, config, paths, dry_run, fs);
    FixOutput {
        field_fixes,
        conflict_fixes,
        relation_fixes,
        status_fixes,
    }
}
