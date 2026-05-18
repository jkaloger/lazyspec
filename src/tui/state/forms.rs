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

impl Default for CreateForm {
    fn default() -> Self {
        Self::new()
    }
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

impl Default for DeleteConfirm {
    fn default() -> Self {
        Self::new()
    }
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

impl Default for StatusPicker {
    fn default() -> Self {
        Self::new()
    }
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

impl Default for LinkEditor {
    fn default() -> Self {
        Self::new()
    }
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

pub struct ProvenanceEditor {
    pub active: bool,
    pub doc_path: PathBuf,
    pub input: String,
    pub error: Option<String>,
}

impl Default for ProvenanceEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvenanceEditor {
    pub fn new() -> Self {
        ProvenanceEditor {
            active: false,
            doc_path: PathBuf::new(),
            input: String::new(),
            error: None,
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
impl Default for AgentDialog {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(feature = "agent")]
#[derive(Debug, Clone)]
pub struct KickoffCandidate {
    pub doc_id: String,
    pub title: String,
    pub type_name: String,
    pub current_assignees: Vec<String>,
}

#[cfg(feature = "agent")]
#[derive(Debug, Clone)]
pub enum KickoffFeedback {
    AssignedAndKicked(String),
    AssignedOnly(String),
    Failed(String),
}

#[cfg(feature = "agent")]
#[derive(Debug, Clone, Default)]
pub struct KickoffPicker {
    pub active: bool,
    pub query: String,
    pub eligible: Vec<KickoffCandidate>,
    pub selected: usize,
    pub feedback: Option<KickoffFeedback>,
}

#[cfg(feature = "agent")]
impl KickoffPicker {
    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.query.is_empty() {
            return (0..self.eligible.len()).collect();
        }
        let q = self.query.to_lowercase();
        self.eligible
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.doc_id.to_lowercase().contains(&q) || c.title.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_candidate(&self) -> Option<&KickoffCandidate> {
        let filtered = self.filtered_indices();
        let idx = filtered.get(self.selected)?;
        self.eligible.get(*idx)
    }
}

#[cfg(test)]
#[cfg(feature = "agent")]
mod kickoff_picker_tests {
    use super::*;

    fn candidate(id: &str, title: &str) -> KickoffCandidate {
        KickoffCandidate {
            doc_id: id.to_string(),
            title: title.to_string(),
            type_name: "story".to_string(),
            current_assignees: vec![],
        }
    }

    #[test]
    fn filtered_indices_returns_all_when_query_empty() {
        let picker = KickoffPicker {
            eligible: vec![candidate("STORY-1", "Foo"), candidate("STORY-2", "Bar")],
            ..Default::default()
        };
        assert_eq!(picker.filtered_indices(), vec![0, 1]);
    }

    #[test]
    fn filtered_indices_matches_doc_id_substring() {
        let picker = KickoffPicker {
            eligible: vec![candidate("STORY-1", "Foo"), candidate("STORY-2", "Bar")],
            query: "story-2".to_string(),
            ..Default::default()
        };
        assert_eq!(picker.filtered_indices(), vec![1]);
    }

    #[test]
    fn filtered_indices_matches_title_case_insensitive() {
        let picker = KickoffPicker {
            eligible: vec![
                candidate("STORY-1", "Apples"),
                candidate("STORY-2", "Oranges"),
            ],
            query: "APP".to_string(),
            ..Default::default()
        };
        assert_eq!(picker.filtered_indices(), vec![0]);
    }

    #[test]
    fn selected_candidate_indexes_through_filter() {
        let picker = KickoffPicker {
            eligible: vec![
                candidate("STORY-1", "Foo"),
                candidate("STORY-2", "Bar"),
                candidate("STORY-3", "Baz"),
            ],
            query: "ba".to_string(),
            selected: 1,
            ..Default::default()
        };
        let c = picker.selected_candidate().expect("present");
        assert_eq!(c.doc_id, "STORY-3");
    }

    #[test]
    fn selected_candidate_none_when_out_of_range() {
        let picker = KickoffPicker {
            eligible: vec![candidate("STORY-1", "Foo")],
            selected: 5,
            ..Default::default()
        };
        assert!(picker.selected_candidate().is_none());
    }
}
