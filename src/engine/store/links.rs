use crate::engine::document::{DocMeta, RelationType};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::Store;

impl Store {
    pub fn related_to(&self, path: &Path) -> Vec<(&RelationType, &PathBuf)> {
        let mut results = Vec::new();
        if let Some(fwd) = self.forward_links.get(path) {
            for (rel, target) in fwd {
                results.push((rel, target));
            }
        }
        if let Some(rev) = self.reverse_links.get(path) {
            for (rel, source) in rev {
                results.push((rel, source));
            }
        }
        results
    }

    pub fn referenced_by(&self, path: &Path) -> Vec<(&RelationType, &PathBuf)> {
        match self.reverse_links.get(path) {
            Some(rev) => rev.iter().map(|(rel, src)| (rel, src)).collect(),
            None => Vec::new(),
        }
    }

    pub(super) fn propagate_parent_links(&mut self) {
        for (child_path, parent_path) in &self.parent_of {
            let Some(parent_links) = self.forward_links.get(parent_path).cloned() else {
                continue;
            };
            for (rel_type, target) in &parent_links {
                self.forward_links
                    .entry(child_path.clone())
                    .or_default()
                    .push((rel_type.clone(), target.clone()));
                self.reverse_links
                    .entry(target.clone())
                    .or_default()
                    .push((rel_type.clone(), child_path.clone()));
            }
        }
    }

    pub(super) fn rebuild_links(&mut self) {
        self.forward_links.clear();
        self.reverse_links.clear();
        let id_to_path: HashMap<String, PathBuf> = self
            .docs
            .values()
            .map(|doc| (doc.id.clone(), doc.path.clone()))
            .collect();
        for (path, meta) in &self.docs {
            for rel in &meta.related {
                let Some(target) = Self::resolve_target(&rel.target, &id_to_path) else {
                    continue;
                };
                self.forward_links
                    .entry(path.clone())
                    .or_default()
                    .push((rel.rel_type.clone(), target.clone()));
                self.reverse_links
                    .entry(target)
                    .or_default()
                    .push((rel.rel_type.clone(), path.clone()));
            }
        }
        self.propagate_parent_links();
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn build_links(
        docs: &HashMap<PathBuf, DocMeta>,
    ) -> (
        HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
        HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    ) {
        let mut forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();
        let mut reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();

        let id_to_path: HashMap<String, PathBuf> = docs
            .values()
            .map(|doc| (doc.id.clone(), doc.path.clone()))
            .collect();

        for (path, meta) in docs {
            for rel in &meta.related {
                let Some(target) = Self::resolve_target(&rel.target, &id_to_path) else {
                    continue;
                };
                forward_links
                    .entry(path.clone())
                    .or_default()
                    .push((rel.rel_type.clone(), target.clone()));
                reverse_links
                    .entry(target)
                    .or_default()
                    .push((rel.rel_type.clone(), path.clone()));
            }
        }

        (forward_links, reverse_links)
    }

    fn resolve_target(target: &str, id_to_path: &HashMap<String, PathBuf>) -> Option<PathBuf> {
        if let Some(path) = id_to_path.get(target) {
            return Some(path.clone());
        }
        // Fall back to treating it as a path (for legacy/path-based targets)
        let path = PathBuf::from(target);
        Some(path)
    }
}
