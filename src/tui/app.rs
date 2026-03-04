use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType};
use crate::engine::store::{Filter, Store};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

fn update_tags(root: &Path, relative: &Path, tags: &[String]) -> Result<()> {
    let full_path = root.join(relative);
    let content = std::fs::read_to_string(&full_path)?;

    let trimmed = content.trim_start();
    let after_first = &trimmed[3..];
    let end = after_first.find("\n---")
        .ok_or_else(|| anyhow!("unterminated frontmatter"))?;
    let yaml_str = &after_first[..end];
    let body = &after_first[end + 4..];

    let mut doc: serde_yaml::Value = serde_yaml::from_str(yaml_str)?;
    let tag_values: Vec<serde_yaml::Value> = tags.iter()
        .map(|t| serde_yaml::Value::String(t.clone()))
        .collect();
    doc["tags"] = serde_yaml::Value::Sequence(tag_values);

    let new_yaml = serde_yaml::to_string(&doc)?;
    let new_content = format!("---\n{}---\n{}", new_yaml, body);
    std::fs::write(&full_path, new_content)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormField {
    Title,
    Author,
    Tags,
    Related,
}

impl FormField {
    fn next(self) -> Self {
        match self {
            FormField::Title => FormField::Author,
            FormField::Author => FormField::Tags,
            FormField::Tags => FormField::Related,
            FormField::Related => FormField::Title,
        }
    }

    fn prev(self) -> Self {
        match self {
            FormField::Title => FormField::Related,
            FormField::Author => FormField::Title,
            FormField::Tags => FormField::Author,
            FormField::Related => FormField::Tags,
        }
    }
}

pub struct CreateForm {
    pub active: bool,
    pub doc_type: DocType,
    pub focused_field: FormField,
    pub title: String,
    pub author: String,
    pub tags: String,
    pub related: String,
    pub error: Option<String>,
}

impl CreateForm {
    pub fn new() -> Self {
        CreateForm {
            active: false,
            doc_type: DocType::Rfc,
            focused_field: FormField::Title,
            title: String::new(),
            author: String::new(),
            tags: String::new(),
            related: String::new(),
            error: None,
        }
    }

    fn reset(&mut self) {
        self.active = false;
        self.focused_field = FormField::Title;
        self.title.clear();
        self.author.clear();
        self.tags.clear();
        self.related.clear();
        self.error = None;
    }

    fn focused_value_mut(&mut self) -> &mut String {
        match self.focused_field {
            FormField::Title => &mut self.title,
            FormField::Author => &mut self.author,
            FormField::Tags => &mut self.tags,
            FormField::Related => &mut self.related,
        }
    }
}

pub struct DeleteConfirm {
    pub active: bool,
    pub doc_path: PathBuf,
    pub doc_title: String,
    pub references: Vec<(String, PathBuf)>,
}

impl DeleteConfirm {
    pub fn new() -> Self {
        DeleteConfirm {
            active: false,
            doc_path: PathBuf::new(),
            doc_title: String::new(),
            references: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewTab {
    Preview,
    Relations,
}

pub struct App {
    pub store: Store,
    pub selected_type: usize,
    pub selected_doc: usize,
    pub doc_types: Vec<DocType>,
    pub should_quit: bool,
    pub fullscreen_doc: bool,
    pub scroll_offset: u16,
    pub search_mode: bool,
    pub search_query: String,
    pub search_results: Vec<std::path::PathBuf>,
    pub search_selected: usize,
    pub show_help: bool,
    pub preview_tab: PreviewTab,
    pub create_form: CreateForm,
    pub delete_confirm: DeleteConfirm,
}

impl App {
    pub fn new(store: Store) -> Self {
        App {
            store,
            selected_type: 0,
            selected_doc: 0,
            doc_types: vec![DocType::Rfc, DocType::Adr, DocType::Story, DocType::Iteration],
            should_quit: false,
            fullscreen_doc: false,
            scroll_offset: 0,
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            show_help: false,
            preview_tab: PreviewTab::Preview,
            create_form: CreateForm::new(),
            delete_confirm: DeleteConfirm::new(),
        }
    }

    pub fn current_type(&self) -> &DocType {
        &self.doc_types[self.selected_type]
    }

    pub fn docs_for_current_type(&self) -> Vec<&DocMeta> {
        let mut docs = self.store.list(&Filter {
            doc_type: Some(self.current_type().clone()),
            ..Default::default()
        });
        docs.sort_by(|a, b| a.path.cmp(&b.path));
        docs
    }

    pub fn selected_doc_meta(&self) -> Option<&DocMeta> {
        let docs = self.docs_for_current_type();
        docs.get(self.selected_doc).copied()
    }

    pub fn doc_count(&self, doc_type: &DocType) -> usize {
        self.store
            .list(&Filter {
                doc_type: Some(doc_type.clone()),
                ..Default::default()
            })
            .len()
    }

    pub fn move_down(&mut self) {
        let count = self.docs_for_current_type().len();
        if count > 0 && self.selected_doc < count - 1 {
            self.selected_doc += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_doc > 0 {
            self.selected_doc -= 1;
        }
    }

    pub fn move_to_top(&mut self) {
        self.selected_doc = 0;
    }

    pub fn enter_fullscreen(&mut self) {
        if self.selected_doc_meta().is_some() {
            self.fullscreen_doc = true;
            self.scroll_offset = 0;
        }
    }

    pub fn exit_fullscreen(&mut self) {
        self.fullscreen_doc = false;
        self.scroll_offset = 0;
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn move_to_bottom(&mut self) {
        let count = self.docs_for_current_type().len();
        if count > 0 {
            self.selected_doc = count - 1;
        }
    }

    pub fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selected = 0;
    }

    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selected = 0;
    }

    pub fn update_search(&mut self) {
        if self.search_query.is_empty() {
            self.search_results.clear();
            self.search_selected = 0;
            return;
        }

        let query = self.search_query.to_lowercase();
        let mut results: Vec<_> = self
            .store
            .all_docs()
            .into_iter()
            .filter(|doc| {
                let title_match = doc.title.to_lowercase().contains(&query);
                let tag_match = doc.tags.iter().any(|t| t.to_lowercase().contains(&query));
                let path_match = doc.path.to_string_lossy().to_lowercase().contains(&query);
                title_match || tag_match || path_match
            })
            .map(|doc| doc.path.clone())
            .collect();
        results.sort();
        self.search_results = results;
        self.search_selected = 0;
    }

    pub fn select_search_result(&mut self) {
        let path = match self.search_results.get(self.search_selected) {
            Some(p) => p.clone(),
            None => return,
        };

        if let Some(doc) = self.store.get(&path) {
            let doc_type = doc.doc_type.clone();
            if let Some(idx) = self.doc_types.iter().position(|t| *t == doc_type) {
                self.selected_type = idx;
                let docs = self.docs_for_current_type();
                if let Some(di) = docs.iter().position(|d| d.path == path) {
                    self.selected_doc = di;
                }
            }
        }
        self.exit_search();
    }

    pub fn toggle_preview_tab(&mut self) {
        self.preview_tab = match self.preview_tab {
            PreviewTab::Preview => PreviewTab::Relations,
            PreviewTab::Relations => PreviewTab::Preview,
        };
    }


    pub fn move_type_next(&mut self) {
        if self.selected_type < self.doc_types.len() - 1 {
            self.selected_type += 1;
            self.selected_doc = 0;
        }
    }

    pub fn move_type_prev(&mut self) {
        if self.selected_type > 0 {
            self.selected_type -= 1;
            self.selected_doc = 0;
        }
    }

    pub fn open_create_form(&mut self) {
        self.create_form.reset();
        self.create_form.active = true;
        self.create_form.doc_type = self.current_type().clone();
    }

    pub fn close_create_form(&mut self) {
        self.create_form.reset();
    }

    pub fn form_type_char(&mut self, c: char) {
        self.create_form.focused_value_mut().push(c);
        self.create_form.error = None;
    }

    pub fn form_backspace(&mut self) {
        self.create_form.focused_value_mut().pop();
        self.create_form.error = None;
    }

    pub fn form_next_field(&mut self) {
        self.create_form.focused_field = self.create_form.focused_field.next();
    }

    pub fn form_prev_field(&mut self) {
        self.create_form.focused_field = self.create_form.focused_field.prev();
    }

    pub fn submit_create_form(&mut self, root: &Path, config: &Config) -> Result<()> {
        let title = self.create_form.title.trim().to_string();
        if title.is_empty() {
            self.create_form.error = Some("Title is required".to_string());
            return Err(anyhow!("Title is required"));
        }

        let doc_type_str = match self.create_form.doc_type {
            DocType::Rfc => "rfc",
            DocType::Adr => "adr",
            DocType::Story => "story",
            DocType::Iteration => "iteration",
        };

        let author = if self.create_form.author.trim().is_empty() {
            "unknown"
        } else {
            self.create_form.author.trim()
        };

        // Validate relations before creating anything
        let relations = match self.parse_relations() {
            Ok(r) => r,
            Err(e) => {
                self.create_form.error = Some(e.to_string());
                return Err(e);
            }
        };

        let path = crate::cli::create::run(root, config, doc_type_str, &title, author)?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let relative_str = relative.to_string_lossy().to_string();

        // Apply tags
        let tags_str = self.create_form.tags.trim().to_string();
        if !tags_str.is_empty() {
            let tags: Vec<String> = tags_str.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            update_tags(root, &relative, &tags)?;
        }

        // Apply relations
        for (rel_type, target_path) in &relations {
            crate::cli::link::link(root, &relative_str, rel_type, &target_path.to_string_lossy())?;
        }

        // Reload the store to pick up the new file
        let _ = self.store.reload_file(root, &relative);

        // Navigate to the new document
        let doc_type = self.create_form.doc_type.clone();
        if let Some(type_idx) = self.doc_types.iter().position(|t| *t == doc_type) {
            self.selected_type = type_idx;
            let docs = self.docs_for_current_type();
            if let Some(doc_idx) = docs.iter().position(|d| d.path == relative) {
                self.selected_doc = doc_idx;
            }
        }

        self.close_create_form();
        Ok(())
    }

    pub fn open_delete_confirm(&mut self) {
        let doc = match self.selected_doc_meta() {
            Some(d) => d,
            None => return,
        };
        let path = doc.path.clone();
        let title = doc.title.clone();
        let refs = self
            .store
            .referenced_by(&path)
            .into_iter()
            .map(|(rel, p)| (rel.to_string(), p.clone()))
            .collect();
        self.delete_confirm.active = true;
        self.delete_confirm.doc_path = path;
        self.delete_confirm.doc_title = title;
        self.delete_confirm.references = refs;
    }

    pub fn close_delete_confirm(&mut self) {
        self.delete_confirm.active = false;
        self.delete_confirm.doc_path = PathBuf::new();
        self.delete_confirm.doc_title.clear();
        self.delete_confirm.references.clear();
    }

    pub fn confirm_delete(&mut self, root: &Path) -> Result<()> {
        let doc_path = self.delete_confirm.doc_path.clone();
        let doc_path_str = doc_path.to_string_lossy().to_string();
        crate::cli::delete::run(root, &doc_path_str)?;
        self.store.remove_file(&doc_path);

        let count = self.docs_for_current_type().len();
        if count == 0 {
            self.selected_doc = 0;
        } else if self.selected_doc >= count {
            self.selected_doc = count - 1;
        }

        self.close_delete_confirm();
        Ok(())
    }

    fn parse_relations(&self) -> Result<Vec<(String, std::path::PathBuf)>> {
        let related_str = self.create_form.related.trim().to_string();
        if related_str.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in related_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let (rel_type, shorthand) = if let Some((prefix, id)) = entry.split_once(':') {
                let rel = match prefix.trim() {
                    "implements" => "implements",
                    "supersedes" => "supersedes",
                    "blocks" => "blocks",
                    "related-to" => "related-to",
                    _ => {
                        return Err(anyhow!("Unknown relation type: {}", prefix.trim()));
                    }
                };
                (rel.to_string(), id.trim().to_string())
            } else {
                ("related-to".to_string(), entry.to_string())
            };

            let doc = self.store.resolve_shorthand(&shorthand)
                .ok_or_else(|| anyhow!("Cannot resolve: {}", shorthand))?;
            results.push((rel_type, doc.path.clone()));
        }
        Ok(results)
    }
}
