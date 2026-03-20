use crate::engine::document::DocMeta;
use crate::engine::store::{ResolveError, Store};
use std::path::{Path, PathBuf};

pub fn resolve_shorthand_or_path<'a>(
    store: &'a Store,
    id: &str,
) -> Result<&'a DocMeta, ResolveError> {
    if let Some(doc) = store.get(Path::new(id)) {
        return Ok(doc);
    }
    store.resolve_shorthand(id)
}

pub fn resolve_to_path(store: &Store, id: &str) -> Result<PathBuf, ResolveError> {
    let doc = resolve_shorthand_or_path(store, id)?;
    Ok(doc.path.clone())
}
