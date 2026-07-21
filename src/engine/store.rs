mod links;
mod loader;

use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{Config, StoreBackend, Traversal};
use crate::engine::document::{DocMeta, DocType, RelationType, Status};
use crate::engine::fs::{FileSystem, RealFileSystem};
use crate::engine::git_ref::GitRefOps;
use crate::engine::refs::RefExpander;
use anyhow::Result;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
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
    /// The relationship names whose `traversal == Some(Traversal::Chain)`,
    /// sourced from `config.relationships`. These form the parent-child DAG
    /// walked by [`resolve_chain`](crate::engine::context::resolve_chain) and
    /// [`resolve_forest`](crate::engine::context::resolve_forest).
    pub(crate) chain_relationships: Vec<String>,
    /// The relationship names whose `traversal == Some(Traversal::Related)`,
    /// walked by [`resolve_chain`](crate::engine::context::resolve_chain)'s
    /// related neighbourhood.
    pub(crate) related_relationships: Vec<String>,
    /// Raw document bodies memoized on first read during [`search`](Store::search),
    /// so repeated fuzzy queries (a live TUI filter re-runs on every keystroke)
    /// score body text from memory instead of re-reading each file from disk.
    /// Startup stays metadata-only (ADR-013): nothing is loaded here until a
    /// search touches the body. Entries are dropped on `reload_file`/`remove_file`
    /// so a changed body is re-read (file-watch invalidation).
    pub(crate) body_cache: std::sync::Mutex<HashMap<PathBuf, String>>,
}

impl Store {
    pub fn load(root: &Path, config: &Config) -> Result<Self> {
        let git_cli = crate::engine::git_ref::GitCli;
        Self::load_with_fs(root, config, &RealFileSystem, Some(&git_cli))
    }

    pub fn load_with_fs(
        root: &Path,
        config: &Config,
        fs: &dyn FileSystem,
        git_ref_ops: Option<&dyn GitRefOps>,
    ) -> Result<Self> {
        let mut docs = HashMap::new();
        let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut parent_of: HashMap<PathBuf, PathBuf> = HashMap::new();
        let mut parse_errors: Vec<ParseError> = Vec::new();

        for type_def in &config.documents.types {
            let full_path = match type_def.store {
                StoreBackend::GithubIssues
                | StoreBackend::GithubMilestones
                | StoreBackend::GithubProjects
                | StoreBackend::GitRef
                | StoreBackend::ClickupTasks => root.join(".lazyspec/cache").join(&type_def.name),
                _ => root.join(&type_def.dir),
            };

            if !fs.exists(&full_path) {
                if type_def.store == StoreBackend::GitRef {
                    if let Some(ops) = git_ref_ops {
                        materialize_git_ref_cache(root, &type_def.name, ops, fs)?;
                    }
                }
                if !fs.exists(&full_path) {
                    continue;
                }
            } else if type_def.store == StoreBackend::GitRef {
                let entries = fs.read_dir(&full_path)?;
                if entries.is_empty() {
                    if let Some(ops) = git_ref_ops {
                        materialize_git_ref_cache(root, &type_def.name, ops, fs)?;
                    }
                }
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

        let chain_relationships: Vec<String> = config
            .relationships
            .iter()
            .filter(|r| r.traversal == Some(Traversal::Chain))
            .map(|r| r.name.clone())
            .collect();
        let related_relationships: Vec<String> = config
            .relationships
            .iter()
            .filter(|r| r.traversal == Some(Traversal::Related))
            .map(|r| r.name.clone())
            .collect();

        let mut store = Store {
            root: root.to_path_buf(),
            docs,
            forward_links,
            reverse_links,
            children,
            parent_of,
            parse_errors,
            chain_relationships,
            related_relationships,
            body_cache: std::sync::Mutex::new(HashMap::new()),
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

        // Mirror resolve_unqualified: collect every prefix match, prefer an
        // exact parent id, and error on genuine ambiguity rather than letting
        // HashMap iteration order pick RFC-12 for `RFC-1/...` (AUDIT-018 C6).
        let parent_matches: Vec<&DocMeta> = self
            .docs
            .values()
            .filter(|d| {
                !self.parent_of.contains_key(&d.path)
                    && canonical_name(&d.path)
                        .map(|n| n.starts_with(parent_id))
                        .unwrap_or(false)
            })
            .collect();

        let parent = match parent_matches.len() {
            0 => return Err(ResolveError::NotFound(id.to_string())),
            1 => parent_matches[0],
            _ => parent_matches
                .iter()
                .find(|d| d.id == parent_id)
                .copied()
                .ok_or_else(|| ResolveError::Ambiguous {
                    id: parent_id.to_string(),
                    matches: parent_matches.iter().map(|d| d.path.clone()).collect(),
                })?,
        };

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

    /// Resolve a relation `target` (a doc id like `"RFC-006"` or a path) to the
    /// `DocMeta` it points at. Mirrors the private link-building `resolve_target`:
    /// look the target up as a document id first, then fall back to treating it
    /// as a path.
    pub fn resolve_relation_target(&self, target: &str) -> Option<&DocMeta> {
        let path = self
            .docs
            .values()
            .find(|d| d.id == target)
            .map(|d| d.path.clone())
            .unwrap_or_else(|| PathBuf::from(target));
        self.get(&path)
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
        // Drop any memoized body so a changed file is re-read (file-watch
        // invalidation, ADR-013). Covers both the removed and re-parsed cases.
        self.body_cache.lock().unwrap().remove(relative_path);

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
        self.body_cache.lock().unwrap().remove(relative_path);
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
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut matcher = Matcher::new(NucleoConfig::DEFAULT);
        let mut buf: Vec<char> = Vec::new();
        let mut results = Vec::new();

        for meta in self.docs.values() {
            // Track the single best-scoring field for this document. A `None`
            // score from `nucleo` means the field did not match at all; a
            // document whose fields all score `None` is dropped (the score
            // floor). Strict `>` keeps the earliest field on ties, so field
            // selection is deterministic regardless of `docs` iteration order.
            let mut best: Option<(u32, &'static str, String)> = None;

            let mut consider = |score: u32, field: &'static str, snippet: &dyn Fn() -> String| {
                if best.as_ref().is_none_or(|(b, _, _)| score > *b) {
                    best = Some((score, field, snippet()));
                }
            };

            if let Some(score) = pattern.score(Utf32Str::new(&meta.title, &mut buf), &mut matcher) {
                consider(score, "title", &|| meta.title.clone());
            }

            for tag in &meta.tags {
                if let Some(score) = pattern.score(Utf32Str::new(tag, &mut buf), &mut matcher) {
                    consider(score, "tag", &|| tag.clone());
                }
            }

            let path_str = meta.path.to_string_lossy();
            if let Some(score) = pattern.score(Utf32Str::new(&path_str, &mut buf), &mut matcher) {
                consider(score, "path", &|| path_str.to_string());
            }

            if let Some(body) = self.cached_or_read_body(&meta.path, fs) {
                let mut indices: Vec<u32> = Vec::new();
                if let Some(score) =
                    pattern.indices(Utf32Str::new(&body, &mut buf), &mut matcher, &mut indices)
                {
                    consider(score, "body", &|| body_snippet(&body, &indices, query));
                }
            }

            if let Some((score, match_field, snippet)) = best {
                results.push(SearchResult {
                    doc: meta,
                    match_field,
                    snippet,
                    score,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.doc.path.cmp(&b.doc.path))
        });
        results
    }

    /// Body text for `path`, served from the in-memory body cache when present
    /// and otherwise read from disk and memoized. `None` when the file cannot be
    /// read. See [`body_cache`](Store::body_cache) for the ADR-013 rationale.
    fn cached_or_read_body(&self, path: &Path, fs: &dyn FileSystem) -> Option<String> {
        if let Some(body) = self.body_cache.lock().unwrap().get(path) {
            return Some(body.clone());
        }
        let body = self.get_body_raw(path, fs).ok()?;
        self.body_cache
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), body.clone());
        Some(body)
    }
}

/// Char indices within `text` that `query` fuzzy-matches, using the same matcher
/// configuration as [`Store::search`]. Empty when `query` is empty or does not
/// match `text`, so a caller can highlight exactly the matched characters of a
/// rendered field. Indices are ascending and de-duplicated.
///
/// Lives beside `search` so both surfaces (CLI, TUI) share one matcher config;
/// the TUI must never own the scoring/matching algorithm (RFC-043 principle 3).
pub fn match_indices(query: &str, text: &str) -> Vec<u32> {
    if query.is_empty() {
        return Vec::new();
    }
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);
    let mut buf: Vec<char> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    if pattern
        .indices(Utf32Str::new(text, &mut buf), &mut matcher, &mut indices)
        .is_some()
    {
        indices.sort_unstable();
        indices.dedup();
        indices
    } else {
        Vec::new()
    }
}

/// Build a body snippet centred on the first fuzzy-matched character, preserving
/// the historical ±40-character window (`nucleo` gives scattered match indices
/// rather than a contiguous substring position).
fn body_snippet(body: &str, indices: &[u32], query: &str) -> String {
    let first_char = indices.first().copied().unwrap_or(0) as usize;
    let pos = body
        .char_indices()
        .nth(first_char)
        .map(|(b, _)| b)
        .unwrap_or(0);
    let start = body.floor_char_boundary(pos.saturating_sub(40));
    let end = body.ceil_char_boundary((pos + query.len() + 40).min(body.len()));
    body[start..end].to_string()
}

fn materialize_git_ref_cache(
    root: &Path,
    type_name: &str,
    ops: &dyn GitRefOps,
    fs: &dyn FileSystem,
) -> Result<()> {
    let ref_prefix = format!("refs/lazyspec/{}/", type_name);
    let refs = ops.list_refs(root, &ref_prefix)?;
    if refs.is_empty() {
        return Ok(());
    }

    let cache_dir = root.join(".lazyspec/cache").join(type_name);
    fs.create_dir_all(&cache_dir)?;

    for (refname, sha) in &refs {
        let id = refname.strip_prefix(&ref_prefix).unwrap_or(refname);
        let content = ops.read_ref_blob(root, sha, "doc.md")?;
        let cache_file = cache_dir.join(format!("{}.md", id));
        fs.write(&cache_file, &content)?;
    }

    let mut lock = CacheLock::load(root)?;
    for (refname, sha) in &refs {
        let id = refname.strip_prefix(&ref_prefix).unwrap_or(refname);
        let doc_key = format!("{}/{}", type_name, id);
        lock.set(&doc_key, sha);
    }
    lock.save(root)?;

    Ok(())
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

pub(crate) fn extract_id(path: &Path) -> String {
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
        // Materialized cache children live under a clean parent-id folder
        // (e.g. `STORY-100/01-STORY-12.md`). Their `NN-` order prefix is not part
        // of the doc id, so strip it before resolving the real child id.
        if let Some(rest) = strip_order_prefix(stem) {
            return extract_id_from_name(rest);
        }
    }

    extract_id_from_name(stem)
}

/// Strip a leading zero-padded numeric order prefix (`NN-`) from a stem, returning
/// the remainder. Returns `None` when no such prefix is present.
pub(crate) fn strip_order_prefix(stem: &str) -> Option<&str> {
    let (head, rest) = stem.split_once('-')?;
    if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
        Some(rest)
    } else {
        None
    }
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
    pub score: u32,
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
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();

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

    /// Build an in-memory store of RFC documents from `(filename, title, tags, body)`
    /// tuples, so the fuzzy `search` tests can control every match surface.
    fn search_store(entries: &[(&str, &str, &[&str], &str)]) -> (Store, InMemoryFileSystem) {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");
        let rfc_dir = root.join("docs/rfcs");
        fs.add_dir(rfc_dir.clone());

        for (filename, title, tags, body) in entries {
            let tags_yaml = if tags.is_empty() {
                "[]".to_string()
            } else {
                let items: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
                format!("[{}]", items.join(", "))
            };
            let content = format!(
                "---\ntitle: \"{}\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: {}\n---\n{}\n",
                title, tags_yaml, body
            );
            fs.add_file(rfc_dir.join(filename), &content);
        }

        let config = Config::default();
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();
        (store, fs)
    }

    #[test]
    fn search_matches_non_contiguous_subsequence_in_title() {
        // `enfz` is a subsequence of "engine fuzzy" (e-n from "engine", f-z from
        // "fuzzy") but never a contiguous substring; the old `.contains()` path
        // would have missed it. Filename has no matchable chars so only the title
        // matches.
        let (store, fs) = search_store(&[("RFC-001-doc.md", "engine fuzzy", &[], "some body")]);

        let results = store.search("enfz", &fs);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc.title, "engine fuzzy");
        assert_eq!(results[0].match_field, "title");
        assert!(results[0].score > 0);
    }

    #[test]
    fn search_ranks_by_score_descending() {
        // The stronger (contiguous, exact) match lives at the later-sorting path,
        // so if it ranks first the ordering must come from score, not the
        // path tie-break.
        let (store, fs) = search_store(&[
            ("RFC-001-x.md", "xfxuxzxzxyx", &[], "x"),
            ("RFC-002-y.md", "fuzzy", &[], "y"),
        ]);

        let results = store.search("fuzzy", &fs);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].doc.title, "fuzzy");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn search_tie_break_by_path_is_stable_across_runs() {
        // Both docs match only via their identical title "alpha", producing equal
        // scores; their filenames carry no 'l'/'p'/'h' so the path field never
        // matches and cannot break the tie by score.
        let (store, fs) = search_store(&[
            ("RFC-001-x.md", "alpha", &[], "body one"),
            ("RFC-002-y.md", "alpha", &[], "body two"),
        ]);

        let first = store.search("alpha", &fs);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].score, first[1].score);
        assert!(first[0].doc.path < first[1].doc.path);

        let baseline: Vec<PathBuf> = first.iter().map(|r| r.doc.path.clone()).collect();
        for _ in 0..5 {
            let again: Vec<PathBuf> = store
                .search("alpha", &fs)
                .iter()
                .map(|r| r.doc.path.clone())
                .collect();
            assert_eq!(again, baseline);
        }
    }

    #[test]
    fn search_score_floor_excludes_non_matches() {
        // "datb" is a subsequence of "database" but not of "frontend" (no 'a'
        // after its trailing 'd'); the non-matcher must be dropped entirely.
        let (store, fs) = search_store(&[
            ("RFC-001-a.md", "database", &[], "x"),
            ("RFC-002-b.md", "frontend", &[], "y"),
        ]);

        let results = store.search("datb", &fs);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc.title, "database");
    }

    #[test]
    fn search_body_only_match_sets_match_field_body() {
        // "fuzzy" appears only in the body: the title "hello" and the filename
        // (no 'u'/'z'/'y') do not match.
        let (store, fs) =
            search_store(&[("RFC-001-c.md", "hello", &[], "the fuzzy matcher lives here")]);

        let results = store.search("fuzzy", &fs);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_field, "body");
        assert!(results[0].snippet.contains("fuzzy"));
    }

    #[test]
    fn search_multi_field_match_returns_one_result_with_best_field_score() {
        // "core" matches the title only as a scattered subsequence
        // ("custom order rebuild engine") but matches the tag exactly. The exact
        // tag match outscores the title, so the doc yields exactly one result
        // naming the tag and carrying the tag field's (higher) score.
        let (store, fs) = search_store(&[(
            "RFC-001-m.md",
            "custom order rebuild engine",
            &["core"],
            "unrelated body text",
        )]);

        let results = store.search("core", &fs);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_field, "tag");

        // The reported score equals the best (tag) field's score in isolation:
        // a doc whose only matchable surface is the same "core" tag scores
        // identically, since nucleo scores each haystack independently.
        let (tag_only_store, tag_fs) = search_store(&[("RFC-002-n.md", "zzz", &["core"], "zzz")]);
        let tag_only = tag_only_store.search("core", &tag_fs);
        assert_eq!(tag_only.len(), 1);
        assert_eq!(tag_only[0].match_field, "tag");
        assert_eq!(results[0].score, tag_only[0].score);
    }

    #[test]
    fn match_indices_returns_the_matched_subsequence_positions() {
        // `tff` matches "tui fuzzy filter" as t(0) f(4) f(10): non-contiguous.
        let idx = match_indices("tff", "tui fuzzy filter");
        assert_eq!(idx, vec![0, 4, 10]);
        assert_eq!(
            "tui fuzzy filter".chars().next(),
            Some('t'),
            "index 0 is the leading 't'"
        );
        assert_eq!("tui fuzzy filter".chars().nth(4), Some('f'));
        assert_eq!("tui fuzzy filter".chars().nth(10), Some('f'));
    }

    #[test]
    fn match_indices_empty_when_no_match_or_empty_query() {
        assert!(match_indices("zzz", "hello world").is_empty());
        assert!(match_indices("", "hello world").is_empty());
    }

    #[test]
    fn search_reads_body_from_cache_until_reload_invalidates_it() {
        // Body match works; the body is then memoized. Editing the file on disk
        // WITHOUT reloading keeps the stale (cached) body, so the new token does
        // not match. `reload_file` drops the entry, and the fresh body is re-read.
        let (mut store, fs) = search_store(&[("RFC-001-c.md", "hello", &[], "the fuzzy matcher")]);
        let path = PathBuf::from("docs/rfcs/RFC-001-c.md");

        assert_eq!(store.search("fuzzy", &fs).len(), 1, "cold body match");

        // Rewrite the body on disk: drop "fuzzy", add "gadget".
        fs.add_file(
            PathBuf::from("/fake/root").join(&path),
            "---\ntitle: \"hello\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\nthe gadget matcher\n",
        );

        assert!(
            store.search("gadget", &fs).is_empty(),
            "cached body is stale until reload"
        );

        store
            .reload_file(Path::new("/fake/root"), &path, &fs)
            .unwrap();

        assert_eq!(
            store.search("gadget", &fs).len(),
            1,
            "reload invalidates the cache; the new body is re-read and matches"
        );
        assert!(
            store.search("fuzzy", &fs).is_empty(),
            "the old body token no longer matches after reload"
        );
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
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();

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
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();

        let rel = PathBuf::from(".lazyspec/cache/issue/ISSUE-007-fix-auth.md");
        let body = store.get_body_raw(&rel, &fs).unwrap();
        assert_eq!(body.trim(), "Auth tokens expire too quickly.");
    }

    fn git_ref_config() -> Config {
        use crate::engine::config::{NumberingStrategy, StoreBackend, TypeDef};

        let ref_type = TypeDef {
            name: "note".to_string(),
            plural: "notes".to_string(),
            dir: "docs/notes".to_string(),
            prefix: "NOTE".to_string(),
            icon: Some("📝".to_string()),
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store: StoreBackend::GitRef,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        };

        let mut config = Config::default();
        config.documents.types.push(ref_type);
        config
    }

    #[test]
    fn test_load_includes_git_ref_cache() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");

        let cache_dir = root.join(".lazyspec/cache/note");
        fs.add_dir(cache_dir.clone());

        let note_path = cache_dir.join("NOTE-001-hello.md");
        fs.add_file(
            &note_path,
            concat!(
                "---\n",
                "title: \"Hello note\"\n",
                "type: note\n",
                "status: draft\n",
                "author: \"tester\"\n",
                "date: 2026-04-01\n",
                "tags: []\n",
                "---\n",
                "A git-ref backed note.\n",
            ),
        );

        let config = git_ref_config();
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();

        let rel = PathBuf::from(".lazyspec/cache/note/NOTE-001-hello.md");
        let doc = store.get(&rel);
        assert!(doc.is_some(), "git-ref doc should be loaded from cache dir");
        assert_eq!(doc.unwrap().title, "Hello note");
        assert_eq!(doc.unwrap().id, "NOTE-001");
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
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();

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

    // Seed a subdirectory parent `docs/rfcs/<folder>/index.md` with one child
    // markdown file, so parent/child shorthand (`RFC-1/STORY-5`) can resolve.
    fn add_parent_with_child(fs: &InMemoryFileSystem, root: &Path, folder: &str, child: &str) {
        let dir = root.join("docs/rfcs").join(folder);
        fs.add_dir(dir.clone());
        fs.add_file(
            dir.join("index.md"),
            &format!(
                "---\ntitle: \"{folder}\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\nparent\n"
            ),
        );
        fs.add_file(
            dir.join(format!("{child}.md")),
            &format!(
                "---\ntitle: \"{child}\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\nchild\n"
            ),
        );
    }

    // AUDIT-018 C6 / STORY-210 AC3: with RFC-1 and RFC-12 (plus more RFC-1*
    // decoys) all present, `RFC-1/STORY-5` must resolve to RFC-1's child --
    // the exact parent id wins over first-prefix-match-in-HashMap-order.
    #[test]
    fn test_resolve_shorthand_prefers_exact_parent_id() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");
        fs.add_dir(root.join("docs/rfcs"));

        add_parent_with_child(&fs, &root, "RFC-1-alpha", "STORY-5-real");
        add_parent_with_child(&fs, &root, "RFC-10-b", "STORY-6-a");
        add_parent_with_child(&fs, &root, "RFC-11-c", "STORY-7-b");
        add_parent_with_child(&fs, &root, "RFC-12-d", "STORY-8-c");
        add_parent_with_child(&fs, &root, "RFC-13-e", "STORY-9-d");

        let store = Store::load_with_fs(&root, &Config::default(), &fs, None).unwrap();

        let doc = store
            .resolve_shorthand("RFC-1/STORY-5")
            .expect("exact parent id RFC-1 must win over RFC-1* prefix decoys");
        assert_eq!(
            doc.path,
            PathBuf::from("docs/rfcs/RFC-1-alpha/STORY-5-real.md")
        );
    }

    // AUDIT-018 C6 / STORY-210 AC3: with no exact RFC-1, a prefix matching
    // several parents (RFC-10, RFC-12) is a genuine ambiguity and must error
    // listing the candidates, mirroring resolve_unqualified.
    #[test]
    fn test_resolve_shorthand_ambiguous_parent_prefix_errors() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");
        fs.add_dir(root.join("docs/rfcs"));

        add_parent_with_child(&fs, &root, "RFC-10-b", "STORY-6-a");
        add_parent_with_child(&fs, &root, "RFC-12-d", "STORY-8-c");

        let store = Store::load_with_fs(&root, &Config::default(), &fs, None).unwrap();

        let err = store
            .resolve_shorthand("RFC-1/STORY-6")
            .expect_err("prefix RFC-1 matching RFC-10 and RFC-12 must be ambiguous");
        match err {
            ResolveError::Ambiguous { matches, .. } => {
                assert_eq!(matches.len(), 2, "both candidates listed: {:?}", matches);
            }
            other => panic!("expected Ambiguous, got: {other:?}"),
        }
    }

    #[test]
    fn test_cold_cache_fallback_materializes_from_git_refs() {
        use crate::engine::git_ref::test_support::MockGitRefClient;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let note_content = concat!(
            "---\n",
            "title: \"Cold note\"\n",
            "type: note\n",
            "status: draft\n",
            "author: \"tester\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "A note from a cold cache.\n",
        );

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![(
                "refs/lazyspec/note/NOTE-001-cold".to_string(),
                "abc123".to_string(),
            )]))
            .with_read_blob_result(Ok(note_content.to_string()));

        let config = git_ref_config();
        let store = Store::load_with_fs(root, &config, &RealFileSystem, Some(&mock)).unwrap();

        let rel = PathBuf::from(".lazyspec/cache/note/NOTE-001-cold.md");
        let doc = store.get(&rel);
        assert!(doc.is_some(), "cold cache fallback should materialize doc");
        assert_eq!(doc.unwrap().title, "Cold note");
        assert_eq!(doc.unwrap().id, "NOTE-001");

        assert!(
            root.join(".lazyspec/cache/note/NOTE-001-cold.md").exists(),
            "cache file should be written to filesystem"
        );

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(
            lock.get("note/NOTE-001-cold"),
            Some("abc123"),
            "cache.lock should contain materialized entry"
        );

        let calls = mock.calls.borrow();
        assert!(calls.iter().any(|c| c.starts_with("list_refs:")));
        assert!(calls.iter().any(|c| c.starts_with("read_ref_blob:")));
    }

    #[test]
    fn test_cold_cache_fallback_skipped_when_no_git_ref_ops() {
        let fs = InMemoryFileSystem::new();
        let root = PathBuf::from("/fake/root");

        let config = git_ref_config();
        let store = Store::load_with_fs(&root, &config, &fs, None).unwrap();

        assert_eq!(store.docs.len(), 0);
    }

    #[test]
    fn test_cold_cache_fallback_with_empty_cache_dir() {
        use crate::engine::git_ref::test_support::MockGitRefClient;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let cache_dir = root.join(".lazyspec/cache/note");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let note_content = concat!(
            "---\n",
            "title: \"Empty dir note\"\n",
            "type: note\n",
            "status: draft\n",
            "author: \"tester\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "Materialized from empty cache dir.\n",
        );

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![(
                "refs/lazyspec/note/NOTE-002-empty".to_string(),
                "def456".to_string(),
            )]))
            .with_read_blob_result(Ok(note_content.to_string()));

        let config = git_ref_config();
        let store = Store::load_with_fs(root, &config, &RealFileSystem, Some(&mock)).unwrap();

        let rel = PathBuf::from(".lazyspec/cache/note/NOTE-002-empty.md");
        let doc = store.get(&rel);
        assert!(
            doc.is_some(),
            "should materialize from refs when cache dir is empty"
        );
        assert_eq!(doc.unwrap().title, "Empty dir note");

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(
            lock.get("note/NOTE-002-empty"),
            Some("def456"),
            "cache.lock should contain materialized entry"
        );
    }

    #[test]
    fn extract_id_nested_cache_child_strips_order_prefix() {
        // A materialized cache child lives under a clean parent-id folder; its
        // `NN-` order prefix is not part of the doc id.
        let path = PathBuf::from(".lazyspec/cache/story/STORY-100/01-STORY-12.md");
        assert_eq!(extract_id(&path), "STORY-12");
    }

    #[test]
    fn extract_id_nested_cache_parent_uses_folder_id() {
        let path = PathBuf::from(".lazyspec/cache/story/STORY-100/index.md");
        assert_eq!(extract_id(&path), "STORY-100");
    }

    #[test]
    fn extract_id_filesystem_subdir_child_keeps_full_stem() {
        // Filesystem-authored subdir children sit under a slug folder (title
        // suffix) and keep their full `NN-name` stem as the id.
        let path = PathBuf::from("docs/stories/STORY-159-shape/01-first.md");
        assert_eq!(extract_id(&path), "01-first");
    }

    #[test]
    fn extract_id_flat_doc_unaffected_by_order_strip() {
        let path = PathBuf::from(".lazyspec/cache/story/STORY-12.md");
        assert_eq!(extract_id(&path), "STORY-12");
    }
}
