use crate::engine::document::{DocMeta, RelationType};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::Store;

/// One resolved relation as the link maps hold it: the document at the far end,
/// and the document whose frontmatter stated the relation.
///
/// The two are usually the obvious pair, but they come apart:
/// [`Store::propagate_parent_links`] lends a parent's links to its nested
/// children, so a child's `forward_links` carry relations the child never
/// stated. `declared_by` is recorded at build time because a reader cannot
/// recover it -- an inherited link is indistinguishable from an own one once
/// pushed onto the same vector -- and the edge that has to admit a link is the
/// declaring document's (ADR-034).
#[derive(Debug, Clone)]
pub struct Link {
    pub rel_type: RelationType,
    /// The far end: the target for a forward link, the source for a reverse one.
    pub endpoint: PathBuf,
    pub declared_by: PathBuf,
}

impl Store {
    pub fn related_to(&self, path: &Path) -> Vec<(&RelationType, &PathBuf)> {
        let mut results = Vec::new();
        if let Some(fwd) = self.forward_links.get(path) {
            for link in fwd {
                results.push((&link.rel_type, &link.endpoint));
            }
        }
        if let Some(rev) = self.reverse_links.get(path) {
            for link in rev {
                results.push((&link.rel_type, &link.endpoint));
            }
        }
        results
    }

    pub fn referenced_by(&self, path: &Path) -> Vec<(&RelationType, &PathBuf)> {
        match self.reverse_links.get(path) {
            Some(rev) => rev
                .iter()
                .map(|link| (&link.rel_type, &link.endpoint))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Lends every link a parent declared to each of its nested children, so a
    /// child inherits its parent's annotations. The lent copy keeps the parent
    /// as `declared_by`: the child is inheriting a relation, not stating one.
    pub(super) fn propagate_parent_links(&mut self) {
        for (child_path, parent_path) in &self.parent_of {
            let Some(parent_links) = self.forward_links.get(parent_path).cloned() else {
                continue;
            };
            for link in &parent_links {
                self.forward_links
                    .entry(child_path.clone())
                    .or_default()
                    .push(link.clone());
                self.reverse_links
                    .entry(link.endpoint.clone())
                    .or_default()
                    .push(Link {
                        rel_type: link.rel_type.clone(),
                        endpoint: child_path.clone(),
                        declared_by: link.declared_by.clone(),
                    });
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
                    .push(Link {
                        rel_type: rel.rel_type.clone(),
                        endpoint: target.clone(),
                        declared_by: path.clone(),
                    });
                self.reverse_links.entry(target).or_default().push(Link {
                    rel_type: rel.rel_type.clone(),
                    endpoint: path.clone(),
                    declared_by: path.clone(),
                });
            }
        }
        self.propagate_parent_links();
    }

    pub(super) fn build_links(
        docs: &HashMap<PathBuf, DocMeta>,
    ) -> (HashMap<PathBuf, Vec<Link>>, HashMap<PathBuf, Vec<Link>>) {
        let mut forward_links: HashMap<PathBuf, Vec<Link>> = HashMap::new();
        let mut reverse_links: HashMap<PathBuf, Vec<Link>> = HashMap::new();

        let id_to_path: HashMap<String, PathBuf> = docs
            .values()
            .map(|doc| (doc.id.clone(), doc.path.clone()))
            .collect();

        for (path, meta) in docs {
            for rel in &meta.related {
                let Some(target) = Self::resolve_target(&rel.target, &id_to_path) else {
                    continue;
                };
                forward_links.entry(path.clone()).or_default().push(Link {
                    rel_type: rel.rel_type.clone(),
                    endpoint: target.clone(),
                    declared_by: path.clone(),
                });
                reverse_links.entry(target).or_default().push(Link {
                    rel_type: rel.rel_type.clone(),
                    endpoint: path.clone(),
                    declared_by: path.clone(),
                });
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
