use crate::engine::document::{DocMeta, DocType};
use crate::engine::store::{Filter, Store};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Types,
    DocList,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewTab {
    Preview,
    Relations,
}

pub struct App {
    pub store: Store,
    pub active_panel: Panel,
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
}

impl App {
    pub fn new(store: Store) -> Self {
        App {
            store,
            active_panel: Panel::Types,
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
        match self.active_panel {
            Panel::Types => {
                if self.selected_type < self.doc_types.len() - 1 {
                    self.selected_type += 1;
                    self.selected_doc = 0;
                    self.preview_tab = PreviewTab::Preview;
                    self.selected_relation = 0;
                }
            }
            Panel::DocList => {
                let count = self.docs_for_current_type().len();
                if count > 0 && self.selected_doc < count - 1 {
                    self.selected_doc += 1;
                    self.preview_tab = PreviewTab::Preview;
                    self.selected_relation = 0;
                }
            }
        }
    }

    pub fn move_up(&mut self) {
        match self.active_panel {
            Panel::Types => {
                if self.selected_type > 0 {
                    self.selected_type -= 1;
                    self.selected_doc = 0;
                    self.preview_tab = PreviewTab::Preview;
                    self.selected_relation = 0;
                }
            }
            Panel::DocList => {
                if self.selected_doc > 0 {
                    self.selected_doc -= 1;
                    self.preview_tab = PreviewTab::Preview;
                    self.selected_relation = 0;
                }
            }
        }
    }

    pub fn move_to_top(&mut self) {
        match self.active_panel {
            Panel::Types => {
                self.selected_type = 0;
                self.selected_doc = 0;
            }
            Panel::DocList => {
                self.selected_doc = 0;
            }
        }
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
        match self.active_panel {
            Panel::Types => {
                self.selected_type = self.doc_types.len() - 1;
                self.selected_doc = 0;
            }
            Panel::DocList => {
                let count = self.docs_for_current_type().len();
                if count > 0 {
                    self.selected_doc = count - 1;
                }
            }
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
}
