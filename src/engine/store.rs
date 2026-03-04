use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType, RelationType, Status};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Filter {
    pub doc_type: Option<DocType>,
    pub status: Option<Status>,
    pub tag: Option<String>,
}

pub struct Store {
    root: PathBuf,
    docs: HashMap<PathBuf, DocMeta>,
    forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
}

impl Store {
    pub fn load(root: &Path, config: &Config) -> Result<Self> {
        let mut docs = HashMap::new();

        let dirs = [
            &config.directories.rfcs,
            &config.directories.adrs,
            &config.directories.specs,
            &config.directories.plans,
        ];

        for dir in &dirs {
            let full_path = root.join(dir);
            if !full_path.exists() {
                continue;
            }
            for entry in fs::read_dir(&full_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let content = fs::read_to_string(&path)?;
                if let Ok(mut meta) = DocMeta::parse(&content) {
                    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                    meta.path = relative;
                    docs.insert(meta.path.clone(), meta);
                }
            }
        }

        let mut forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();
        let mut reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>> = HashMap::new();

        for (path, meta) in &docs {
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

        Ok(Store {
            root: root.to_path_buf(),
            docs,
            forward_links,
            reverse_links,
        })
    }

    pub fn all_docs(&self) -> Vec<&DocMeta> {
        self.docs.values().collect()
    }

    pub fn list(&self, filter: &Filter) -> Vec<&DocMeta> {
        self.docs
            .values()
            .filter(|d| {
                if let Some(ref dt) = filter.doc_type {
                    if &d.doc_type != dt {
                        return false;
                    }
                }
                if let Some(ref s) = filter.status {
                    if &d.status != s {
                        return false;
                    }
                }
                if let Some(ref tag) = filter.tag {
                    if !d.tags.contains(tag) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn get(&self, path: &Path) -> Option<&DocMeta> {
        self.docs.get(path)
    }

    pub fn get_body(&self, path: &Path) -> Result<String> {
        let full_path = self.root.join(path);
        let content = fs::read_to_string(&full_path)?;
        DocMeta::extract_body(&content)
    }

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

    pub fn resolve_shorthand(&self, id: &str) -> Option<&DocMeta> {
        self.docs.values().find(|d| {
            d.path
                .file_name()
                .and_then(|f| f.to_str())
                .map(|f| f.starts_with(id))
                .unwrap_or(false)
        })
    }

    pub fn reload_file(&mut self, root: &Path, relative_path: &Path) -> Result<()> {
        let full_path = root.join(relative_path);
        if !full_path.exists() {
            self.docs.remove(relative_path);
            self.rebuild_links();
            return Ok(());
        }

        let content = std::fs::read_to_string(&full_path)?;
        if let Ok(mut meta) = DocMeta::parse(&content) {
            meta.path = relative_path.to_path_buf();
            self.docs.insert(relative_path.to_path_buf(), meta);
        }
        self.rebuild_links();
        Ok(())
    }

    pub fn remove_file(&mut self, relative_path: &Path) {
        self.docs.remove(relative_path);
        self.rebuild_links();
    }

    fn rebuild_links(&mut self) {
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for (path, meta) in &self.docs {
            for rel in &meta.related {
                let target = PathBuf::from(&rel.target);
                if !self.docs.contains_key(&target) {
                    errors.push(ValidationError::BrokenLink {
                        source: path.clone(),
                        target,
                    });
                }
            }
        }
        errors
    }
}

#[derive(Debug)]
pub enum ValidationError {
    BrokenLink { source: PathBuf, target: PathBuf },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::BrokenLink { source, target } => {
                write!(
                    f,
                    "broken link: {} -> {}",
                    source.display(),
                    target.display()
                )
            }
        }
    }
}
