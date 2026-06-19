#[cfg(feature = "agent")]
use super::forms::AgentDialog;
use super::forms::{CreateForm, DeleteConfirm, LinkEditor, ProvenanceEditor, StatusPicker};
use super::graph::flatten_forest;

use crate::engine::cache::DiskCache;
use crate::engine::config::{Config, NumberingStrategy, StoreBackend};
use crate::engine::document::{rewrite_frontmatter, DocMeta, DocType, Status};
use crate::engine::fs::FileSystem;
use crate::engine::git_status::{query_git_branch, GitStatusCache};
use crate::engine::reservation::ReservationProgress;
use crate::engine::store::{Filter, Store};
#[cfg(feature = "agent")]
use crate::tui::agent::{load_all_records, AgentSpawner};
use crate::tui::views::status_bar::StatusBarComponents;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

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
    ExpansionResult {
        path: PathBuf,
        body: String,
        body_hash: u64,
    },
    DiagramRendered {
        source_hash: u64,
        entry: crate::tui::content::diagram::DiagramCacheEntry,
    },
    CreateStarted,
    CreateProgress {
        message: String,
    },
    CreateComplete {
        result: Result<CreateResult, String>,
    },
    CacheRefresh,
    GhPushResult(Result<(), String>),
    #[cfg(feature = "agent")]
    AgentFinished,
}

fn update_tags(root: &Path, relative: &Path, tags: &[String], fs: &dyn FileSystem) -> Result<()> {
    let full_path = root.join(relative);
    rewrite_frontmatter(&full_path, fs, |doc| {
        let tag_values: Vec<serde_yaml::Value> = tags
            .iter()
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
    /// A diamond/multi-parent re-encounter: drawn as a one-line back-reference
    /// (Task 3 renders it without recursing). The full node was emitted earlier.
    pub reference: bool,
    /// Doc ids of this node's OWN depth-1 `related-to` neighbours, minus those on
    /// its `implements` lineage (its transitive ancestors and descendants, already
    /// drawn as tree edges through the node). Siblings/cousins reachable only
    /// through a shared ancestor ARE included — they have no `implements` path to
    /// the node, so the link is genuinely cross-cutting. Display-only (RFC-006
    /// Graph mode Phase 1, rendered `┄▷ <id>` by the renderer), sorted for
    /// determinism. This is the node's own depth-1 set, NOT the `context`
    /// command's related set (which also surfaces the related-to links of the
    /// node's ancestors). Back-reference nodes carry an empty set: the annotation
    /// belongs on the full first-encounter node line.
    pub related: Vec<String>,
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

/// The relation tab's three sections, derived from one engine `resolve_chain`:
/// the upward `implements` lineage (chain), the docs that implement this one
/// (children / reverse-implements), and the `related-to` neighbours. Holding
/// `PathBuf`s (not borrowed `&DocMeta`) keeps this owned across the `App`
/// method boundary; the renderer and the navigable item list both derive from
/// it so they stay in lock-step.
#[derive(Debug, Default, Clone)]
pub struct RelationSections {
    pub chain: Vec<PathBuf>,
    pub children: Vec<PathBuf>,
    pub related: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    Types,
    Filters,
    #[cfg(feature = "metrics")]
    Metrics,
    Graph,
    Settings,
    #[cfg(feature = "agent")]
    Agents,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::Types => ViewMode::Filters,
            #[cfg(feature = "metrics")]
            ViewMode::Filters => ViewMode::Metrics,
            #[cfg(feature = "metrics")]
            ViewMode::Metrics => ViewMode::Graph,
            #[cfg(not(feature = "metrics"))]
            ViewMode::Filters => ViewMode::Graph,
            ViewMode::Graph => ViewMode::Settings,
            #[cfg(feature = "agent")]
            ViewMode::Settings => ViewMode::Agents,
            #[cfg(not(feature = "agent"))]
            ViewMode::Settings => ViewMode::Types,
            #[cfg(feature = "agent")]
            ViewMode::Agents => ViewMode::Types,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            ViewMode::Types => "Types",
            ViewMode::Filters => "Filters",
            #[cfg(feature = "metrics")]
            ViewMode::Metrics => "Metrics",
            ViewMode::Graph => "Graph",
            ViewMode::Settings => "Settings",
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
    pub provenance_editor: ProvenanceEditor,
    #[cfg(feature = "agent")]
    pub agent_dialog: AgentDialog,
    #[cfg(feature = "agent")]
    pub agent_spawner: AgentSpawner,
    /// User-authored prompt templates discovered under `.lazyspec/agents/`,
    /// loaded once at construction (ADR-015 zero-defaults: empty when absent).
    #[cfg(feature = "agent")]
    pub agent_prompts: Vec<crate::engine::prompt::AgentPrompt>,
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
    pub wrap_mode: bool,
    pub doc_tree: Vec<DocListNode>,
    pub show_warnings: bool,
    pub warnings_selected: usize,
    pub validation_errors: Vec<String>,
    pub validation_warnings: Vec<String>,
    pub status_bar_warnings: Vec<String>,
    pub fix_request: bool,
    pub config_reload_request: bool,
    pub fix_result: Option<String>,
    pub doc_list_offset: usize,
    pub doc_list_height: usize,
    pub fullscreen_height: usize,
    #[cfg(feature = "agent")]
    pub agent_selected_index: usize,
    #[cfg(feature = "agent")]
    pub resume_request: Option<String>,
    /// A pending interactive-agent terminal handover (RFC-046 slice 5). Set when a
    /// `mode: interactive` template is selected; drained by the event loop, which
    /// suspends/runs/restores the terminal. No AgentRecord is written (AC7).
    #[cfg(feature = "agent")]
    pub interactive_request: Option<super::forms::InteractiveRequest>,
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
    pub image_dimensions_cache: HashMap<PathBuf, (u32, u32)>,
    pub ascii_diagrams: bool,
    pub diagram_blocks_cache: Option<(
        PathBuf,
        u64,
        Vec<crate::tui::content::diagram::DiagramBlock>,
    )>,
    pub filtered_docs_cache: Option<Vec<PathBuf>>,
    pub search_index: Vec<SearchEntry>,
    pub git_branch: Option<String>,
    pub git_status_cache: GitStatusCache,
    pub gh_conflict_message: Option<String>,
    pub gh_push_in_flight: Arc<AtomicBool>,
    pub last_sync: Option<Instant>,
    pub gh_issue_map_stale: bool,
    pub status_bar_enabled: bool,
    pub status_bar_components: StatusBarComponents,
    /// Link-editor keywords (relationship names + inverses), derived from the
    /// config `[[relationships]]` registry at construction. The link editor
    /// cycles `rel_type_index` over this list.
    pub rel_types: Vec<String>,
    pub settings_category: usize,
    pub settings_entry: usize,
    pub settings_drill: Option<usize>,
}

impl App {
    pub fn new(
        store: Store,
        config: &Config,
        picker: ratatui_image::picker::Picker,
        fs: Box<dyn FileSystem>,
    ) -> Self {
        let (event_tx, _event_rx) = crossbeam_channel::unbounded();
        let git_branch = query_git_branch(store.root());
        let git_status_cache = GitStatusCache::new(store.root());
        let has_github_issues = config
            .documents
            .types
            .iter()
            .any(|t| t.store == StoreBackend::GithubIssues);

        #[cfg(feature = "agent")]
        let agent_spawner = AgentSpawner::new(store.root());
        // ADR-015 zero-defaults: an absent agents dir yields no prompts; discovery
        // warnings are surfaced to stderr inside `discover_prompts`, not stored.
        #[cfg(feature = "agent")]
        let agent_prompts = {
            let (prompts, _warnings) = crate::engine::prompt::discover_prompts(store.root(), &*fs);
            prompts
        };

        let mut app = App {
            fs,
            store,
            selected_type: 0,
            selected_doc: 0,
            doc_types: Vec::new(),
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
            provenance_editor: ProvenanceEditor::new(),
            #[cfg(feature = "agent")]
            agent_dialog: AgentDialog::new(),
            #[cfg(feature = "agent")]
            agent_spawner,
            #[cfg(feature = "agent")]
            agent_prompts,
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
            wrap_mode: false,
            doc_tree: Vec::new(),
            show_warnings: false,
            warnings_selected: 0,
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
            status_bar_warnings: Vec::new(),
            fix_request: false,
            config_reload_request: false,
            fix_result: None,
            doc_list_offset: 0,
            doc_list_height: 0,
            fullscreen_height: 0,
            #[cfg(feature = "agent")]
            agent_selected_index: 0,
            #[cfg(feature = "agent")]
            resume_request: None,
            #[cfg(feature = "agent")]
            interactive_request: None,
            expanded_body_cache: HashMap::new(),
            expansion_in_flight: None,
            event_tx,
            expansion_cancel: None,
            disk_cache: DiskCache::new(),
            terminal_image_protocol:
                crate::tui::infra::terminal_caps::TerminalImageProtocol::Halfblocks,
            tool_availability: crate::tui::content::diagram::ToolAvailability { d2: false },
            diagram_cache: crate::tui::content::diagram::DiagramCache::new(),
            picker,
            image_states: HashMap::new(),
            image_dimensions_cache: HashMap::new(),
            ascii_diagrams: false,
            diagram_blocks_cache: None,
            filtered_docs_cache: None,
            search_index: Vec::new(),
            git_branch,
            git_status_cache,
            gh_conflict_message: None,
            gh_push_in_flight: Arc::new(AtomicBool::new(false)),
            last_sync: if has_github_issues {
                Some(Instant::now())
            } else {
                None
            },
            gh_issue_map_stale: false,
            status_bar_enabled: false,
            status_bar_components: StatusBarComponents::default(),
            rel_types: Vec::new(),
            settings_category: 0,
            settings_entry: 0,
            settings_drill: None,
        };
        app.apply_config(config);
        app.rebuild_search_index();
        app.build_doc_tree();
        app
    }

    pub fn apply_config(&mut self, config: &Config) {
        let default_glyphs = ["●", "■", "▲", "◆", "★", "◎"];
        self.type_icons.clear();
        self.type_plurals.clear();
        self.doc_types.clear();
        for (i, t) in config.documents.types.iter().enumerate() {
            let icon = t
                .icon
                .clone()
                .unwrap_or_else(|| default_glyphs[i % default_glyphs.len()].to_string());
            self.type_icons.insert(t.name.clone(), icon);
            self.type_plurals.insert(t.name.clone(), t.plural.clone());
            self.doc_types.push(DocType::new(&t.name));
        }

        let (components, warnings) = StatusBarComponents::from_config(&config.ui.statusbar);
        self.status_bar_components = components;
        self.status_bar_warnings = warnings;
        self.status_bar_enabled = config.ui.statusbar.enabled;
        self.ascii_diagrams = config.ui.ascii_diagrams;
        self.rel_types = config.relationship_keywords();

        if self.selected_type >= self.doc_types.len() {
            self.selected_type = self.doc_types.len().saturating_sub(1);
        }
    }

    pub fn refresh_validation(&mut self, config: &Config) {
        let result = crate::engine::validation::validate_full(&self.store, config);
        self.validation_errors = result.errors.iter().map(|e| e.to_string()).collect();
        self.validation_warnings = result.warnings.iter().map(|e| e.to_string()).collect();
        self.validation_warnings
            .extend(self.status_bar_warnings.iter().cloned());
        self.filtered_docs_cache = None;
        self.rebuild_search_index();
    }

    pub fn cycle_mode(&mut self) {
        if self.view_mode == ViewMode::Filters {
            self.reset_filters();
        }
        self.view_mode = self.view_mode.next();
        if self.view_mode == ViewMode::Settings {
            self.settings_drill = None;
            self.settings_entry = 0;
        }
        if self.view_mode == ViewMode::Graph {
            self.rebuild_graph();
        }
        if self.view_mode == ViewMode::Filters {
            self.enter_filters_mode();
            self.selected_doc = 0;
        }
        #[cfg(feature = "agent")]
        if self.view_mode == ViewMode::Agents {
            if let Ok(records) = load_all_records(Some(self.agent_spawner.history_dir())) {
                self.agent_spawner.records = records;
            }
            self.agent_selected_index = 0;
        }
    }

    pub fn settings_categories() -> &'static [&'static str] {
        &[
            "General",
            "Document Types",
            "Relationships",
            "Validation Rules",
            "Numbering",
            "GitHub",
            "Coordination",
            "Certification",
            "Agents",
            "Interface",
        ]
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
        self.search_index = self
            .store
            .all_docs()
            .iter()
            .map(|doc| {
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
            })
            .collect();
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
                    Some(Status::Accepted) => Some(Status::InProgress),
                    Some(Status::InProgress) => Some(Status::Complete),
                    Some(Status::Complete) => Some(Status::Rejected),
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
                    Some(Status::Rejected) => Some(Status::Complete),
                    Some(Status::Complete) => Some(Status::InProgress),
                    Some(Status::InProgress) => Some(Status::Accepted),
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
        let forest = crate::engine::context::resolve_forest(&self.store);
        self.graph_nodes = flatten_forest(&forest, &self.store);
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

        if visible > SCROLL_PADDING
            && self.selected_doc >= self.doc_list_offset + visible - SCROLL_PADDING
        {
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
        let mut results: Vec<_> = self
            .search_index
            .iter()
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

    /// The relation tab's three sections for `doc`, derived from a single
    /// engine [`resolve_chain`](crate::engine::context::resolve_chain) so the
    /// navigable list and the rendered list share one source of truth.
    pub fn relation_sections(&self, doc: &DocMeta) -> RelationSections {
        let resolved = match crate::engine::context::resolve_chain(&self.store, &doc.id, 1) {
            Ok(r) => r,
            Err(_) => return RelationSections::default(),
        };

        let chain = resolved
            .nodes
            .iter()
            .map(|n| n.doc.path.clone())
            .filter(|p| *p != doc.path)
            .collect();
        let children = resolved
            .forward
            .iter()
            .map(|r| r.doc.path.clone())
            .collect();
        let related = resolved
            .related
            .iter()
            .map(|r| r.doc.path.clone())
            .collect();

        RelationSections {
            chain,
            children,
            related,
        }
    }

    pub fn relation_items(&self, doc: &DocMeta) -> Vec<PathBuf> {
        let sections = self.relation_sections(doc);
        let mut items = sections.chain;
        items.extend(sections.children);
        items.extend(sections.related);
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
                    let store = Store::load(&root, &config).map_err(|e| e.to_string())?;
                    let path = crate::cli::create::run(
                        &root,
                        &config,
                        &store,
                        &doc_type_str,
                        &title,
                        &author,
                        |p| {
                            let message = match &p {
                                ReservationProgress::QueryingRemote => {
                                    "Querying remote for latest tag...".to_string()
                                }
                                ReservationProgress::PushAttempt {
                                    attempt,
                                    max,
                                    candidate,
                                } => {
                                    format!(
                                        "Push attempt {}/{} for candidate {}...",
                                        attempt, max, candidate
                                    )
                                }
                                ReservationProgress::PushRejected { candidate } => {
                                    format!(
                                        "Push rejected for candidate {}, retrying...",
                                        candidate
                                    )
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
                        update_tags(&root, &relative, &tags, &thread_fs)
                            .map_err(|e| e.to_string())?;
                    }

                    if !relations.is_empty() {
                        let store = Store::load(&root, &config).map_err(|e| e.to_string())?;
                        for (rel_type, target_path) in &relations {
                            crate::cli::link::link_with_config(
                                &root,
                                &store,
                                &relative_str,
                                rel_type,
                                &target_path.to_string_lossy(),
                                &thread_fs,
                                Some(&config),
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

        let path = crate::cli::create::run(
            root,
            config,
            &self.store,
            &doc_type_str,
            &title,
            &author,
            |_| {},
        )?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let relative_str = relative.to_string_lossy().to_string();

        if !tags_str.is_empty() {
            let tags: Vec<String> = tags_str
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            update_tags(root, &relative, &tags, &*self.fs)?;
        }

        // Reload the store before applying relations so the new doc is resolvable
        let _ = self.store.reload_file(root, &relative, &*self.fs);

        // Apply relations
        for (rel_type, target_path) in &relations {
            crate::cli::link::link_with_config(
                root,
                &self.store,
                &relative_str,
                rel_type,
                &target_path.to_string_lossy(),
                &*self.fs,
                Some(config),
            )?;
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
        self.gh_issue_map_stale = true;
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

    pub fn confirm_delete(&mut self, root: &Path, config: &Config) -> Result<()> {
        let doc_path = self.delete_confirm.doc_path.clone();
        let doc_path_str = doc_path.to_string_lossy().to_string();
        crate::cli::delete::run_with_config(root, &self.store, &doc_path_str, Some(config))?;
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
            Status::InProgress => 3,
            Status::Complete => 4,
            Status::Rejected => 5,
            Status::Superseded => 6,
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

    pub fn confirm_status_change(&mut self, root: &Path, config: &Config) -> Result<()> {
        let status = match self.status_picker.selected {
            0 => Status::Draft,
            1 => Status::Review,
            2 => Status::Accepted,
            3 => Status::InProgress,
            4 => Status::Complete,
            5 => Status::Rejected,
            6 => Status::Superseded,
            _ => return Err(anyhow!("invalid status index")),
        };
        let doc_path = self.status_picker.doc_path.clone();
        let doc_path_str = doc_path.to_string_lossy().to_string();

        crate::cli::update::run_with_config(
            root,
            &self.store,
            &doc_path_str,
            &[("status", &status.to_string())],
            Some(config),
        )?;
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

    pub fn open_provenance_editor(&mut self) {
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

        self.provenance_editor.active = true;
        self.provenance_editor.doc_path = path;
        self.provenance_editor.input.clear();
        self.provenance_editor.error = None;
    }

    pub fn close_provenance_editor(&mut self) {
        self.provenance_editor.active = false;
        self.provenance_editor.doc_path = PathBuf::new();
        self.provenance_editor.input.clear();
        self.provenance_editor.error = None;
    }

    pub fn provenance_type_char(&mut self, c: char) {
        self.provenance_editor.input.push(c);
        self.provenance_editor.error = None;
    }

    pub fn provenance_backspace(&mut self) {
        self.provenance_editor.input.pop();
        self.provenance_editor.error = None;
    }

    pub fn submit_provenance(&mut self, root: &Path, config: &Config) -> Result<()> {
        let trimmed = self.provenance_editor.input.trim().to_string();
        if trimmed.is_empty() {
            self.provenance_editor.error = Some("citation must not be empty".into());
            return Ok(());
        }

        let doc_path = self.provenance_editor.doc_path.clone();
        let doc = match self
            .store
            .all_docs()
            .iter()
            .find(|d| d.path == doc_path)
            .copied()
            .cloned()
        {
            Some(d) => d,
            None => {
                self.provenance_editor.error = Some("document not found".into());
                return Ok(());
            }
        };

        if doc.provenance.iter().any(|c| c == &trimmed) {
            self.provenance_editor.error = Some("citation already present".into());
            return Ok(());
        }

        let type_name = doc.doc_type.as_str().to_string();
        let doc_id = doc.id.clone();
        let mut new_list = doc.provenance.clone();
        new_list.push(trimmed);

        if let Err(e) =
            crate::engine::provenance::set_provenance(root, config, &type_name, &doc_id, &new_list)
        {
            self.provenance_editor.error = Some(e.to_string());
            return Ok(());
        }

        self.store.reload_file(root, &doc_path, &*self.fs)?;
        self.filtered_docs_cache = None;
        self.rebuild_search_index();
        self.build_doc_tree();
        self.close_provenance_editor();
        Ok(())
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

    pub(crate) fn confirm_link(&mut self, root: &Path, config: &Config) -> Result<()> {
        let selected = self.link_editor.selected;
        let target_path = self.link_editor.results[selected].clone();
        let from = self.link_editor.doc_path.to_string_lossy().to_string();
        let to = target_path.to_string_lossy().to_string();
        let rel_type = self
            .rel_types
            .get(self.link_editor.rel_type_index)
            .map(|s| s.as_str())
            .unwrap_or("related-to")
            .to_string();

        let outcome = crate::cli::link::link_with_config(
            root,
            &self.store,
            &from,
            &rel_type,
            &to,
            &*self.fs,
            Some(config),
        )?;
        // Inverse keywords flip direction, so the modified file is the target,
        // not the viewed doc. Reload whichever file actually changed.
        self.store.reload_file(root, &outcome.source, &*self.fs)?;
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
        for entry in related_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let (rel_type, shorthand) = if let Some((prefix, id)) = entry.split_once(':') {
                let rel = crate::engine::document::RelationType::new(prefix.trim());
                (rel.to_string(), id.trim().to_string())
            } else {
                ("related-to".to_string(), entry.to_string())
            };

            let doc = self
                .store
                .resolve_shorthand(&shorthand)
                .map_err(|_| anyhow!("Cannot resolve: {}", shorthand))?;
            results.push((rel_type, doc.path.clone()));
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::TypeDef;
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

        #[cfg(feature = "agent")]
        let agent_spawner = AgentSpawner::new(store.root());

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
            provenance_editor: ProvenanceEditor::new(),
            #[cfg(feature = "agent")]
            agent_dialog: AgentDialog::new(),
            #[cfg(feature = "agent")]
            agent_spawner,
            #[cfg(feature = "agent")]
            agent_prompts: Vec::new(),
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
            wrap_mode: false,
            doc_tree: (0..doc_count).map(make_dummy_node).collect(),
            show_warnings: false,
            warnings_selected: 0,
            validation_errors: Vec::new(),
            validation_warnings: Vec::new(),
            status_bar_warnings: Vec::new(),
            fix_request: false,
            config_reload_request: false,
            fix_result: None,
            doc_list_offset: 0,
            doc_list_height: 0,
            fullscreen_height: 0,
            #[cfg(feature = "agent")]
            agent_selected_index: 0,
            #[cfg(feature = "agent")]
            resume_request: None,
            #[cfg(feature = "agent")]
            interactive_request: None,
            expanded_body_cache: HashMap::new(),
            expansion_in_flight: None,
            event_tx: tx,
            expansion_cancel: None,
            disk_cache: DiskCache::new(),
            terminal_image_protocol:
                crate::tui::infra::terminal_caps::TerminalImageProtocol::Unsupported,
            tool_availability: crate::tui::content::diagram::ToolAvailability { d2: false },
            diagram_cache: crate::tui::content::diagram::DiagramCache::new(),
            picker: ratatui_image::picker::Picker::halfblocks(),
            image_states: HashMap::new(),
            image_dimensions_cache: HashMap::new(),
            ascii_diagrams: false,
            diagram_blocks_cache: None,
            filtered_docs_cache: None,
            search_index: Vec::new(),
            git_branch: None,
            git_status_cache: GitStatusCache::new(Path::new(".")),
            gh_conflict_message: None,
            gh_push_in_flight: Arc::new(AtomicBool::new(false)),
            last_sync: None,
            gh_issue_map_stale: false,
            status_bar_enabled: true,
            status_bar_components: StatusBarComponents::default(),
            rel_types: Config::default().relationship_keywords(),
            settings_category: 0,
            settings_entry: 0,
            settings_drill: None,
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
        assert_eq!(
            app.doc_list_offset, 0,
            "selection at 7, still within viewport"
        );

        app.move_down();
        assert_eq!(app.selected_doc, 8);
        assert_eq!(
            app.doc_list_offset, 1,
            "viewport should scroll to maintain 2-row bottom padding"
        );
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
        assert_eq!(
            app.doc_list_offset, 5,
            "viewport stays put while selection is in interior"
        );

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
        assert_eq!(
            app.doc_list_offset, 5,
            "selection at padding boundary, offset still 5"
        );

        app.move_up();
        assert_eq!(app.selected_doc, 6);
        assert_eq!(
            app.doc_list_offset, 4,
            "crossed padding boundary, viewport adjusts"
        );
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
        assert_eq!(
            app.doc_list_offset, 0,
            "offset stays 0 when list fits in viewport"
        );
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
        assert_eq!(
            app.scroll_offset, 0,
            "modal should block Ctrl-D from reaching fullscreen"
        );
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
            provenance: vec![],
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
            provenance: vec![],
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
            app.validation_errors
                .iter()
                .any(|e| e.contains("duplicate id")),
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

    #[test]
    fn gh_conflict_blocks_other_keys() {
        let mut app = make_test_app(5);
        app.gh_conflict_message = Some("conflict detected".to_string());
        let root = std::path::PathBuf::from(".");
        let config = Config::default();

        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE, &root, &config);
        assert!(
            !app.should_quit,
            "quit should be blocked while conflict overlay is visible"
        );
        assert!(
            app.gh_conflict_message.is_some(),
            "conflict message should persist"
        );
    }

    #[test]
    fn gh_conflict_dismissed_by_esc() {
        let mut app = make_test_app(5);
        app.gh_conflict_message = Some("conflict detected".to_string());
        let root = std::path::PathBuf::from(".");
        let config = Config::default();

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE, &root, &config);
        assert!(
            app.gh_conflict_message.is_none(),
            "Esc should dismiss conflict overlay"
        );
    }

    #[test]
    fn gh_conflict_none_by_default() {
        let app = make_test_app(0);
        assert!(app.gh_conflict_message.is_none());
    }

    #[test]
    fn status_picker_navigates_all_seven_statuses() {
        let mut app = make_test_app(5);
        app.status_picker.active = true;
        app.status_picker.selected = 0;

        let root = PathBuf::from(".");
        let config = Config::default();

        for expected in 1..=6 {
            app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, &root, &config);
            assert_eq!(app.status_picker.selected, expected);
        }

        // should not exceed index 6
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.status_picker.selected, 6);

        // navigate back up to 0
        for expected in (0..=5).rev() {
            app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, &root, &config);
            assert_eq!(app.status_picker.selected, expected);
        }

        // should not go below 0
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.status_picker.selected, 0);
    }

    #[test]
    fn status_picker_esc_closes() {
        let mut app = make_test_app(5);
        app.status_picker.active = true;
        app.status_picker.selected = 3;

        let root = PathBuf::from(".");
        let config = Config::default();

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE, &root, &config);
        assert!(!app.status_picker.active);
        assert_eq!(app.status_picker.selected, 0);
    }

    #[test]
    fn open_status_picker_sets_index_from_doc_status() {
        use crate::engine::document::DocMeta;
        use chrono::NaiveDate;

        let mut app = make_test_app(1);
        let path = PathBuf::from("docs/rfcs/RFC-001.md");
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let statuses = [
            (Status::Draft, 0),
            (Status::Review, 1),
            (Status::Accepted, 2),
            (Status::InProgress, 3),
            (Status::Complete, 4),
            (Status::Rejected, 5),
            (Status::Superseded, 6),
        ];

        for (status, expected_index) in &statuses {
            app.store.docs.insert(
                path.clone(),
                DocMeta {
                    path: path.clone(),
                    title: "Test".to_string(),
                    doc_type: DocType::new("rfc"),
                    status: status.clone(),
                    id: "RFC-001".to_string(),
                    tags: Vec::new(),
                    provenance: Vec::new(),
                    author: String::new(),
                    date,
                    related: Vec::new(),
                    validate_ignore: false,
                    virtual_doc: false,
                },
            );
            app.doc_tree[0].path = path.clone();
            app.selected_doc = 0;

            app.open_status_picker();
            assert_eq!(
                app.status_picker.selected, *expected_index,
                "status {:?} should map to index {}",
                status, expected_index
            );
            app.close_status_picker();
        }
    }

    #[test]
    fn cache_refresh_sets_last_sync() {
        let mut app = make_test_app(0);
        assert!(app.last_sync.is_none());
        app.last_sync = Some(Instant::now());
        assert!(app.last_sync.is_some());
    }

    #[test]
    fn last_sync_initially_none() {
        let app = make_test_app(0);
        assert!(app.last_sync.is_none());
    }

    #[test]
    fn wrap_mode_defaults_to_off() {
        let app = make_test_app(0);
        assert!(!app.wrap_mode);
    }

    #[test]
    fn wrap_mode_survives_doc_tree_rebuild() {
        let mut app = make_test_app(3);
        app.wrap_mode = true;
        app.build_doc_tree();
        assert!(app.wrap_mode);
    }

    // --- relation_items / relation_sections --------------------------------

    fn relations_doc_md(title: &str, doc_type: &str, related: &str) -> String {
        let related_block = if related == "[]" {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\n{related_block}\n---\n\n{title} body\n"
        )
    }

    /// Build an `App` wrapping a real `Store` loaded from in-memory files under
    /// a fresh TempDir. The TempDir is returned so it outlives the store.
    fn app_with_store(files: &[(&str, &str)]) -> (tempfile::TempDir, App) {
        let tmp = tempfile::TempDir::new().unwrap();
        for (rel_path, contents) in files {
            let full = tmp.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        let store = Store::load(tmp.path(), &Config::default()).unwrap();
        let mut app = make_test_app(0);
        app.store = store;
        (tmp, app)
    }

    /// The `DocMeta` whose id matches, cloned out of the store.
    fn doc_by_id(app: &App, id: &str) -> DocMeta {
        app.store
            .docs
            .values()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("doc {id} not in store"))
            .clone()
    }

    fn ids_for_paths(app: &App, paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| app.store.get(p).unwrap().id.clone())
            .collect()
    }

    #[test]
    fn relation_items_lists_both_parents_of_multiparent_doc() {
        let (_tmp, app) = app_with_store(&[
            (
                "docs/rfcs/RFC-001-a.md",
                &relations_doc_md("A", "rfc", "[]"),
            ),
            (
                "docs/rfcs/RFC-002-b.md",
                &relations_doc_md("B", "rfc", "[]"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &relations_doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: RFC-001\n- implements: RFC-002",
                ),
            ),
        ]);

        let doc = doc_by_id(&app, "ITERATION-001");
        let items = app.relation_items(&doc);
        let ids: std::collections::BTreeSet<String> =
            ids_for_paths(&app, &items).into_iter().collect();

        assert!(
            ids.contains("RFC-001") && ids.contains("RFC-002"),
            "both parents must appear in the lineage, got {ids:?}"
        );
    }

    #[test]
    fn relation_items_order_is_chain_then_children_then_related_excluding_target() {
        // Lineage: RFC-001 <- STORY-001 <- ITERATION-001 (target).
        // Child of ITERATION-001: ITERATION-002 (implements it).
        // Related-to of ITERATION-001: RFC-002.
        let (_tmp, app) = app_with_store(&[
            (
                "docs/rfcs/RFC-001-base.md",
                &relations_doc_md("Base", "rfc", "[]"),
            ),
            (
                "docs/rfcs/RFC-002-side.md",
                &relations_doc_md("Side", "rfc", "[]"),
            ),
            (
                "docs/stories/STORY-001-mid.md",
                &relations_doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-target.md",
                &relations_doc_md(
                    "Target",
                    "iteration",
                    "- implements: STORY-001\n- related-to: RFC-002",
                ),
            ),
            (
                "docs/iterations/ITERATION-002-child.md",
                &relations_doc_md("Child", "iteration", "- implements: ITERATION-001"),
            ),
        ]);

        let doc = doc_by_id(&app, "ITERATION-001");
        let items = app.relation_items(&doc);
        let ids = ids_for_paths(&app, &items);

        assert_eq!(
            ids,
            vec!["RFC-001", "STORY-001", "ITERATION-002", "RFC-002"],
            "order must be chain (root-first, target excluded), then children, then related"
        );
        assert!(
            !ids.contains(&"ITERATION-001".to_string()),
            "target must be excluded from its own relation list"
        );
    }

    #[test]
    fn relation_items_related_set_matches_resolve_chain_related() {
        let (_tmp, app) = app_with_store(&[
            (
                "docs/rfcs/RFC-001-anchor.md",
                &relations_doc_md("Anchor", "rfc", "- related-to: RFC-002"),
            ),
            (
                "docs/rfcs/RFC-002-near.md",
                &relations_doc_md("Near", "rfc", "[]"),
            ),
            (
                "docs/rfcs/RFC-003-unrelated.md",
                &relations_doc_md("Unrelated", "rfc", "[]"),
            ),
        ]);

        let doc = doc_by_id(&app, "RFC-001");

        let resolved = crate::engine::context::resolve_chain(&app.store, &doc.id, 1).unwrap();
        let expected: std::collections::BTreeSet<PathBuf> = resolved
            .related
            .iter()
            .map(|r| r.doc.path.clone())
            .collect();

        let sections = app.relation_sections(&doc);
        let actual: std::collections::BTreeSet<PathBuf> = sections.related.into_iter().collect();

        assert_eq!(
            actual, expected,
            "tab related set must match engine resolve_chain related membership"
        );
        // And the related item is present in the flattened items.
        let item_ids: std::collections::BTreeSet<String> =
            ids_for_paths(&app, &app.relation_items(&doc))
                .into_iter()
                .collect();
        assert!(item_ids.contains("RFC-002"));
        assert!(!item_ids.contains("RFC-003"));
    }

    #[test]
    fn relation_items_empty_for_isolated_doc() {
        let (_tmp, app) = app_with_store(&[(
            "docs/rfcs/RFC-001-lonely.md",
            &relations_doc_md("Lonely", "rfc", "[]"),
        )]);

        let doc = doc_by_id(&app, "RFC-001");
        assert!(
            app.relation_items(&doc).is_empty(),
            "an isolated doc has no relations"
        );
    }

    fn config_with_types(types: &[&str]) -> Config {
        let mut config = Config::default();
        let template = config.documents.types[0].clone();
        config.documents.types = types
            .iter()
            .map(|name| TypeDef {
                name: name.to_string(),
                plural: format!("{name}s"),
                dir: format!("docs/{name}s"),
                prefix: name.to_uppercase(),
                ..template.clone()
            })
            .collect();
        config
    }

    #[test]
    fn apply_config_drops_type_and_clamps_selected_type() {
        let mut app = make_test_app(0);
        app.apply_config(&config_with_types(&["rfc", "story"]));
        app.selected_type = app.doc_types.len() - 1;

        app.apply_config(&config_with_types(&["rfc"]));

        let names: Vec<String> = app
            .doc_types
            .iter()
            .map(|t| t.as_str().to_string())
            .collect();
        assert_eq!(names, vec!["rfc"], "dropped type must be excluded");
        assert!(
            app.selected_type < app.doc_types.len(),
            "selected_type must stay in bounds after types shrink"
        );
        // current_type() indexes doc_types[selected_type] -- must not panic.
        assert_eq!(app.current_type().as_str(), "rfc");
    }

    #[test]
    fn apply_config_refreshes_rel_types() {
        use crate::engine::config::RelationshipDef;

        let mut app = make_test_app(0);
        let mut config = config_with_types(&["rfc"]);
        config.relationships = vec![RelationshipDef {
            name: "derives-from".to_string(),
            inverse: Some("derived-by".to_string()),
        }];

        app.apply_config(&config);

        assert_eq!(
            app.rel_types,
            vec!["derives-from".to_string(), "derived-by".to_string()],
            "rel_types must reflect the reloaded [[relationships]] (name then inverse)"
        );
    }

    #[test]
    fn apply_config_refreshes_icon_and_plural() {
        let mut app = make_test_app(0);

        let mut config = config_with_types(&["rfc"]);
        config.documents.types[0].icon = Some("@".to_string());
        config.documents.types[0].plural = "rfcen".to_string();
        app.apply_config(&config);

        assert_eq!(app.type_icons.get("rfc").map(String::as_str), Some("@"));
        assert_eq!(
            app.type_plurals.get("rfc").map(String::as_str),
            Some("rfcen")
        );
    }

    #[test]
    fn relation_sections_single_parent_common_case_unchanged() {
        let (_tmp, app) = app_with_store(&[
            (
                "docs/rfcs/RFC-001-base.md",
                &relations_doc_md("Base", "rfc", "[]"),
            ),
            (
                "docs/stories/STORY-001-leaf.md",
                &relations_doc_md("Leaf", "story", "- implements: RFC-001"),
            ),
        ]);

        let doc = doc_by_id(&app, "STORY-001");
        let sections = app.relation_sections(&doc);

        assert_eq!(ids_for_paths(&app, &sections.chain), vec!["RFC-001"]);
        assert!(sections.children.is_empty());
        assert!(sections.related.is_empty());
    }
}
