use crate::engine::document::{DocType, RelationType};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormField {
    Title,
    Author,
    Tags,
    Related,
}

impl FormField {
    pub(super) fn next(self) -> Self {
        match self {
            FormField::Title => FormField::Author,
            FormField::Author => FormField::Tags,
            FormField::Tags => FormField::Related,
            FormField::Related => FormField::Title,
        }
    }

    pub(super) fn prev(self) -> Self {
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
    pub loading: bool,
    pub status_message: Option<String>,
}

impl CreateForm {
    pub fn new() -> Self {
        CreateForm {
            active: false,
            doc_type: DocType::new(DocType::RFC),
            focused_field: FormField::Title,
            title: String::new(),
            author: String::new(),
            tags: String::new(),
            related: String::new(),
            error: None,
            loading: false,
            status_message: None,
        }
    }

    pub(super) fn reset(&mut self) {
        self.active = false;
        self.focused_field = FormField::Title;
        self.title.clear();
        self.author.clear();
        self.tags.clear();
        self.related.clear();
        self.error = None;
        self.loading = false;
        self.status_message = None;
    }

    pub(super) fn focused_value_mut(&mut self) -> &mut String {
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

pub struct StatusPicker {
    pub active: bool,
    pub selected: usize,
    pub doc_path: PathBuf,
}

impl StatusPicker {
    pub fn new() -> Self {
        StatusPicker {
            active: false,
            selected: 0,
            doc_path: PathBuf::new(),
        }
    }
}

pub const REL_TYPES: [&str; 4] = RelationType::ALL_STRS;

pub struct LinkEditor {
    pub active: bool,
    pub doc_path: PathBuf,
    pub rel_type_index: usize,
    pub query: String,
    pub results: Vec<PathBuf>,
    pub selected: usize,
}

impl LinkEditor {
    pub fn new() -> Self {
        LinkEditor {
            active: false,
            doc_path: PathBuf::new(),
            rel_type_index: 0,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
        }
    }
}

#[cfg(feature = "agent")]
pub struct AgentDialog {
    pub active: bool,
    pub selected_index: usize,
    pub actions: Vec<String>,
    pub doc_path: PathBuf,
    pub doc_title: String,
    pub text_input: Option<String>,
}

#[cfg(feature = "agent")]
impl AgentDialog {
    pub fn new() -> Self {
        AgentDialog {
            active: false,
            selected_index: 0,
            actions: Vec::new(),
            doc_path: PathBuf::new(),
            doc_title: String::new(),
            text_input: None,
        }
    }
}
