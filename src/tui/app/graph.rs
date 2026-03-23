use crate::engine::document::RelationType;
use crate::engine::store::Store;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::GraphNode;

pub fn traverse_dependency_chain(
    store: &Store,
    path: &Path,
    depth: usize,
    nodes: &mut Vec<GraphNode>,
    visited: &mut HashSet<PathBuf>,
) {
    if !visited.insert(path.to_path_buf()) {
        return;
    }

    let Some(doc) = store.get(path) else {
        return;
    };

    nodes.push(GraphNode {
        path: doc.path.clone(),
        title: doc.title.clone(),
        doc_type: doc.doc_type.clone(),
        status: doc.status.clone(),
        depth,
    });

    let mut children: Vec<&PathBuf> = store
        .referenced_by(path)
        .into_iter()
        .filter(|(rel, _)| **rel == RelationType::Implements)
        .map(|(_, p)| p)
        .collect();
    children.sort_by(|a, b| {
        let a_title = store.get(a).map(|d| d.title.as_str()).unwrap_or("");
        let b_title = store.get(b).map(|d| d.title.as_str()).unwrap_or("");
        a_title.cmp(b_title)
    });

    for child_path in children {
        traverse_dependency_chain(store, child_path, depth + 1, nodes, visited);
    }
}
