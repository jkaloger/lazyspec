use crate::engine::config::TypeDef;
use crate::engine::document::{DocMeta, DocType, Status};
use crate::engine::fs::FileSystem;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{extract_id, title_from_folder_name, ParseError};

pub fn load_type_directory(
    root: &Path,
    full_path: &Path,
    type_def: &TypeDef,
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
        parse_document_entry(root, &path, docs, parse_errors, fs)?;
    }
    Ok(())
}

pub fn parse_document_entry(
    root: &Path,
    path: &Path,
    docs: &mut HashMap<PathBuf, DocMeta>,
    parse_errors: &mut Vec<ParseError>,
    fs: &dyn FileSystem,
) -> Result<Option<PathBuf>> {
    let content = fs.read_to_string(path)?;
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    match DocMeta::parse(&content) {
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

fn load_child_markdown_files(
    root: &Path,
    dir: &Path,
    skip_index: bool,
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
        if let Some(rel) = parse_document_entry(root, &child_path, docs, parse_errors, fs)? {
            child_paths.push(rel);
        }
    }
    Ok(child_paths)
}

fn load_subdirectory(
    root: &Path,
    path: &Path,
    type_def: &TypeDef,
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
        parse_document_entry(root, &index_path, docs, parse_errors, fs)?;
        let child_paths = load_child_markdown_files(root, path, true, docs, parse_errors, fs)?;
        for cp in &child_paths {
            parent_of.insert(cp.clone(), parent_relative.clone());
        }
        if !child_paths.is_empty() {
            children.insert(parent_relative, child_paths);
        }
        return Ok(());
    }

    let child_paths = load_child_markdown_files(root, path, false, docs, parse_errors, fs)?;
    if child_paths.is_empty() {
        return Ok(());
    }

    let folder_name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let folder_relative = path.strip_prefix(root).unwrap_or(path);
    let parent_relative = folder_relative.join(".virtual");

    let all_accepted = child_paths.iter().all(|cp| {
        docs.get(cp)
            .map(|d| d.status == Status::Accepted)
            .unwrap_or(false)
    });

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
        id: extract_id(&parent_relative),
    };
    docs.insert(parent_relative.clone(), virtual_meta);

    for cp in &child_paths {
        parent_of.insert(cp.clone(), parent_relative.clone());
    }
    children.insert(parent_relative, child_paths);

    Ok(())
}
