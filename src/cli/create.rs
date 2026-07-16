use crate::cli::json::doc_to_json;
use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::reservation;
use crate::engine::store::Store;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub use crate::engine::ops::create::{run, run_with_body};

pub fn run_json(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
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
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<String> {
    let path = run_with_body(
        root,
        config,
        store,
        doc_type,
        title,
        author,
        parent,
        body,
        on_progress,
    )?;
    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

    let content = fs::read_to_string(&path)?;
    let mut meta = DocMeta::parse(&content)?;
    meta.path = relative;
    // Derive the assigned id from the written path exactly as the store does
    // on load; DocMeta::parse leaves it empty (AUDIT-018 F5).
    meta.id = crate::engine::store::extract_id(&meta.path);

    let json = doc_to_json(&meta);
    Ok(serde_json::to_string_pretty(&json)?)
}
