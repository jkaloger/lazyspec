use crate::cli::json::{doc_to_json, merge_push_outcome};
use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::git_ref::GitRefOps;
use crate::engine::reservation;
use crate::engine::store::Store;
use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::engine::ops::create::run_with_body_full;
pub use crate::engine::ops::create::{run, run_with_body};

#[allow(clippy::too_many_arguments)]
pub fn run_json(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    git: &dyn GitRefOps,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<String> {
    run_json_with_body(
        root,
        config,
        store,
        doc_type,
        title,
        author,
        None,
        None,
        git,
        on_progress,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_json_with_body(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    parent: Option<&str>,
    body: Option<&str>,
    git: &dyn GitRefOps,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<String> {
    let created = run_with_body_full(
        root,
        config,
        store,
        doc_type,
        title,
        author,
        parent,
        body,
        git,
        on_progress,
    )?;
    let relative = created
        .path
        .strip_prefix(root)
        .unwrap_or(&created.path)
        .to_path_buf();

    let content = fs::read_to_string(&created.path)?;
    let mut meta = DocMeta::parse(&content)?;
    meta.path = relative;
    // Derive the assigned id from the written path exactly as the store does
    // on load; DocMeta::parse leaves it empty (AUDIT-018 F5).
    meta.id = crate::engine::store::extract_id(&meta.path);
    // A directory template's parts/sidecars (RFC-074 AC2): `DocMeta::parse`
    // only sees the just-written file, not its siblings, so they are carried
    // separately from `create_document`'s own scan rather than re-derived
    // from a full store load.
    meta.parts = created.parts;
    meta.sidecars = created.sidecars;

    let mut json = doc_to_json(&meta);
    merge_push_outcome(&mut json, &created.push_outcome);
    Ok(serde_json::to_string_pretty(&json)?)
}
