use crate::engine::config::{AttrDef, TypeDef};
use crate::engine::document::{DocMeta, DocType, Part, Status};
use crate::engine::fs::FileSystem;
use anyhow::Result;
use chrono::Utc;
use globset::{Glob, GlobMatcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{extract_id, title_from_folder_name, ParseError};

/// Compile a document's `governs` entries into matchers, keeping each entry's
/// source text so a match can report which glob matched (RFC-068).
///
/// One bad entry fails the whole set: a document with an uncompilable pin
/// governs nothing until it is fixed, rather than governing a silent subset.
/// The failure rides the loader's existing per-document [`ParseError`] channel.
pub fn compile_governs(meta: &DocMeta) -> Result<Vec<(String, GlobMatcher)>, ParseError> {
    meta.governs
        .iter()
        .map(|entry| {
            Glob::new(entry)
                .map(|g| (entry.clone(), g.compile_matcher()))
                .map_err(|e| ParseError {
                    path: meta.path.clone(),
                    error: format!("invalid governs glob '{entry}': {e}"),
                })
        })
        .collect()
}

/// True iff `index_path` (relative to the store root) is the `index.md` of a
/// bundle folder: a direct subdirectory of a type directory. An `index.md`
/// sitting in the type directory itself is a plain document, exactly as
/// [`load_type_directory`] treats it.
pub fn is_bundle_index(type_dirs: &[PathBuf], index_path: &Path) -> bool {
    if index_path.file_name().and_then(|f| f.to_str()) != Some("index.md") {
        return false;
    }
    let Some(type_dir) = index_path.parent().and_then(Path::parent) else {
        return false;
    };
    type_dirs.iter().any(|d| d == type_dir)
}

#[allow(clippy::too_many_arguments)]
pub fn load_type_directory(
    root: &Path,
    full_path: &Path,
    type_def: &TypeDef,
    declared_parts: &[String],
    docs: &mut HashMap<PathBuf, DocMeta>,
    children: &mut HashMap<PathBuf, Vec<PathBuf>>,
    parent_of: &mut HashMap<PathBuf, PathBuf>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<()> {
    for path in fs.read_dir(full_path)? {
        if fs.is_dir(&path) {
            load_subdirectory(
                root,
                &path,
                type_def,
                declared_parts,
                docs,
                children,
                parent_of,
                parse_errors,
                fs,
            )?;
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        parse_document_entry(root, &path, &type_def.attributes, docs, parse_errors, fs)?;
    }
    Ok(())
}

pub fn parse_document_entry(
    root: &Path,
    path: &Path,
    schema: &[AttrDef],
    docs: &mut HashMap<PathBuf, DocMeta>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<Option<PathBuf>> {
    let content = fs.read_to_string(path)?;
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    match DocMeta::parse_with_schema(&content, schema) {
        Ok(mut meta) => {
            meta.path = relative.clone();
            meta.id = extract_id(&meta.path);
            docs.insert(meta.path.clone(), meta);
            Ok(Some(relative))
        }
        Err(e) => {
            parse_errors.push(ParseError {
                path: relative,
                error: e.to_string(),
            });
            Ok(None)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn load_child_markdown_files(
    root: &Path,
    dir: &Path,
    skip_index: bool,
    schema: &[AttrDef],
    docs: &mut HashMap<PathBuf, DocMeta>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<Vec<PathBuf>> {
    let mut child_paths = Vec::new();
    for child_path in fs.read_dir(dir)? {
        if fs.is_dir(&child_path) {
            continue;
        }
        if skip_index && child_path.file_name().and_then(|f| f.to_str()) == Some("index.md") {
            continue;
        }
        if child_path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(rel) = parse_document_entry(root, &child_path, schema, docs, parse_errors, fs)?
        {
            child_paths.push(rel);
        }
    }
    // Deterministic loader order: the sub-issue reconcile keys remote ordering
    // off this, and `fs.read_dir` is unordered on most platforms.
    child_paths.sort();
    Ok(child_paths)
}

/// What a document folder holds besides its `index.md` (RFC-074 AC3): child
/// documents (a `.md` carrying frontmatter, unchanged from before this
/// story), parts (a `.md` with none), and sidecars (anything else).
struct FolderContents {
    child_paths: Vec<PathBuf>,
    parts: Vec<Part>,
    sidecars: Vec<PathBuf>,
}

/// True iff `content` opens with a YAML frontmatter delimiter. Cheap
/// discriminator between a child document (has frontmatter, parsed and
/// validated the usual way) and a part (has none, and is never a parse
/// error) -- checked directly rather than by matching `split_frontmatter`'s
/// error text, so a file that opens with `---` but is otherwise malformed
/// still takes the child-document path and surfaces its real parse error.
fn looks_like_frontmatter(content: &str) -> bool {
    content.trim_start().starts_with("---")
}

/// Scan a document folder's entries (everything beside `index.md`),
/// classifying each into a child document, a part, or a sidecar per the
/// RFC-074 AC3 table. Parts are ordered by `declared_parts` (the template's
/// declared order) first, then any extra parts alphabetically (AC5).
#[allow(clippy::too_many_arguments)]
fn scan_document_folder(
    root: &Path,
    dir: &Path,
    schema: &[AttrDef],
    declared_parts: &[String],
    docs: &mut HashMap<PathBuf, DocMeta>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<FolderContents> {
    let mut child_paths = Vec::new();
    let mut sidecars = Vec::new();
    // (stem, relative path) for every frontmatter-less `.md`, before ordering.
    let mut found_parts: Vec<(String, PathBuf)> = Vec::new();

    for entry_path in fs.read_dir(dir)? {
        if fs.is_dir(&entry_path) {
            continue;
        }
        let file_name = entry_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or_default();
        if file_name == "index.md" {
            continue;
        }

        let relative = entry_path
            .strip_prefix(root)
            .unwrap_or(&entry_path)
            .to_path_buf();

        if entry_path.extension().and_then(|e| e.to_str()) != Some("md") {
            sidecars.push(relative);
            continue;
        }

        let content = fs.read_to_string(&entry_path)?;
        if looks_like_frontmatter(&content) {
            if let Some(rel) =
                parse_document_entry(root, &entry_path, schema, docs, parse_errors, fs)?
            {
                child_paths.push(rel);
            }
            continue;
        }

        let stem = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file_name)
            .to_string();
        found_parts.push((stem, relative));
    }

    // Deterministic loader order, same reasoning as `load_child_markdown_files`.
    child_paths.sort();
    sidecars.sort();

    let mut parts = Vec::new();
    for name in declared_parts {
        if let Some(pos) = found_parts.iter().position(|(n, _)| n == name) {
            let (name, path) = found_parts.remove(pos);
            parts.push(Part { name, path });
        }
    }
    found_parts.sort_by(|a, b| a.0.cmp(&b.0));
    parts.extend(
        found_parts
            .into_iter()
            .map(|(name, path)| Part { name, path }),
    );

    Ok(FolderContents {
        child_paths,
        parts,
        sidecars,
    })
}

#[allow(clippy::too_many_arguments)]
fn load_subdirectory(
    root: &Path,
    path: &Path,
    type_def: &TypeDef,
    declared_parts: &[String],
    docs: &mut HashMap<PathBuf, DocMeta>,
    children: &mut HashMap<PathBuf, Vec<PathBuf>>,
    parent_of: &mut HashMap<PathBuf, PathBuf>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<()> {
    let index_path = path.join("index.md");

    if fs.exists(&index_path) {
        let parent_relative = index_path
            .strip_prefix(root)
            .unwrap_or(&index_path)
            .to_path_buf();
        parse_document_entry(
            root,
            &index_path,
            &type_def.attributes,
            docs,
            parse_errors,
            fs,
        )?;
        let FolderContents {
            child_paths,
            parts,
            sidecars,
        } = scan_document_folder(
            root,
            path,
            &type_def.attributes,
            declared_parts,
            docs,
            parse_errors,
            fs,
        )?;
        for cp in &child_paths {
            parent_of.insert(cp.clone(), parent_relative.clone());
        }
        if !child_paths.is_empty() {
            children.insert(parent_relative.clone(), child_paths);
        }
        if let Some(index_meta) = docs.get_mut(&parent_relative) {
            index_meta.parts = parts;
            index_meta.sidecars = sidecars;
        }
        return Ok(());
    }

    let child_paths = load_child_markdown_files(
        root,
        path,
        false,
        &type_def.attributes,
        docs,
        parse_errors,
        fs,
    )?;
    if child_paths.is_empty() {
        return Ok(());
    }

    let folder_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let folder_relative = path.strip_prefix(root).unwrap_or(path);
    let parent_relative = folder_relative.join(".virtual");

    let all_accepted = child_paths.iter().all(|cp| {
        docs.get(cp)
            .map(|d| d.status == Status::new("accepted"))
            .unwrap_or(false)
    });

    let virtual_meta = DocMeta {
        path: parent_relative.clone(),
        title: title_from_folder_name(folder_name),
        doc_type: DocType::new(&type_def.name),
        status: if all_accepted {
            Status::new("accepted")
        } else {
            Status::new("draft")
        },
        author: "".to_string(),
        date: Utc::now().date_naive(),
        tags: vec![],
        provenance: vec![],
        governs: vec![],
        reviewed: None,
        related: vec![],
        validate_ignore: false,
        virtual_doc: true,
        assignee: None,
        attributes: Default::default(),
        id: extract_id(&parent_relative),
        parts: Vec::new(),
        sidecars: Vec::new(),
    };
    docs.insert(parent_relative.clone(), virtual_meta);

    for cp in &child_paths {
        parent_of.insert(cp.clone(), parent_relative.clone());
    }
    children.insert(parent_relative, child_paths);

    Ok(())
}
