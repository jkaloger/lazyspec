use crate::engine::document::{DocMeta, DocType};
use crate::engine::store::{Filter, Store};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Types,
    DocList,
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
}

impl App {
    pub fn new(store: Store) -> Self {
        App {
            store,
            active_panel: Panel::Types,
            selected_type: 0,
            selected_doc: 0,
            doc_types: vec![DocType::Rfc, DocType::Adr, DocType::Spec, DocType::Plan],
            should_quit: false,
            fullscreen_doc: false,
            scroll_offset: 0,
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
                }
            }
            Panel::DocList => {
                let count = self.docs_for_current_type().len();
                if count > 0 && self.selected_doc < count - 1 {
                    self.selected_doc += 1;
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
                }
            }
            Panel::DocList => {
                if self.selected_doc > 0 {
                    self.selected_doc -= 1;
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
}
