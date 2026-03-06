use crate::engine::config::Config;
use crate::engine::document::{DocMeta, DocType};
use crate::engine::store::{Filter, Store};
use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::{Path, PathBuf};

fn update_tags(root: &Path, relative: &Path, tags: &[String]) -> Result<()> {
    let full_path = root.join(relative);
    let content = std::fs::read_to_string(&full_path)?;

    let (yaml_str, body) = crate::engine::document::split_frontmatter(&content)?;

    let mut doc: serde_yaml::Value = serde_yaml::from_str(&yaml_str)?;
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
pub enum ViewMode {
    Types,
    Filters,
    Metrics,
    Graph,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::Types => ViewMode::Filters,
            ViewMode::Filters => ViewMode::Metrics,
            ViewMode::Metrics => ViewMode::Graph,
            ViewMode::Graph => ViewMode::Types,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            ViewMode::Types => "Types",
            ViewMode::Filters => "Filters",
            ViewMode::Metrics => "Metrics",
            ViewMode::Graph => "Graph",
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
    pub selected_relation: usize,
    pub create_form: CreateForm,
    pub delete_confirm: DeleteConfirm,
    pub view_mode: ViewMode,
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
            selected_relation: 0,
            create_form: CreateForm::new(),
            delete_confirm: DeleteConfirm::new(),
            view_mode: ViewMode::Types,
        }
    }

    pub fn cycle_mode(&mut self) {
        self.view_mode = self.view_mode.next();
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
        self.selected_relation = 0;
    }

    pub fn relation_count(&self) -> usize {
        match self.selected_doc_meta() {
            Some(doc) => self.store.related_to(&doc.path).len(),
            None => 0,
        }
    }

    pub fn move_relation_down(&mut self) {
        let count = self.relation_count();
        if count > 0 && self.selected_relation < count - 1 {
            self.selected_relation += 1;
        }
    }

    pub fn move_relation_up(&mut self) {
        if self.selected_relation > 0 {
            self.selected_relation -= 1;
        }
    }

    pub fn navigate_to_relation(&mut self) {
        let doc = match self.selected_doc_meta() {
            Some(d) => d,
            None => return,
        };
        let relations = self.store.related_to(&doc.path);
        let target = match relations.get(self.selected_relation) {
            Some((_, path)) => (*path).clone(),
            None => return,
        };

        if let Some(target_doc) = self.store.get(&target) {
            let doc_type = target_doc.doc_type.clone();
            if let Some(type_idx) = self.doc_types.iter().position(|t| *t == doc_type) {
                self.selected_type = type_idx;
                let docs = self.docs_for_current_type();
                if let Some(doc_idx) = docs.iter().position(|d| d.path == target) {
                    self.selected_doc = doc_idx;
                }
            }
        }
        self.preview_tab = PreviewTab::Preview;
        self.selected_relation = 0;
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

        let doc_type_str = self.create_form.doc_type.to_string().to_lowercase();

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

        let path = crate::cli::create::run(root, config, &doc_type_str, &title, author)?;
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

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers, root: &Path, config: &Config) {
        if self.show_help {
            self.show_help = false;
            return;
        }
        if self.create_form.active {
            return self.handle_create_form_key(code, root, config);
        }
        if self.delete_confirm.active {
            return self.handle_delete_confirm_key(code, root);
        }
        if self.search_mode {
            return self.handle_search_key(code, modifiers);
        }
        if self.fullscreen_doc {
            return self.handle_fullscreen_key(code);
        }
        self.handle_normal_key(code, modifiers);
    }

    fn handle_create_form_key(&mut self, code: KeyCode, root: &Path, config: &Config) {
        match code {
            KeyCode::Esc => self.close_create_form(),
            KeyCode::Enter => {
                let _ = self.submit_create_form(root, config);
            }
            KeyCode::Tab => self.form_next_field(),
            KeyCode::BackTab => self.form_prev_field(),
            KeyCode::Backspace => self.form_backspace(),
            KeyCode::Char(c) => self.form_type_char(c),
            _ => {}
        }
    }

    fn handle_delete_confirm_key(&mut self, code: KeyCode, root: &Path) {
        match code {
            KeyCode::Enter => { let _ = self.confirm_delete(root); }
            KeyCode::Esc => self.close_delete_confirm(),
            _ => {}
        }
    }

    fn handle_search_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Esc => self.exit_search(),
            KeyCode::Enter => self.select_search_result(),
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_search();
            }
            KeyCode::Up => self.search_move_up(),
            KeyCode::Down => self.search_move_down(),
            KeyCode::Char(c) => {
                if modifiers.contains(KeyModifiers::CONTROL) && c == 'k' {
                    self.search_move_up();
                } else if modifiers.contains(KeyModifiers::CONTROL) && c == 'j' {
                    self.search_move_down();
                } else {
                    self.search_query.push(c);
                    self.update_search();
                }
            }
            _ => {}
        }
    }

    fn handle_fullscreen_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.exit_fullscreen(),
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
            KeyCode::Char('g') => self.scroll_offset = 0,
            KeyCode::Char('G') => self.scroll_offset = u16::MAX / 2,
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = true;
            }
            (KeyCode::Char('/'), _) => self.enter_search(),
            (KeyCode::Char('n'), _) => self.open_create_form(),
            (KeyCode::Char('d'), _) if self.selected_doc_meta().is_some() => {
                self.open_delete_confirm();
            }
            (KeyCode::Enter, _) => {
                if self.preview_tab == PreviewTab::Relations {
                    self.navigate_to_relation();
                } else {
                    self.enter_fullscreen();
                }
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.preview_tab == PreviewTab::Relations {
                    self.move_relation_down();
                } else {
                    self.move_down();
                }
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                if self.preview_tab == PreviewTab::Relations {
                    self.move_relation_up();
                } else {
                    self.move_up();
                }
            }
            (KeyCode::Char('h') | KeyCode::Left, _) => self.move_type_prev(),
            (KeyCode::Char('l') | KeyCode::Right, _) => self.move_type_next(),
            (KeyCode::Tab, _) => self.toggle_preview_tab(),
            (KeyCode::Char('g'), _) => self.move_to_top(),
            (KeyCode::Char('G'), _) => self.move_to_bottom(),
            (KeyCode::Char('`'), _) => self.cycle_mode(),
            _ => {}
        }
    }

    pub fn search_move_up(&mut self) {
        if self.search_selected > 0 {
            self.search_selected -= 1;
        }
    }

    pub fn search_move_down(&mut self) {
        if !self.search_results.is_empty() && self.search_selected < self.search_results.len() - 1 {
            self.search_selected += 1;
        }
    }

    fn parse_relations(&self) -> Result<Vec<(String, std::path::PathBuf)>> {
        let related_str = self.create_form.related.trim().to_string();
        if related_str.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in related_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let (rel_type, shorthand) = if let Some((prefix, id)) = entry.split_once(':') {
                let rel: crate::engine::document::RelationType = prefix.trim().parse()?;
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
