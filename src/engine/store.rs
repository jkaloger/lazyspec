use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType, RelationType, Status};
use crate::engine::refs::RefExpander;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Default)]
pub struct Filter {
    pub doc_type: Option<DocType>,
    pub status: Option<Status>,
    pub tag: Option<String>,
}

pub struct Store {
    pub(crate) root: PathBuf,
    pub(crate) docs: HashMap<PathBuf, DocMeta>,
    pub(crate) forward_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    pub(crate) reverse_links: HashMap<PathBuf, Vec<(RelationType, PathBuf)>>,
    pub(crate) children: HashMap<PathBuf, Vec<PathBuf>>,
    pub(crate) parent_of: HashMap<PathBuf, PathBuf>,
    pub(crate) parse_errors: Vec<ParseError>,
}

impl Store {
    pub fn load(root: &Path, config: &Config) -> Result<Self> {
        let mut docs = HashMap::new();
        let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut parent_of: HashMap<PathBuf, PathBuf> = HashMap::new();
        let mut parse_errors: Vec<ParseError> = Vec::new();

        for type_def in &config.types {
            let full_path = root.join(&type_def.dir);
            if !full_path.exists() {
                continue;
            }
            for entry in fs::read_dir(&full_path)? {
                let entry = entry?;
                let path = entry.path();

                if entry.file_type()?.is_dir() {
                    let index_path = path.join("index.md");
                    if index_path.exists() {
                        let content = fs::read_to_string(&index_path)?;
                        let parent_relative = index_path
                            .strip_prefix(root)
                            .unwrap_or(&index_path)
                            .to_path_buf();
                        match DocMeta::parse(&content) {
                            Ok(mut meta) => {
                                meta.path = parent_relative.clone();
                                meta.id = extract_id(&meta.path);
                                docs.insert(meta.path.clone(), meta);
                            }
                            Err(e) => {
                                parse_errors.push(ParseError {
                                    path: parent_relative.clone(),
                                    error: e.to_string(),
                                });
                            }
                        }

                        let mut child_paths = Vec::new();
                        for child_entry in fs::read_dir(&path)? {
                            let child_entry = child_entry?;
                            if child_entry.file_type()?.is_dir() {
                                continue;
                            }
                            let child_path = child_entry.path();
                            if child_path.file_name().and_then(|f| f.to_str()) == Some("index.md") {
                                continue;
                            }
                            if child_path.extension().and_then(|e| e.to_str()) != Some("md") {
                                continue;
                            }
                            let child_content = fs::read_to_string(&child_path)?;
                            let child_relative = child_path
                                .strip_prefix(root)
                                .unwrap_or(&child_path)
                                .to_path_buf();
                            match DocMeta::parse(&child_content) {
                                Ok(mut child_meta) => {
                                    child_meta.path = child_relative.clone();
                                    child_meta.id = extract_id(&child_meta.path);
                                    parent_of
                                        .insert(child_relative.clone(), parent_relative.clone());
                                    child_paths.push(child_relative.clone());
                                    docs.insert(child_meta.path.clone(), child_meta);
                                }
                                Err(e) => {
                                    parse_errors.push(ParseError {
                                        path: child_relative,
                                        error: e.to_string(),
                                    });
                                }
                            }
                        }
                        if !child_paths.is_empty() {
                            children.insert(parent_relative, child_paths);
                        }
                    } else {
                        let mut child_paths = Vec::new();
                        for child_entry in fs::read_dir(&path)? {
                            let child_entry = child_entry?;
                            if child_entry.file_type()?.is_dir() {
                                continue;
                            }
                            let child_path = child_entry.path();
                            if child_path.extension().and_then(|e| e.to_str()) != Some("md") {
                                continue;
                            }
                            let child_content = fs::read_to_string(&child_path)?;
                            let child_relative = child_path
                                .strip_prefix(root)
                                .unwrap_or(&child_path)
                                .to_path_buf();
                            match DocMeta::parse(&child_content) {
                                Ok(mut child_meta) => {
                                    child_meta.path = child_relative.clone();
                                    child_meta.id = extract_id(&child_meta.path);
                                    child_paths.push(child_relative.clone());
                                    docs.insert(child_meta.path.clone(), child_meta);
                                }
                                Err(e) => {
                                    parse_errors.push(ParseError {
                                        path: child_relative,
                                        error: e.to_string(),
                                    });
                                }
                            }
                        }

                        if !child_paths.is_empty() {
                            let folder_name =
                                path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                            let folder_relative = path.strip_prefix(root).unwrap_or(&path);
                            let parent_relative = folder_relative.join(".virtual");

                            let all_accepted = child_paths.iter().all(|cp| {
                                docs.get(cp)
                                    .map(|d| d.status == Status::Accepted)
                                    .unwrap_or(false)
                            });

                            let virtual_id = extract_id(&parent_relative);
                            let virtual_meta = DocMeta {
                                path: parent_relative.clone(),
                                title: title_from_folder_name(folder_name),
                                doc_type: DocType::new(&type_def.name),
                                status: if all_accepted {
                                    Status::Accepted
                                } else {
                                    Status::Draft
                                },
                                author: "".to_string(),
                                date: Utc::now().date_naive(),
                                tags: vec![],
                                related: vec![],
                                validate_ignore: false,
                                virtual_doc: true,
                                id: virtual_id,
                            };
                            docs.insert(parent_relative.clone(), virtual_meta);

                            for cp in &child_paths {
                                parent_of.insert(cp.clone(), parent_relative.clone());
                            }
                            children.insert(parent_relative, child_paths);
                        }
                    }

                    continue;
                }

                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let content = fs::read_to_string(&path)?;
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                match DocMeta::parse(&content) {
                    Ok(mut meta) => {
                        meta.path = relative;
                        meta.id = extract_id(&meta.path);
                        docs.insert(meta.path.clone(), meta);
                    }
                    Err(e) => {
                        parse_errors.push(ParseError {
                            path: relative,
                            error: e.to_string(),
                        });
                    }
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
            children,
            parent_of,
            parse_errors,
        })
    }

    pub fn all_docs(&self) -> Vec<&DocMeta> {
        self.docs.values().collect()
    }

    pub fn parse_errors(&self) -> &[ParseError] {
        &self.parse_errors
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

    pub fn get_body_raw(&self, path: &Path) -> Result<String> {
        let full_path = self.root.join(path);
        let content = fs::read_to_string(&full_path)?;
        DocMeta::extract_body(&content)
    }

    pub fn get_body_expanded(&self, path: &Path, max_lines: usize) -> Result<String> {
        let body = self.get_body_raw(path)?;
        let expander = RefExpander::with_max_lines(self.root.clone(), max_lines);
        expander.expand(&body)
    }

    pub fn get_body(&self, path: &Path) -> Result<String> {
        self.get_body_raw(path)
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

    pub fn resolve_shorthand(&self, id: &str) -> Result<&DocMeta, ResolveError> {
        if let Some((parent_id, child_stem)) = id.split_once('/') {
            // Qualified: find parent first (among non-children only)
            let parent = self.docs.values().find(|d| {
                if self.parent_of.contains_key(&d.path) {
                    return false;
                }
                let name = if d.path.file_name().and_then(|f| f.to_str()) == Some("index.md")
                    || d.path.file_name().and_then(|f| f.to_str()) == Some(".virtual")
                {
                    d.path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|f| f.to_str())
                } else {
                    d.path.file_name().and_then(|f| f.to_str())
                };
                name.map(|n| n.starts_with(parent_id)).unwrap_or(false)
            }).ok_or_else(|| ResolveError::NotFound(id.to_string()))?;
            // Then find child within parent's children
            let child_paths = self.children.get(&parent.path)
                .ok_or_else(|| ResolveError::NotFound(id.to_string()))?;
            child_paths.iter().find_map(|cp| {
                let stem = cp.file_stem().and_then(|f| f.to_str())?;
                if stem.starts_with(child_stem) {
                    self.docs.get(cp)
                } else {
                    None
                }
            }).ok_or_else(|| ResolveError::NotFound(id.to_string()))
        } else {
            // Unqualified: collect all matches, excluding children
            let matches: Vec<&DocMeta> = self.docs.values().filter(|d| {
                if self.parent_of.contains_key(&d.path) {
                    return false;
                }
                let name = if d.path.file_name().and_then(|f| f.to_str()) == Some("index.md")
                    || d.path.file_name().and_then(|f| f.to_str()) == Some(".virtual")
                {
                    d.path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|f| f.to_str())
                } else {
                    d.path.file_name().and_then(|f| f.to_str())
                };
                name.map(|n| n.starts_with(id)).unwrap_or(false)
            }).collect();

            match matches.len() {
                0 => Err(ResolveError::NotFound(id.to_string())),
                1 => Ok(matches[0]),
                _ => {
                    let paths: Vec<PathBuf> = matches.iter().map(|d| d.path.clone()).collect();
                    Err(ResolveError::Ambiguous { id: id.to_string(), matches: paths })
                }
            }
        }
    }

    pub fn reload_file(&mut self, root: &Path, relative_path: &Path) -> Result<()> {
        let full_path = root.join(relative_path);
        if !full_path.exists() {
            self.docs.remove(relative_path);
            self.rebuild_links();
            return Ok(());
        }

        let content = std::fs::read_to_string(&full_path)?;
        match DocMeta::parse(&content) {
            Ok(mut meta) => {
                meta.path = relative_path.to_path_buf();
                meta.id = extract_id(&meta.path);
                self.docs.insert(relative_path.to_path_buf(), meta);
                self.parse_errors.retain(|e| e.path != relative_path);
            }
            Err(e) => {
                self.docs.remove(relative_path);
                self.parse_errors.retain(|pe| pe.path != relative_path);
                self.parse_errors.push(ParseError {
                    path: relative_path.to_path_buf(),
                    error: e.to_string(),
                });
            }
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

    pub fn children_of(&self, path: &Path) -> &[PathBuf] {
        self.children.get(path).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn parent_of(&self, path: &Path) -> Option<&PathBuf> {
        self.parent_of.get(path)
    }

    pub fn validate_full(&self, config: &Config) -> crate::engine::validation::ValidationResult {
        crate::engine::validation::validate_full(self, config)
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

            if meta
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(&query_lower))
            {
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

            if let Ok(body) = self.get_body_raw(&meta.path) {
                let body_lower = body.to_lowercase();
                if let Some(pos) = body_lower.find(&query_lower) {
                    let start = body.floor_char_boundary(pos.saturating_sub(40));
                    let end = body.ceil_char_boundary((pos + query.len() + 40).min(body.len()));
                    let snippet = body[start..end].to_string();
                    results.push(SearchResult {
                        doc: meta,
                        match_field: "body",
                        snippet,
                    });
                }
            }
        }

        results.sort_by(|a, b| DocMeta::sort_by_date(&a.doc, &b.doc));
        results
    }

}

pub fn extract_id_from_name(name: &str) -> String {
    let parts: Vec<&str> = name.split('-').collect();
    for (i, part) in parts.iter().enumerate() {
        if !part.is_empty() && !part.chars().all(|c| c.is_ascii_uppercase()) {
            return parts[..=i].join("-");
        }
    }
    name.to_string()
}

fn extract_id(path: &Path) -> String {
    let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let stem = path.file_stem().and_then(|f| f.to_str()).unwrap_or("");

    if file_name == "index.md" || file_name == ".virtual" {
        let folder = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|f| f.to_str())
            .unwrap_or("");
        return extract_id_from_name(folder);
    }

    // Check if this is a child document (depth > 1 from type dir means it's inside a parent folder)
    // A child has a parent folder that itself has a TYPE-NNN pattern
    if let Some(parent) = path.parent() {
        let parent_name = parent.file_name().and_then(|f| f.to_str()).unwrap_or("");
        let parent_id = extract_id_from_name(parent_name);
        if parent_id != parent_name {
            // Parent has a TYPE-NNN pattern, so this is a child document
            return stem.to_string();
        }
    }

    extract_id_from_name(stem)
}

fn strip_type_prefix(name: &str) -> &str {
    let bytes = name.as_bytes();
    let mut i = 0;

    while i < bytes.len() && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    if i == 0 || i >= bytes.len() || bytes[i] != b'-' {
        return name;
    }
    i += 1;

    let id_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphanumeric() && !bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    if i == id_start || i >= bytes.len() || bytes[i] != b'-' {
        return name;
    }
    i += 1;

    &name[i..]
}

fn title_from_folder_name(name: &str) -> String {
    let stripped = strip_type_prefix(name);
    stripped
        .split('-')
        .filter(|w| !w.is_empty())
        .enumerate()
        .map(|(i, w)| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) if i == 0 => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str().to_lowercase())
                }
                Some(c) => {
                    format!(
                        "{}{}",
                        c.to_lowercase().collect::<String>(),
                        chars.as_str().to_lowercase()
                    )
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug)]
pub enum ResolveError {
    NotFound(String),
    Ambiguous { id: String, matches: Vec<PathBuf> },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotFound(id) => write!(f, "document not found: {}", id),
            ResolveError::Ambiguous { id, matches } => {
                writeln!(f, "Ambiguous ID '{}' matches multiple documents:", id)?;
                for m in matches {
                    writeln!(f, "  {}", m.display())?;
                }
                write!(f, "Specify the full path to show a specific document.")
            }
        }
    }
}

#[derive(Debug)]
pub struct SearchResult<'a> {
    pub doc: &'a DocMeta,
    pub match_field: &'static str,
    pub snippet: String,
}

