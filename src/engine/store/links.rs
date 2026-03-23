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

    pub(super) fn rebuild_links(&mut self) {
        self.forward_links.clear();
        self.reverse_links.clear();
        for (path, meta) in &self.docs {
            for rel in &meta.related {
                let target = PathBuf::from(&rel.target);
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
    }

    pub(super) fn build_links(
        docs: &HashMap<PathBuf, DocMeta>,
    ) -> (
        HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
        HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    ) {
        let mut forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();
        let mut reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();

        for (path, meta) in docs {
            for rel in &meta.related {
                let target = PathBuf::from(&rel.target);
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
}
