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
    pub(crate) docs: HashMap<PathBuf, DocMeta>,
    forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    pub(crate) reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
}

impl Store {
    pub fn load(root: &Path, config: &Config) -> Result<Self> {
        let mut docs = HashMap::new();

        let dirs = [
            &config.directories.rfcs,
            &config.directories.adrs,
            &config.directories.stories,
            &config.directories.iterations,
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

    pub fn referenced_by(&self, path: &Path) -> Vec<(&RelationType, &PathBuf)> {
        match self.reverse_links.get(path) {
            Some(rev) => rev.iter().map(|(rel, src)| (rel, src)).collect(),
            None => Vec::new(),
        }
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

    pub fn validate_full(&self) -> crate::engine::validation::ValidationResult {
        crate::engine::validation::validate_full(self)
    }

    pub fn search(&self, query: &str) -> Vec<SearchResult<'_>> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for meta in self.docs.values() {
            if meta.title.to_lowercase().contains(&query_lower) {
                results.push(SearchResult {
                    doc: meta,
                    match_field: "title",
                    snippet: meta.title.clone(),
                });
                continue;
            }

            if meta.tags.iter().any(|t| t.to_lowercase().contains(&query_lower)) {
                let matched_tag = meta
                    .tags
                    .iter()
                    .find(|t| t.to_lowercase().contains(&query_lower))
                    .unwrap();
                results.push(SearchResult {
                    doc: meta,
                    match_field: "tag",
                    snippet: matched_tag.clone(),
                });
                continue;
            }

            if let Ok(body) = self.get_body(&meta.path) {
                let body_lower = body.to_lowercase();
                if let Some(pos) = body_lower.find(&query_lower) {
                    let start = pos.saturating_sub(40);
                    let end = (pos + query.len() + 40).min(body.len());
                    let snippet = body[start..end].to_string();
                    results.push(SearchResult {
                        doc: meta,
                        match_field: "body",
                        snippet,
                    });
                }
            }
        }

        results.sort_by(|a, b| a.doc.path.cmp(&b.doc.path));
        results
    }
}

#[derive(Debug)]
pub struct SearchResult<'a> {
    pub doc: &'a DocMeta,
    pub match_field: &'static str,
    pub snippet: String,
}
