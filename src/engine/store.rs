mod links;
mod loader;

use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{DocMeta, DocType, RelationType, Status};
use crate::engine::fs::{FileSystem, RealFileSystem};
use crate::engine::refs::RefExpander;
use anyhow::Result;
use std::collections::HashMap;
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
        Self::load_with_fs(root, config, &RealFileSystem)
    }

    pub fn load_with_fs(root: &Path, config: &Config, fs: &dyn FileSystem) -> Result<Self> {
        let mut docs = HashMap::new();
        let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut parent_of: HashMap<PathBuf, PathBuf> = HashMap::new();
        let mut parse_errors: Vec<ParseError> = Vec::new();

        for type_def in &config.documents.types {
            let full_path = if type_def.store == StoreBackend::GithubIssues {
                root.join(".lazyspec/cache").join(&type_def.name)
            } else {
                root.join(&type_def.dir)
            };
            if !fs.exists(&full_path) {
                continue;
            }
            loader::load_type_directory(
                root,
                &full_path,
                type_def,
                &mut docs,
                &mut children,
                &mut parent_of,
                &mut parse_errors,
                fs,
            )?;
        }

        let (forward_links, reverse_links) = Self::build_links(&docs);

        let mut store = Store {
            root: root.to_path_buf(),
            docs,
            forward_links,
            reverse_links,
            children,
            parent_of,
            parse_errors,
        };
        store.propagate_parent_links();

        Ok(store)
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

    pub fn get_body_raw(&self, path: &Path, fs: &dyn FileSystem) -> Result<String> {
        let full_path = self.root.join(path);
        let content = fs.read_to_string(&full_path)?;
        DocMeta::extract_body(&content)
    }

    pub fn get_body_expanded(
        &self,
        path: &Path,
        max_lines: usize,
        fs: &dyn FileSystem,
    ) -> Result<String> {
        let body = self.get_body_raw(path, fs)?;
        let expander = RefExpander::with_max_lines(self.root.clone(), max_lines);
        expander.expand(&body)
    }

    pub fn get_body(&self, path: &Path, fs: &dyn FileSystem) -> Result<String> {
        self.get_body_raw(path, fs)
    }

    pub fn resolve_shorthand(&self, id: &str) -> Result<&DocMeta, ResolveError> {
        let Some((parent_id, child_stem)) = id.split_once('/') else {
            return self.resolve_unqualified(id);
        };

        let parent = self
            .docs
            .values()
            .find(|d| {
                !self.parent_of.contains_key(&d.path)
                    && canonical_name(&d.path)
                        .map(|n| n.starts_with(parent_id))
                        .unwrap_or(false)
            })
            .ok_or_else(|| ResolveError::NotFound(id.to_string()))?;

        let child_paths = self
            .children
            .get(&parent.path)
            .ok_or_else(|| ResolveError::NotFound(id.to_string()))?;

        child_paths
            .iter()
            .find_map(|cp| {
                let stem = cp.file_stem().and_then(|f| f.to_str())?;
                if stem.starts_with(child_stem) {
                    self.docs.get(cp)
                } else {
                    None
                }
            })
            .ok_or_else(|| ResolveError::NotFound(id.to_string()))
    }

    fn resolve_unqualified(&self, id: &str) -> Result<&DocMeta, ResolveError> {
        let matches: Vec<&DocMeta> = self
            .docs
            .values()
            .filter(|d| {
                !self.parent_of.contains_key(&d.path)
                    && canonical_name(&d.path)
                        .map(|n| n.starts_with(id))
                        .unwrap_or(false)
            })
            .collect();

        match matches.len() {
            0 => Err(ResolveError::NotFound(id.to_string())),
            1 => Ok(matches[0]),
            _ => {
                let paths: Vec<PathBuf> = matches.iter().map(|d| d.path.clone()).collect();
                Err(ResolveError::Ambiguous {
                    id: id.to_string(),
                    matches: paths,
                })
            }
        }
    }

    pub fn reload_file(
        &mut self,
        root: &Path,
        relative_path: &Path,
        fs: &dyn FileSystem,
    ) -> Result<()> {
        let full_path = root.join(relative_path);
        if !fs.exists(&full_path) {
            self.docs.remove(relative_path);
            self.rebuild_links();
            return Ok(());
        }

        let content = fs.read_to_string(&full_path)?;
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn children_of(&self, path: &Path) -> &[PathBuf] {
        self.children.get(path).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn parent_of(&self, path: &Path) -> Option<&PathBuf> {
        self.parent_of.get(path)
    }

    pub fn forward_links_for(&self, path: &Path) -> &[(RelationType, PathBuf)] {
        self.forward_links
            .get(path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn reverse_links_for(&self, path: &Path) -> &[(RelationType, PathBuf)] {
        self.reverse_links
            .get(path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn validate_full(&self, config: &Config) -> crate::engine::validation::ValidationResult {
        crate::engine::validation::validate_full(self, config)
    }

    pub fn search(&self, query: &str, fs: &dyn FileSystem) -> Vec<SearchResult<'_>> {
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

            if let Ok(body) = self.get_body_raw(&meta.path, fs) {
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

        results.sort_by(|a, b| DocMeta::sort_by_date(a.doc, b.doc));
        results
    }
}

fn canonical_name(path: &Path) -> Option<&str> {
    let file_name = path.file_name().and_then(|f| f.to_str())?;
    if file_name == "index.md" || file_name == ".virtual" {
        return path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|f| f.to_str());
    }
    Some(file_name)
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

    if let Some(parent) = path.parent() {
        let parent_name = parent.file_name().and_then(|f| f.to_str()).unwrap_or("");
        let parent_id = extract_id_from_name(parent_name);
        if parent_id != parent_name {
            return stem.to_string();
        }
    }

    extract_id_from_name(stem)
}

fn strip_type_prefix_sqids(name: &str) -> &str {
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
    let stripped = strip_type_prefix_sqids(name);
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

impl std::error::Error for ResolveError {}

#[derive(Debug)]
pub struct SearchResult<'a> {
    pub doc: &'a DocMeta,
    pub match_field: &'static str,
    pub snippet: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use crate::engine::fs::FileSystem;
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;

    struct InMemoryFileSystem {
        files: Mutex<StdHashMap<PathBuf, String>>,
        dirs: Mutex<Vec<PathBuf>>,
    }

    impl InMemoryFileSystem {
        fn new() -> Self {
            Self {
                files: Mutex::new(StdHashMap::new()),
                dirs: Mutex::new(Vec::new()),
            }
        }

        fn add_file(&self, path: impl Into<PathBuf>, content: &str) {
            self.files
                .lock()
                .unwrap()
                .insert(path.into(), content.to_string());
        }

        fn add_dir(&self, path: impl Into<PathBuf>) {
            self.dirs.lock().unwrap().push(path.into());
        }
    }

    impl FileSystem for InMemoryFileSystem {
        fn read_to_string(&self, path: &Path) -> Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("file not found: {}", path.display()))
        }

        fn write(&self, path: &Path, contents: &str) -> Result<()> {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), contents.to_string());
            Ok(())
        }

        fn rename(&self, _from: &Path, _to: &Path) -> Result<()> {
            unimplemented!("rename not needed for load tests")
        }

        fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>> {
            let files = self.files.lock().unwrap();
            let dirs = self.dirs.lock().unwrap();

            let mut entries: Vec<PathBuf> = files
                .keys()
                .filter(|p| p.parent() == Some(path))
                .cloned()
                .collect();

            for d in dirs.iter() {
                if d.parent() == Some(path) {
                    entries.push(d.clone());
                }
            }

            Ok(entries)
        }

        fn exists(&self, path: &Path) -> bool {
            let files = self.files.lock().unwrap();
            let dirs = self.dirs.lock().unwrap();
            files.contains_key(path) || dirs.contains(&path.to_path_buf())
        }

        fn create_dir_all(&self, path: &Path) -> Result<()> {
            self.dirs.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.lock().unwrap().contains(&path.to_path_buf())
        }
    }

    #[test]
    fn test_load_with_in_memory_filesystem() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");

        let rfc_dir = root.join("docs/rfcs");
        fs.add_dir(rfc_dir.clone());

        let rfc1_path = rfc_dir.join("RFC-001-first.md");
        fs.add_file(
            &rfc1_path,
            concat!(
                "---\n",
                "title: \"First RFC\"\n",
                "type: rfc\n",
                "status: draft\n",
                "author: \"test\"\n",
                "date: 2026-01-01\n",
                "tags: []\n",
                "---\n",
                "Body of first RFC.\n",
            ),
        );

        let rfc2_path = rfc_dir.join("RFC-002-second.md");
        fs.add_file(
            &rfc2_path,
            concat!(
                "---\n",
                "title: \"Second RFC\"\n",
                "type: rfc\n",
                "status: accepted\n",
                "author: \"test\"\n",
                "date: 2026-01-02\n",
                "tags: [\"important\"]\n",
                "---\n",
                "Body of second RFC.\n",
            ),
        );

        let config = Config::default();
        let store = Store::load_with_fs(&root, &config, &fs).unwrap();

        assert_eq!(store.docs.len(), 2);

        let doc1 = store.get(&PathBuf::from("docs/rfcs/RFC-001-first.md"));
        assert!(doc1.is_some());
        assert_eq!(doc1.unwrap().title, "First RFC");
        assert_eq!(doc1.unwrap().id, "RFC-001");

        let doc2 = store.get(&PathBuf::from("docs/rfcs/RFC-002-second.md"));
        assert!(doc2.is_some());
        assert_eq!(doc2.unwrap().title, "Second RFC");
        assert_eq!(doc2.unwrap().id, "RFC-002");
    }

    fn github_issues_config() -> Config {
        use crate::engine::config::{NumberingStrategy, StoreBackend, TypeDef};

        let issue_type = TypeDef {
            name: "issue".to_string(),
            plural: "issues".to_string(),
            dir: "docs/issues".to_string(),
            prefix: "ISSUE".to_string(),
            icon: Some("◉".to_string()),
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store: StoreBackend::GithubIssues,
            singleton: false,
            parent_type: None,
        };

        let mut config = Config::default();
        config.documents.types.push(issue_type);
        config
    }

    #[test]
    fn test_load_includes_github_issues_cache() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");

        let cache_dir = root.join(".lazyspec/cache/issue");
        fs.add_dir(cache_dir.clone());

        let issue_path = cache_dir.join("ISSUE-042-login-broken.md");
        fs.add_file(
            &issue_path,
            concat!(
                "---\n",
                "title: \"Login broken\"\n",
                "type: issue\n",
                "status: draft\n",
                "author: \"alice\"\n",
                "date: 2026-03-01\n",
                "tags: [\"bug\"]\n",
                "---\n",
                "The login page returns 500.\n",
            ),
        );

        let config = github_issues_config();
        let store = Store::load_with_fs(&root, &config, &fs).unwrap();

        assert_eq!(store.docs.len(), 1);

        let rel = PathBuf::from(".lazyspec/cache/issue/ISSUE-042-login-broken.md");
        let doc = store.get(&rel);
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().title, "Login broken");
        assert_eq!(doc.unwrap().id, "ISSUE-042");
    }

    #[test]
    fn test_show_works_for_cached_github_issues_doc() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");

        let cache_dir = root.join(".lazyspec/cache/issue");
        fs.add_dir(cache_dir.clone());

        let issue_path = cache_dir.join("ISSUE-007-fix-auth.md");
        fs.add_file(
            &issue_path,
            concat!(
                "---\n",
                "title: \"Fix auth\"\n",
                "type: issue\n",
                "status: draft\n",
                "author: \"bob\"\n",
                "date: 2026-03-15\n",
                "tags: []\n",
                "---\n",
                "Auth tokens expire too quickly.\n",
            ),
        );

        let config = github_issues_config();
        let store = Store::load_with_fs(&root, &config, &fs).unwrap();

        let rel = PathBuf::from(".lazyspec/cache/issue/ISSUE-007-fix-auth.md");
        let body = store.get_body_raw(&rel, &fs).unwrap();
        assert_eq!(body.trim(), "Auth tokens expire too quickly.");
    }

    #[test]
    fn test_resolve_shorthand_finds_cached_doc() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");

        let cache_dir = root.join(".lazyspec/cache/issue");
        fs.add_dir(cache_dir.clone());

        let issue_path = cache_dir.join("ISSUE-001-example.md");
        fs.add_file(
            &issue_path,
            concat!(
                "---\n",
                "title: \"Example issue\"\n",
                "type: issue\n",
                "status: draft\n",
                "author: \"carol\"\n",
                "date: 2026-03-20\n",
                "tags: []\n",
                "---\n",
                "An example cached issue.\n",
            ),
        );

        let config = github_issues_config();
        let store = Store::load_with_fs(&root, &config, &fs).unwrap();

        let doc = store
            .resolve_shorthand("ISSUE-001")
            .expect("should resolve cached doc");
        assert_eq!(doc.title, "Example issue");
        assert_eq!(doc.id, "ISSUE-001");
        assert_eq!(
            doc.path,
            PathBuf::from(".lazyspec/cache/issue/ISSUE-001-example.md")
        );
    }
}
