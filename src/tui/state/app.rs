use super::forms::{CreateForm, DeleteConfirm, LinkEditor, REL_TYPES, StatusPicker};
#[cfg(feature = "agent")]
use super::forms::AgentDialog;
use super::graph::traverse_dependency_chain;

use crate::engine::cache::DiskCache;
use crate::engine::config::{Config, NumberingStrategy};
use crate::engine::document::{rewrite_frontmatter, DocMeta, DocType, RelationType, Status};
use crate::engine::fs::FileSystem;
use crate::engine::git_status::GitStatusCache;
use crate::engine::reservation::ReservationProgress;
use crate::engine::store::{Filter, Store};
#[cfg(feature = "agent")]
use crate::tui::agent::{load_all_records, AgentSpawner};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct SearchEntry {
    pub path: PathBuf,
    pub searchable: String, // pre-lowercased "title\0tag1\0tag2\0path"
}

pub struct CreateResult {
    pub path: PathBuf,
    pub doc_type: DocType,
}

pub enum AppEvent {
    Terminal(crossterm::event::KeyEvent),
    FileChange(notify::Event),
    ExpansionResult { path: PathBuf, body: String, body_hash: u64 },
    DiagramRendered { source_hash: u64, entry: crate::tui::content::diagram::DiagramCacheEntry },
    ProbeResult {
        picker: ratatui_image::picker::Picker,
        protocol: crate::tui::infra::terminal_caps::TerminalImageProtocol,
        tool_availability: crate::tui::content::diagram::ToolAvailability,
    },
    CreateStarted,
    CreateProgress { message: String },
    CreateComplete { result: Result<CreateResult, String> },
    #[cfg(feature = "agent")]
    AgentFinished,
}

fn update_tags(root: &Path, relative: &Path, tags: &[String], fs: &dyn FileSystem) -> Result<()> {
    let full_path = root.join(relative);
    rewrite_frontmatter(&full_path, fs, |doc| {
        let tag_values: Vec<serde_yaml::Value> = tags.iter()
            .map(|t| serde_yaml::Value::String(t.clone()))
            .collect();
        doc["tags"] = serde_yaml::Value::Sequence(tag_values);
        Ok(())
    })
}

pub fn resolve_editor_from(editor: Option<&str>, visual: Option<&str>) -> String {
    if let Some(e) = editor {
        if !e.is_empty() {
            return e.to_string();
        }
    }
    if let Some(v) = visual {
        if !v.is_empty() {
            return v.to_string();
        }
    }
    "vi".to_string()
}

pub fn resolve_editor() -> String {
    resolve_editor_from(
        std::env::var("EDITOR").ok().as_deref(),
        std::env::var("VISUAL").ok().as_deref(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterField {
    Status,
    Tag,
    ClearAction,
}

impl FilterField {
    pub fn next(self) -> Self {
        match self {
            FilterField::Status => FilterField::Tag,
            FilterField::Tag => FilterField::ClearAction,
            FilterField::ClearAction => FilterField::Status,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            FilterField::Status => FilterField::ClearAction,
            FilterField::Tag => FilterField::Status,
            FilterField::ClearAction => FilterField::Tag,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub path: PathBuf,
    pub title: String,
    pub doc_type: DocType,
    pub status: Status,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct DocListNode {
    pub path: PathBuf,
    pub id: String,
    pub title: String,
    pub doc_type: DocType,
    pub status: Status,
    pub depth: usize,
    pub is_parent: bool,
    pub is_virtual: bool,
    pub has_duplicate_id: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Types,
    Filters,
    Metrics,
    Graph,
    #[cfg(feature = "agent")]
    Agents,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::Types => ViewMode::Filters,
            ViewMode::Filters => ViewMode::Metrics,
            ViewMode::Metrics => ViewMode::Graph,
            #[cfg(feature = "agent")]
            ViewMode::Graph => ViewMode::Agents,
            #[cfg(not(feature = "agent"))]
            ViewMode::Graph => ViewMode::Types,
            #[cfg(feature = "agent")]
            ViewMode::Agents => ViewMode::Types,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            ViewMode::Types => "Types",
            ViewMode::Filters => "Filters",
            ViewMode::Metrics => "Metrics",
            ViewMode::Graph => "Graph",
            #[cfg(feature = "agent")]
            ViewMode::Agents => "Agents",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewTab {
    Preview,
    Relations,
}

pub const SCROLL_PADDING: usize = 2;

pub struct App {
    pub fs: Box<dyn FileSystem>,
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
    pub status_picker: StatusPicker,
    pub link_editor: LinkEditor,
    #[cfg(feature = "agent")]
    pub agent_dialog: AgentDialog,
    #[cfg(feature = "agent")]
    pub agent_spawner: AgentSpawner,
    pub view_mode: ViewMode,
    pub graph_nodes: Vec<GraphNode>,
    pub graph_selected: usize,
    pub editor_request: Option<PathBuf>,
    pub filter_focused: FilterField,
    pub filter_status: Option<Status>,
    pub filter_tag: Option<String>,
    pub available_tags: Vec<String>,
    pub type_icons: HashMap<String, String>,
    pub type_plurals: HashMap<String, String>,
    pub expanded_parents: HashSet<PathBuf>,
    pub doc_tree: Vec<DocListNode>,
    pub show_warnings: bool,
    pub warnings_selected: usize,
    pub validation_errors: Vec<String>,
    pub validation_warnings: Vec<String>,
    pub fix_request: bool,
    pub fix_result: Option<String>,
    pub doc_list_offset: usize,
    pub doc_list_height: usize,
    pub fullscreen_height: usize,
    #[cfg(feature = "agent")]
    pub agent_selected_index: usize,
    #[cfg(feature = "agent")]
    pub resume_request: Option<String>,
    pub expanded_body_cache: HashMap<PathBuf, String>,
    pub expansion_in_flight: Option<PathBuf>,
    pub event_tx: crossbeam_channel::Sender<AppEvent>,
    pub expansion_cancel: Option<Arc<AtomicBool>>,
    pub disk_cache: DiskCache,
    pub terminal_image_protocol: crate::tui::infra::terminal_caps::TerminalImageProtocol,
    pub tool_availability: crate::tui::content::diagram::ToolAvailability,
    pub diagram_cache: crate::tui::content::diagram::DiagramCache,
    pub picker: ratatui_image::picker::Picker,
    pub image_states: HashMap<u64, ratatui_image::protocol::StatefulProtocol>,
    pub ascii_diagrams: bool,
    pub diagram_blocks_cache: Option<(PathBuf, u64, Vec<crate::tui::content::diagram::DiagramBlock>)>,
    pub filtered_docs_cache: Option<Vec<PathBuf>>,
    pub search_index: Vec<SearchEntry>,
    pub git_status_cache: GitStatusCache,
}

impl App {
    pub fn new(store: Store, config: &Config, picker: ratatui_image::picker::Picker, fs: Box<dyn FileSystem>) -> Self {
        let default_glyphs = ["●", "■", "▲", "◆", "★", "◎"];
        let type_icons: HashMap<String, String> = config.documents.types.iter().enumerate().map(|(i, t)| {
            let icon = t.icon.clone().unwrap_or_else(|| default_glyphs[i % default_glyphs.len()].to_string());
            (t.name.clone(), icon)
        }).collect();
        let type_plurals: HashMap<String, String> = config.documents.types.iter()
            .map(|t| (t.name.clone(), t.plural.clone()))
            .collect();

        let (event_tx, _event_rx) = crossbeam_channel::unbounded();
        let git_status_cache = GitStatusCache::new(store.root());

        let mut app = App {
            fs,
            store,
            selected_type: 0,
            selected_doc: 0,
            doc_types: config.documents.types.iter().map(|t| DocType::new(&t.name)).collect(),
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
            status_picker: StatusPicker::new(),
            link_editor: LinkEditor::new(),
            #[cfg(feature = "agent")]
            agent_dialog: AgentDialog::new(),
            #[cfg(feature = "agent")]
            agent_spawner: AgentSpawner::new(),
            view_mode: ViewMode::Types,
            graph_nodes: Vec::new(),
            graph_selected: 0,
            editor_request: None,
            filter_focused: FilterField::Status,
            filter_status: None,
            filter_tag: None,
            available_tags: Vec::new(),
            type_icons,
            type_plurals,
            expanded_parents: HashSet::new(),
            doc_tree: Vec::new(),
            show_warnings: false,
            warnings_selected: 0,
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
            fix_request: false,
            fix_result: None,
            doc_list_offset: 0,
            doc_list_height: 0,
            fullscreen_height: 0,
            #[cfg(feature = "agent")]
            agent_selected_index: 0,
            #[cfg(feature = "agent")]
            resume_request: None,
            expanded_body_cache: HashMap::new(),
            expansion_in_flight: None,
            event_tx,
            expansion_cancel: None,
            disk_cache: DiskCache::new(),
            terminal_image_protocol: crate::tui::infra::terminal_caps::TerminalImageProtocol::Halfblocks,
            tool_availability: crate::tui::content::diagram::ToolAvailability { d2: false },
            diagram_cache: crate::tui::content::diagram::DiagramCache::new(),
            picker,
            image_states: HashMap::new(),
            ascii_diagrams: config.ui.ascii_diagrams,
            diagram_blocks_cache: None,
            filtered_docs_cache: None,
            search_index: Vec::new(),
            git_status_cache,
        };
        app.rebuild_search_index();
        app.build_doc_tree();
        app
    }

    pub fn refresh_validation(&mut self, config: &Config) {
        let result = crate::engine::validation::validate_full(&self.store, config);
        self.validation_errors = result.errors.iter().map(|e| e.to_string()).collect();
        self.validation_warnings = result.warnings.iter().map(|e| e.to_string()).collect();
        self.filtered_docs_cache = None;
        self.rebuild_search_index();
    }

    pub fn cycle_mode(&mut self) {
        if self.view_mode == ViewMode::Filters {
            self.reset_filters();
        }
        self.view_mode = self.view_mode.next();
        if self.view_mode == ViewMode::Graph {
            self.rebuild_graph();
        }
        if self.view_mode == ViewMode::Filters {
            self.enter_filters_mode();
            self.selected_doc = 0;
        }
        #[cfg(feature = "agent")]
        if self.view_mode == ViewMode::Agents {
            if let Ok(records) = load_all_records(None) {
                self.agent_spawner.records = records;
            }
            self.agent_selected_index = 0;
        }
    }

    pub fn toggle_expanded(&mut self, path: &Path) {
        let key = path.to_path_buf();
        if !self.expanded_parents.remove(&key) {
            self.expanded_parents.insert(key);
        }
        self.build_doc_tree();
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded_parents.contains(path)
    }

    pub fn build_doc_tree(&mut self) {
        let docs = self.store.list(&Filter {
            doc_type: Some(self.current_type().clone()),
            ..Default::default()
        });

        let mut sorted: Vec<&DocMeta> = docs.into_iter().collect();
        sorted.sort_by(|a, b| DocMeta::sort_by_date(a, b));

        // Count occurrences of each ID (excluding children) to detect duplicates
        let mut id_counts: HashMap<String, usize> = HashMap::new();
        for doc in &sorted {
            if self.store.parent_of(&doc.path).is_none() {
                *id_counts.entry(doc.id.clone()).or_insert(0) += 1;
            }
        }

        let mut tree = Vec::new();

        for doc in &sorted {
            if self.store.parent_of(&doc.path).is_some() {
                continue;
            }

            let children = self.store.children_of(&doc.path);
            let is_parent = !children.is_empty();
            let has_duplicate_id = id_counts.get(&doc.id).copied().unwrap_or(0) > 1;

            tree.push(DocListNode {
                path: doc.path.clone(),
                id: doc.id.clone(),
                title: doc.title.clone(),
                doc_type: doc.doc_type.clone(),
                status: doc.status.clone(),
                depth: 0,
                is_parent,
                is_virtual: doc.virtual_doc,
                has_duplicate_id,
            });

            if is_parent && self.is_expanded(&doc.path) {
                let mut child_docs: Vec<&DocMeta> = children
                    .iter()
                    .filter_map(|cp| self.store.get(cp))
                    .collect();
                child_docs.sort_by(|a, b| DocMeta::sort_by_date(a, b));

                for child in child_docs {
                    tree.push(DocListNode {
                        path: child.path.clone(),
                        id: child.id.clone(),
                        title: child.title.clone(),
                        doc_type: child.doc_type.clone(),
                        status: child.status.clone(),
                        depth: 1,
                        is_parent: false,
                        is_virtual: child.virtual_doc,
                        has_duplicate_id: false,
                    });
                }
            }
        }

        self.doc_tree = tree;
    }

    pub fn enter_filters_mode(&mut self) {
        let mut tags: Vec<String> = self
            .store
            .all_docs()
            .iter()
            .flat_map(|doc| doc.tags.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        tags.sort();
        self.available_tags = tags;
    }

    pub fn rebuild_search_index(&mut self) {
        self.search_index = self.store.all_docs().iter().map(|doc| {
            let mut searchable = doc.title.to_lowercase();
            for tag in &doc.tags {
                searchable.push('\0');
                searchable.push_str(&tag.to_lowercase());
            }
            searchable.push('\0');
            searchable.push_str(&doc.path.to_string_lossy().to_lowercase());
            SearchEntry {
                path: doc.path.clone(),
                searchable,
            }
        }).collect();
    }

    pub fn reset_filters(&mut self) {
        self.filter_status = None;
        self.filter_tag = None;
        self.filter_focused = FilterField::Status;
        self.filtered_docs_cache = None;
    }

    pub fn cycle_filter_value_next(&mut self) {
        match self.filter_focused {
            FilterField::Status => {
                self.filter_status = match &self.filter_status {
                    None => Some(Status::Draft),
                    Some(Status::Draft) => Some(Status::Review),
                    Some(Status::Review) => Some(Status::Accepted),
                    Some(Status::Accepted) => Some(Status::Rejected),
                    Some(Status::Rejected) => Some(Status::Superseded),
                    Some(Status::Superseded) => None,
                };
            }
            FilterField::Tag => {
                self.filter_tag = match &self.filter_tag {
                    None => self.available_tags.first().cloned(),
                    Some(current) => {
                        let pos = self.available_tags.iter().position(|t| t == current);
                        match pos {
                            Some(i) if i + 1 < self.available_tags.len() => {
                                Some(self.available_tags[i + 1].clone())
                            }
                            _ => None,
                        }
                    }
                };
            }
            FilterField::ClearAction => {}
        }
        self.filtered_docs_cache = None;
    }

    pub fn cycle_filter_value_prev(&mut self) {
        match self.filter_focused {
            FilterField::Status => {
                self.filter_status = match &self.filter_status {
                    None => Some(Status::Superseded),
                    Some(Status::Superseded) => Some(Status::Rejected),
                    Some(Status::Rejected) => Some(Status::Accepted),
                    Some(Status::Accepted) => Some(Status::Review),
                    Some(Status::Review) => Some(Status::Draft),
                    Some(Status::Draft) => None,
                };
            }
            FilterField::Tag => {
                self.filter_tag = match &self.filter_tag {
                    None => self.available_tags.last().cloned(),
                    Some(current) => {
                        let pos = self.available_tags.iter().position(|t| t == current);
                        match pos {
                            Some(0) | None => None,
                            Some(i) => Some(self.available_tags[i - 1].clone()),
                        }
                    }
                };
            }
            FilterField::ClearAction => {}
        }
        self.filtered_docs_cache = None;
    }

    pub fn rebuild_graph(&mut self) {
        let all_docs = self.store.all_docs();

        let mut roots: Vec<&DocMeta> = all_docs
            .iter()
            .filter(|doc| {
                !doc.related
                    .iter()
                    .any(|r| r.rel_type == RelationType::Implements)
            })
            .copied()
            .collect();

        roots.sort_by(|a, b| a.doc_type.to_string().cmp(&b.doc_type.to_string()).then(a.title.cmp(&b.title)));

        let mut nodes = Vec::new();
        let mut visited = HashSet::new();

        for root in &roots {
            traverse_dependency_chain(&self.store, &root.path, 0, &mut nodes, &mut visited);
        }

        self.graph_nodes = nodes;
        self.graph_selected = 0;
    }

    pub fn current_type(&self) -> &DocType {
        &self.doc_types[self.selected_type]
    }

    pub fn docs_for_current_type(&self) -> Vec<&DocMeta> {
        let mut docs = self.store.list(&Filter {
            doc_type: Some(self.current_type().clone()),
            ..Default::default()
        });
        docs.sort_by(|a, b| DocMeta::sort_by_date(a, b));
        docs
    }

    pub fn selected_doc_meta(&self) -> Option<&DocMeta> {
        self.doc_tree
            .get(self.selected_doc)
            .and_then(|node| self.store.get(&node.path))
    }

    pub fn doc_count(&self, doc_type: &DocType) -> usize {
        self.store
            .list(&Filter {
                doc_type: Some(doc_type.clone()),
                ..Default::default()
            })
            .len()
    }

    pub fn adjust_viewport(&mut self, doc_count: usize) {
        let visible = self.doc_list_height;
        if visible == 0 || doc_count == 0 {
            return;
        }

        if self.selected_doc < self.doc_list_offset + SCROLL_PADDING {
            self.doc_list_offset = self.selected_doc.saturating_sub(SCROLL_PADDING);
        }

        if visible > SCROLL_PADDING && self.selected_doc >= self.doc_list_offset + visible - SCROLL_PADDING {
            self.doc_list_offset = self.selected_doc + SCROLL_PADDING + 1 - visible;
        }

        let max_offset = doc_count.saturating_sub(visible);
        self.doc_list_offset = self.doc_list_offset.min(max_offset);
    }

    pub fn move_down(&mut self) {
        let count = self.doc_tree.len();
        if count > 0 && self.selected_doc < count - 1 {
            self.selected_doc += 1;
        }
        self.adjust_viewport(self.doc_tree.len());
    }

    pub fn move_up(&mut self) {
        if self.selected_doc > 0 {
            self.selected_doc -= 1;
        }
        self.adjust_viewport(self.doc_tree.len());
    }

    pub fn clamp_selected_doc(&mut self) {
        let count = self.doc_tree.len();
        if count == 0 {
            self.selected_doc = 0;
        } else if self.selected_doc >= count {
            self.selected_doc = count - 1;
        }
    }

    pub fn move_to_top(&mut self) {
        self.selected_doc = 0;
        self.doc_list_offset = 0;
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

    pub fn half_page_down(&mut self, list_len: usize) {
        if list_len == 0 {
            return;
        }
        let jump = self.doc_list_height / 2;
        self.selected_doc = (self.selected_doc + jump).min(list_len - 1);
        self.adjust_viewport(list_len);
    }

    pub fn half_page_up(&mut self, list_len: usize) {
        let jump = self.doc_list_height / 2;
        self.selected_doc = self.selected_doc.saturating_sub(jump);
        self.adjust_viewport(list_len);
    }

    pub fn move_to_bottom(&mut self) {
        let count = self.doc_tree.len();
        if count > 0 {
            self.selected_doc = count - 1;
            self.doc_list_offset = count.saturating_sub(self.doc_list_height);
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
        let mut results: Vec<_> = self.search_index.iter()
            .filter(|e| e.searchable.contains(&query))
            .map(|e| e.path.clone())
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
                self.build_doc_tree();
                if let Some(di) = self.doc_tree.iter().position(|n| n.path == path) {
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

    pub fn relation_items(&self, doc: &DocMeta) -> Vec<PathBuf> {
        let mut items = Vec::new();

        // Chain: walk Implements links upward from doc
        let mut chain = Vec::new();
        let mut current_path = doc.path.clone();
        loop {
            let current_doc = match self.store.get(&current_path) {
                Some(d) => d,
                None => break,
            };
            let implements_target = current_doc.related.iter().find_map(|r| {
                if r.rel_type == RelationType::Implements {
                    // resolve target path via forward_links
                    if let Some(fwd) = self.store.forward_links.get(&current_doc.path) {
                        for (rel, target) in fwd {
                            if *rel == RelationType::Implements {
                                return Some(target.clone());
                            }
                        }
                    }
                    None
                } else {
                    None
                }
            });
            match implements_target {
                Some(parent) => {
                    chain.push(parent.clone());
                    current_path = parent;
                }
                None => break,
            }
        }
        chain.reverse();
        items.extend(chain);

        // Children: docs that implement this doc (reverse Implements)
        if let Some(rev) = self.store.reverse_links.get(&doc.path) {
            for (rel, source) in rev {
                if *rel == RelationType::Implements {
                    items.push(source.clone());
                }
            }
        }

        // Related: forward and reverse RelatedTo links
        if let Some(fwd) = self.store.forward_links.get(&doc.path) {
            for (rel, target) in fwd {
                if *rel == RelationType::RelatedTo {
                    items.push(target.clone());
                }
            }
        }
        if let Some(rev) = self.store.reverse_links.get(&doc.path) {
            for (rel, source) in rev {
                if *rel == RelationType::RelatedTo {
                    items.push(source.clone());
                }
            }
        }

        items
    }

    pub fn relation_count(&self) -> usize {
        match self.selected_doc_meta() {
            Some(doc) => self.relation_items(doc).len(),
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
        let items = self.relation_items(doc);
        let target = match items.get(self.selected_relation) {
            Some(path) => path.clone(),
            None => return,
        };

        if let Some(target_doc) = self.store.get(&target) {
            let doc_type = target_doc.doc_type.clone();
            if let Some(type_idx) = self.doc_types.iter().position(|t| *t == doc_type) {
                self.selected_type = type_idx;
                self.build_doc_tree();
                if let Some(doc_idx) = self.doc_tree.iter().position(|n| n.path == target) {
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
            self.build_doc_tree();
        }
    }

    pub fn move_type_prev(&mut self) {
        if self.selected_type > 0 {
            self.selected_type -= 1;
            self.selected_doc = 0;
            self.build_doc_tree();
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
            "unknown".to_string()
        } else {
            self.create_form.author.trim().to_string()
        };

        let relations = match self.parse_relations() {
            Ok(r) => r,
            Err(e) => {
                self.create_form.error = Some(e.to_string());
                return Err(e);
            }
        };

        let tags_str = self.create_form.tags.trim().to_string();

        let type_def = config.type_by_name(&doc_type_str);
        let is_reserved = type_def
            .map(|td| matches!(td.numbering, NumberingStrategy::Reserved))
            .unwrap_or(false);

        if is_reserved {
            let root = root.to_path_buf();
            let config = config.clone();
            let doc_type_str = doc_type_str.clone();
            let title = title.clone();
            let author = author.clone();
            let tags_str = tags_str.clone();
            let relations = relations.clone();
            let tx = self.event_tx.clone();
            let doc_type = self.create_form.doc_type.clone();

            self.create_form.loading = true;
            self.create_form.status_message = Some("Reserving...".to_string());
            let _ = self.event_tx.send(AppEvent::CreateStarted);

            std::thread::spawn(move || {
                let thread_fs = crate::engine::fs::RealFileSystem;
                let progress_tx = tx.clone();
                let result = (|| -> Result<CreateResult, String> {
                    let path = crate::cli::create::run(
                        &root,
                        &config,
                        &doc_type_str,
                        &title,
                        &author,
                        |p| {
                            let message = match &p {
                                ReservationProgress::QueryingRemote => {
                                    "Querying remote for latest tag...".to_string()
                                }
                                ReservationProgress::PushAttempt { attempt, max, candidate } => {
                                    format!("Push attempt {}/{} for candidate {}...", attempt, max, candidate)
                                }
                                ReservationProgress::PushRejected { candidate } => {
                                    format!("Push rejected for candidate {}, retrying...", candidate)
                                }
                                ReservationProgress::Reserved { number } => {
                                    format!("Reserved number {}", number)
                                }
                            };
                            let _ = progress_tx.send(AppEvent::CreateProgress { message });
                        },
                    )
                    .map_err(|e| e.to_string())?;

                    let relative = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();
                    let relative_str = relative.to_string_lossy().to_string();

                    if !tags_str.is_empty() {
                        let tags: Vec<String> = tags_str
                            .split(',')
                            .map(|t| t.trim().to_string())
                            .filter(|t| !t.is_empty())
                            .collect();
                        update_tags(&root, &relative, &tags, &thread_fs).map_err(|e| e.to_string())?;
                    }

                    if !relations.is_empty() {
                        let store = Store::load(&root, &config).map_err(|e| e.to_string())?;
                        for (rel_type, target_path) in &relations {
                            crate::cli::link::link(
                                &root,
                                &store,
                                &relative_str,
                                rel_type,
                                &target_path.to_string_lossy(),
                                &thread_fs,
                            )
                            .map_err(|e| e.to_string())?;
                        }
                    }

                    Ok(CreateResult {
                        path: relative,
                        doc_type,
                    })
                })();

                let _ = tx.send(AppEvent::CreateComplete { result });
            });

            return Ok(());
        }

        let path = crate::cli::create::run(root, config, &doc_type_str, &title, &author, |_| {})?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let relative_str = relative.to_string_lossy().to_string();

        if !tags_str.is_empty() {
            let tags: Vec<String> = tags_str.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            update_tags(root, &relative, &tags, &*self.fs)?;
        }

        // Reload the store before applying relations so the new doc is resolvable
        let _ = self.store.reload_file(root, &relative, &*self.fs);

        // Apply relations
        for (rel_type, target_path) in &relations {
            crate::cli::link::link(root, &self.store, &relative_str, rel_type, &target_path.to_string_lossy(), &*self.fs)?;
        }

        // Reload again to pick up the relation changes
        let _ = self.store.reload_file(root, &relative, &*self.fs);
        self.filtered_docs_cache = None;
        self.rebuild_search_index();

        let doc_type = self.create_form.doc_type.clone();
        if let Some(type_idx) = self.doc_types.iter().position(|t| *t == doc_type) {
            self.selected_type = type_idx;
            self.build_doc_tree();
            if let Some(doc_idx) = self.doc_tree.iter().position(|n| n.path == relative) {
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
        crate::cli::delete::run(root, &self.store, &doc_path_str)?;
        self.store.remove_file(&doc_path);
        self.filtered_docs_cache = None;
        self.rebuild_search_index();

        self.close_delete_confirm();
        self.build_doc_tree();
        self.clamp_selected_doc();
        Ok(())
    }

    pub fn open_status_picker(&mut self) {
        let doc = if self.view_mode == ViewMode::Filters {
            match self.selected_filtered_doc() {
                Some(d) => d,
                None => return,
            }
        } else {
            match self.selected_doc_meta() {
                Some(d) => d,
                None => return,
            }
        };

        let index = match &doc.status {
            Status::Draft => 0,
            Status::Review => 1,
            Status::Accepted => 2,
            Status::Rejected => 3,
            Status::Superseded => 4,
        };
        let path = doc.path.clone();

        self.status_picker.selected = index;
        self.status_picker.doc_path = path;
        self.status_picker.active = true;
    }

    pub fn close_status_picker(&mut self) {
        self.status_picker.active = false;
        self.status_picker.selected = 0;
        self.status_picker.doc_path = PathBuf::new();
    }

    pub fn confirm_status_change(&mut self, root: &Path, _config: &Config) -> Result<()> {
        let status = match self.status_picker.selected {
            0 => Status::Draft,
            1 => Status::Review,
            2 => Status::Accepted,
            3 => Status::Rejected,
            4 => Status::Superseded,
            _ => return Err(anyhow!("invalid status index")),
        };
        let doc_path = self.status_picker.doc_path.clone();
        let doc_path_str = doc_path.to_string_lossy().to_string();

        crate::cli::update::run(root, &self.store, &doc_path_str, &[("status", &status.to_string())])?;
        self.store.reload_file(root, &doc_path, &*self.fs)?;
        self.filtered_docs_cache = None;
        self.rebuild_search_index();
        self.build_doc_tree();
        self.close_status_picker();
        Ok(())
    }

    pub fn open_link_editor(&mut self) {
        let doc = if self.view_mode == ViewMode::Filters {
            match self.selected_filtered_doc() {
                Some(d) => d,
                None => return,
            }
        } else {
            match self.selected_doc_meta() {
                Some(d) => d,
                None => return,
            }
        };

        let path = doc.path.clone();

        self.link_editor.active = true;
        self.link_editor.doc_path = path;
        self.link_editor.rel_type_index = 0;
        self.link_editor.query = String::new();
        self.link_editor.selected = 0;
        self.update_link_search();
    }

    pub fn close_link_editor(&mut self) {
        self.link_editor.active = false;
        self.link_editor.doc_path = PathBuf::new();
        self.link_editor.rel_type_index = 0;
        self.link_editor.query = String::new();
        self.link_editor.results = Vec::new();
        self.link_editor.selected = 0;
    }

    pub fn update_link_search(&mut self) {
        let query = self.link_editor.query.to_lowercase();
        let doc_path = self.link_editor.doc_path.clone();

        let mut candidates: Vec<(String, PathBuf)> = self
            .store
            .all_docs()
            .iter()
            .filter(|d| d.path != doc_path)
            .filter(|d| {
                if query.is_empty() {
                    return true;
                }
                let display = format!("{}: {}", d.id.to_uppercase(), d.title).to_lowercase();
                display.contains(&query)
            })
            .map(|d| {
                let display = format!("{}: {}", d.id.to_uppercase(), d.title);
                (display, d.path.clone())
            })
            .collect();

        candidates.sort_by(|a, b| a.0.cmp(&b.0));

        self.link_editor.results = candidates.into_iter().map(|(_, path)| path).collect();
        if self.link_editor.selected >= self.link_editor.results.len() {
            self.link_editor.selected = self.link_editor.results.len().saturating_sub(1);
        }
    }

    pub(crate) fn confirm_link(&mut self, root: &Path) -> Result<()> {
        let selected = self.link_editor.selected;
        let target_path = self.link_editor.results[selected].clone();
        let from = self.link_editor.doc_path.to_string_lossy().to_string();
        let to = target_path.to_string_lossy().to_string();
        let rel_type = REL_TYPES[self.link_editor.rel_type_index];

        crate::cli::link::link(root, &self.store, &from, rel_type, &to, &*self.fs)?;
        self.store.reload_file(root, &self.link_editor.doc_path.clone(), &*self.fs)?;
        self.filtered_docs_cache = None;
        self.rebuild_search_index();
        self.build_doc_tree();
        self.close_link_editor();
        Ok(())
    }

    pub fn open_warnings(&mut self) {
        self.show_warnings = true;
        self.warnings_selected = 0;
        self.fix_result = None;
    }

    pub fn close_warnings(&mut self) {
        self.show_warnings = false;
        self.warnings_selected = 0;
    }

    pub fn warnings_move_up(&mut self) {
        if self.warnings_selected > 0 {
            self.warnings_selected -= 1;
        }
    }

    pub fn total_warnings_count(&self) -> usize {
        self.store.parse_errors().len()
            + self.validation_errors.len()
            + self.validation_warnings.len()
    }

    pub fn warnings_move_down(&mut self) {
        let len = self.total_warnings_count();
        if len > 0 && self.warnings_selected < len - 1 {
            self.warnings_selected += 1;
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
                .map_err(|_| anyhow!("Cannot resolve: {}", shorthand))?;
            results.push((rel_type, doc.path.clone()));
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::store::Store;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn make_dummy_node(index: usize) -> DocListNode {
        DocListNode {
            path: PathBuf::from(format!("docs/rfcs/RFC-{:03}.md", index)),
            id: format!("RFC-{:03}", index),
            title: format!("Doc {}", index),
            doc_type: DocType::new("rfc"),
            status: Status::Draft,
            depth: 0,
            is_parent: false,
            is_virtual: false,
            has_duplicate_id: false,
        }
    }

    fn make_test_app(doc_count: usize) -> App {
        let store = Store {
            root: PathBuf::from("."),
            docs: HashMap::new(),
            forward_links: HashMap::new(),
            reverse_links: HashMap::new(),
            children: HashMap::new(),
            parent_of: HashMap::new(),
            parse_errors: Vec::new(),
        };

        let (tx, _rx) = crossbeam_channel::unbounded();

        let app = App {
            fs: Box::new(crate::engine::fs::RealFileSystem),
            store,
            selected_type: 0,
            selected_doc: 0,
            doc_types: vec![DocType::new("rfc")],
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
            status_picker: StatusPicker::new(),
            link_editor: LinkEditor::new(),
            #[cfg(feature = "agent")]
            agent_dialog: AgentDialog::new(),
            #[cfg(feature = "agent")]
            agent_spawner: AgentSpawner::new(),
            view_mode: ViewMode::Types,
            graph_nodes: Vec::new(),
            graph_selected: 0,
            editor_request: None,
            filter_focused: FilterField::Status,
            filter_status: None,
            filter_tag: None,
            available_tags: Vec::new(),
            type_icons: HashMap::new(),
            type_plurals: HashMap::new(),
            expanded_parents: HashSet::new(),
            doc_tree: (0..doc_count).map(make_dummy_node).collect(),
            show_warnings: false,
            warnings_selected: 0,
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
            fix_request: false,
            fix_result: None,
            doc_list_offset: 0,
            doc_list_height: 0,
            fullscreen_height: 0,
            #[cfg(feature = "agent")]
            agent_selected_index: 0,
            #[cfg(feature = "agent")]
            resume_request: None,
            expanded_body_cache: HashMap::new(),
            expansion_in_flight: None,
            event_tx: tx,
            expansion_cancel: None,
            disk_cache: DiskCache::new(),
            terminal_image_protocol: crate::tui::infra::terminal_caps::TerminalImageProtocol::Unsupported,
            tool_availability: crate::tui::content::diagram::ToolAvailability { d2: false },
            diagram_cache: crate::tui::content::diagram::DiagramCache::new(),
            picker: ratatui_image::picker::Picker::halfblocks(),
            image_states: HashMap::new(),
            ascii_diagrams: false,
            diagram_blocks_cache: None,
            filtered_docs_cache: None,
            search_index: Vec::new(),
            git_status_cache: GitStatusCache::new(Path::new(".")),
        };
        app
    }

    #[test]
    fn viewport_adjusts_down_with_padding() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;

        for _ in 0..7 {
            app.move_down();
        }
        assert_eq!(app.selected_doc, 7);
        assert_eq!(app.doc_list_offset, 0, "selection at 7, still within viewport");

        app.move_down();
        assert_eq!(app.selected_doc, 8);
        assert_eq!(app.doc_list_offset, 1, "viewport should scroll to maintain 2-row bottom padding");
    }

    #[test]
    fn viewport_adjusts_up_with_padding() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;
        app.doc_list_offset = 5;
        app.selected_doc = 7;

        app.move_up();
        assert_eq!(app.selected_doc, 6);
        assert_eq!(app.doc_list_offset, 4);

        app.move_up();
        assert_eq!(app.selected_doc, 5);
        assert_eq!(app.doc_list_offset, 3);
    }

    #[test]
    fn sticky_viewport_on_scroll_up() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;
        app.doc_list_offset = 5;
        app.selected_doc = 12;

        app.move_up();
        assert_eq!(app.selected_doc, 11);
        assert_eq!(app.doc_list_offset, 5, "viewport stays put while selection is in interior");

        app.move_up();
        assert_eq!(app.selected_doc, 10);
        assert_eq!(app.doc_list_offset, 5);

        app.move_up();
        assert_eq!(app.selected_doc, 9);
        assert_eq!(app.doc_list_offset, 5);

        app.move_up();
        assert_eq!(app.selected_doc, 8);
        assert_eq!(app.doc_list_offset, 5);

        app.move_up();
        assert_eq!(app.selected_doc, 7);
        assert_eq!(app.doc_list_offset, 5, "selection at padding boundary, offset still 5");

        app.move_up();
        assert_eq!(app.selected_doc, 6);
        assert_eq!(app.doc_list_offset, 4, "crossed padding boundary, viewport adjusts");
    }

    #[test]
    fn padding_clamped_at_boundaries() {
        let mut app = make_test_app(5);
        app.doc_list_height = 10;

        app.move_up();
        assert_eq!(app.selected_doc, 0);
        assert_eq!(app.doc_list_offset, 0);

        app.selected_doc = 4;
        app.move_down();
        assert_eq!(app.selected_doc, 4, "can't go past the last item");
        assert_eq!(app.doc_list_offset, 0, "offset stays 0 when list fits in viewport");
    }

    #[test]
    fn move_to_top_resets_offset() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;
        app.selected_doc = 15;
        app.doc_list_offset = 8;

        app.move_to_top();
        assert_eq!(app.selected_doc, 0);
        assert_eq!(app.doc_list_offset, 0);
    }

    #[test]
    fn move_to_bottom_sets_max_offset() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;

        app.move_to_bottom();
        assert_eq!(app.selected_doc, 19);
        assert_eq!(app.doc_list_offset, 10);
    }

    #[test]
    fn half_page_down_moves_by_half_height() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;
        app.selected_doc = 0;

        app.half_page_down(20);
        assert_eq!(app.selected_doc, 5);
        // viewport should adjust: selected_doc(5) + SCROLL_PADDING(2) + 1 - visible(10) = -2, so offset stays 0
        assert_eq!(app.doc_list_offset, 0);
    }

    #[test]
    fn half_page_up_moves_by_half_height() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;
        app.selected_doc = 15;
        app.doc_list_offset = 8;

        app.half_page_up(20);
        assert_eq!(app.selected_doc, 10);
    }

    #[test]
    fn fullscreen_half_page_scroll() {
        let mut app = make_test_app(5);
        app.fullscreen_height = 20;
        app.scroll_offset = 0;

        app.handle_fullscreen_key(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(app.scroll_offset, 10);

        app.handle_fullscreen_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn fullscreen_half_page_underflow() {
        let mut app = make_test_app(5);
        app.fullscreen_height = 20;
        app.scroll_offset = 3;

        app.handle_fullscreen_key(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.scroll_offset, 0, "should saturate at 0");
    }

    #[test]
    fn modal_blocks_fullscreen_half_page() {
        let mut app = make_test_app(5);
        app.fullscreen_doc = true;
        app.fullscreen_height = 20;
        app.scroll_offset = 0;
        app.create_form.active = true;

        let root = std::path::PathBuf::from(".");
        let config = Config::default();
        app.handle_key(KeyCode::Char('d'), KeyModifiers::CONTROL, &root, &config);
        assert_eq!(app.scroll_offset, 0, "modal should block Ctrl-D from reaching fullscreen");
    }

    #[test]
    fn half_page_clamps_at_boundaries() {
        let mut app = make_test_app(20);
        app.doc_list_height = 10;
        app.selected_doc = 18;

        app.half_page_down(20);
        assert_eq!(app.selected_doc, 19);

        app.selected_doc = 2;
        app.half_page_up(20);
        assert_eq!(app.selected_doc, 0);
    }

    #[test]
    fn refresh_validation_populates_errors_for_duplicate_ids() {
        use crate::engine::config::Config;
        use crate::engine::document::DocMeta;
        use chrono::Utc;

        let mut store = Store {
            root: PathBuf::from("."),
            docs: HashMap::new(),
            forward_links: HashMap::new(),
            reverse_links: HashMap::new(),
            children: HashMap::new(),
            parent_of: HashMap::new(),
            parse_errors: Vec::new(),
        };

        let meta_a = DocMeta {
            path: PathBuf::from("docs/rfcs/RFC-001.md"),
            title: "First".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::Draft,
            author: "test".to_string(),
            date: Utc::now().date_naive(),
            tags: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            id: "RFC-001".to_string(),
        };
        let meta_b = DocMeta {
            path: PathBuf::from("docs/rfcs/RFC-001-dup.md"),
            title: "Duplicate".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::Draft,
            author: "test".to_string(),
            date: Utc::now().date_naive(),
            tags: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            id: "RFC-001".to_string(),
        };

        store.docs.insert(meta_a.path.clone(), meta_a);
        store.docs.insert(meta_b.path.clone(), meta_b);

        let config = Config::default();
        let mut app = make_test_app(0);
        app.store = store;
        app.refresh_validation(&config);

        assert!(
            !app.validation_errors.is_empty(),
            "expected validation errors for duplicate IDs"
        );
        assert!(
            app.validation_errors.iter().any(|e| e.contains("duplicate id")),
            "expected a 'duplicate id' error, got: {:?}",
            app.validation_errors
        );
    }

    #[test]
    fn total_warnings_count_includes_all_sources() {
        let mut app = make_test_app(0);
        app.validation_errors = vec!["err1".to_string(), "err2".to_string()];
        app.validation_warnings = vec!["warn1".to_string()];

        assert_eq!(app.total_warnings_count(), 3);
    }
}
