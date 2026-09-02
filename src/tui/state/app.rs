#[cfg(feature = "agent")]
use super::forms::AgentDialog;
use super::forms::{
    CreateForm, DeleteConfirm, EdgeKey, EditableField, FieldEditor, FieldPath, LinkEditor,
    OpenRequest, OverrideKeyPrompt, ProvenanceEditor, RelKey, SettingsDeleteConfirm,
    SettingsDeleteTarget, SettingsImpactConfirm, SettingsQuitPrompt, SettingsVariantPicker,
    StatusPicker, TypeKey, ZoneOrderingEditor,
};
use super::graph::flatten_forest;
use super::settings_guard;
pub use crate::engine::graph::GraphNode;

use crate::engine::cache::DiskCache;
use crate::engine::config::{
    default_normalize, CertificationOverride, Config, EdgeDef, GithubConfig, NumberingStrategy,
    RelSelector, RelationshipDef, ReservedConfig, ReservedFormat, Severity, SqidsConfig,
    StoreBackend, Traversal, TypeDef, TypeSelector, WILDCARD,
};
use crate::engine::document::{rewrite_frontmatter, DocMeta, DocType, Status};
use crate::engine::fs::FileSystem;
use crate::engine::git_status::{query_git_branch, GitStatusCache};
use crate::engine::ops::open::{resolve_open_target, OpenTarget};
use crate::engine::reservation::ReservationProgress;
use crate::engine::store::{Filter, Store};
#[cfg(feature = "agent")]
use crate::tui::agent::{load_all_records, AgentSpawner};
use crate::tui::views::keybinds::KeyContext;
use crate::tui::views::panels::UNSET_VARIANT;
use crate::tui::views::status_bar::StatusBarComponents;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

/// A typed value destined for a settings buffer field, dispatched by
/// `App::settings_write`'s exhaustive `FieldPath` match. Carries the field's
/// real type so the writer never re-parses.
enum SettingsValue {
    Text(String),
    OptText(Option<String>),
    List(Vec<String>),
    Bool(bool),
    Num(u64),
    Numbering(NumberingStrategy),
    Store(StoreBackend),
    ReservedFormat(ReservedFormat),
    /// An edge's `required`, whose absence states no requiredness at all
    /// (RFC-067) -- so the carrier is the `Option`, not the severity.
    OptSeverity(Option<Severity>),
    /// An edge's `traversal`, absent when the row names no walk role (ADR-030).
    OptTraversal(Option<Traversal>),
}

/// Parse and bounds-check a numeric settings input. Pure and unit-testable.
fn validate_bounded(input: &str, min: u64, max: u64) -> Result<u64, String> {
    let n: u64 = input
        .parse()
        .map_err(|_| format!("'{}' is not a number", input))?;
    if n < min || n > max {
        return Err(format!("value must be between {} and {}", min, max));
    }
    Ok(n)
}

/// An optional config section a just-cycled enum value depends on: `numbering =
/// sqids` needs `[numbering.sqids]`, `numbering = reserved` needs
/// `[numbering.reserved]`, `store = github-issues` needs `[github]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigDep {
    NumberingSqids,
    NumberingReserved,
    Github,
}

/// What `scaffold_dependency` did: the section it inserted, plus the first
/// required-but-empty field that scaffolding produced (only the sqids salt; the
/// other sections scaffold complete with parser defaults).
#[derive(Debug, Clone, PartialEq)]
pub struct ScaffoldResult {
    pub inserted: ConfigDep,
    pub required_empty_field: Option<FieldPath>,
}

/// Insert the optional config section `dep` requires into `buffer` with
/// parser-matching defaults, or skip if it already exists. Returns `Some` with a
/// record of the insert when a section was added, or `None` when the section was
/// already present (the buffer is left untouched). Pure: operates only on the
/// passed buffer. The scaffolded section is byte-identical to what `Config::parse`
/// would have produced for an empty section, so a later save round-trips cleanly.
pub fn scaffold_dependency(buffer: &mut Config, dep: ConfigDep) -> Option<ScaffoldResult> {
    match dep {
        ConfigDep::NumberingSqids => {
            if buffer.documents.sqids.is_some() {
                return None;
            }
            buffer.documents.sqids = Some(SqidsConfig {
                salt: String::new(),
                min_length: 3,
            });
            Some(ScaffoldResult {
                inserted: ConfigDep::NumberingSqids,
                required_empty_field: Some(FieldPath::SqidsSalt),
            })
        }
        ConfigDep::NumberingReserved => {
            if buffer.documents.reserved.is_some() {
                return None;
            }
            buffer.documents.reserved = Some(ReservedConfig {
                remote: "origin".to_string(),
                format: ReservedFormat::Incremental,
                max_retries: 5,
            });
            Some(ScaffoldResult {
                inserted: ConfigDep::NumberingReserved,
                required_empty_field: None,
            })
        }
        ConfigDep::Github => {
            if buffer.documents.github.is_some() {
                return None;
            }
            buffer.documents.github = Some(GithubConfig {
                repo: None,
                cache_ttl: 60,
            });
            Some(ScaffoldResult {
                inserted: ConfigDep::Github,
                required_empty_field: None,
            })
        }
    }
}

fn numbering_variant(n: &NumberingStrategy) -> &'static str {
    match n {
        NumberingStrategy::Incremental => "incremental",
        NumberingStrategy::Sqids => "sqids",
        NumberingStrategy::Reserved => "reserved",
    }
}

fn numbering_from_variant(v: &str) -> Option<NumberingStrategy> {
    match v {
        "incremental" => Some(NumberingStrategy::Incremental),
        "sqids" => Some(NumberingStrategy::Sqids),
        "reserved" => Some(NumberingStrategy::Reserved),
        _ => None,
    }
}

fn store_from_variant(v: &str) -> Option<StoreBackend> {
    match v {
        "filesystem" => Some(StoreBackend::Filesystem),
        "github-issues" => Some(StoreBackend::GithubIssues),
        "github-milestones" => Some(StoreBackend::GithubMilestones),
        "github-projects" => Some(StoreBackend::GithubProjects),
        "git-ref" => Some(StoreBackend::GitRef),
        _ => None,
    }
}

fn severity_from_variant(v: &str) -> Option<Severity> {
    match v {
        "error" => Some(Severity::Error),
        "warning" => Some(Severity::Warning),
        _ => None,
    }
}

fn traversal_from_variant(v: &str) -> Option<Traversal> {
    match v {
        "chain" => Some(Traversal::Chain),
        "related" => Some(Traversal::Related),
        _ => None,
    }
}

/// Parse a variant drawn from an optional field's list, whose first entry is
/// `UNSET_VARIANT`. `Some(None)` is the unset entry, which clears the key; the
/// outer `None` is an unrecognised variant, which writes nothing at all --
/// preserving `settings_set_enum_variant`'s no-op on a variant it cannot parse.
fn optional_variant<T>(variant: &str, parse: fn(&str) -> Option<T>) -> Option<Option<T>> {
    if variant == UNSET_VARIANT {
        return Some(None);
    }
    parse(variant).map(Some)
}

/// The comma editor's seed for an edge type position: the wildcard as the `*`
/// its author wrote, a set as its bare names. Unlike `TypeSelector::spelling`
/// a multi-name set carries no brackets, because `FieldEditor::List` splits the
/// seed on commas and would keep them inside the first and last name.
fn type_position_raw(selector: &TypeSelector) -> String {
    match selector {
        TypeSelector::Any => WILDCARD.to_string(),
        TypeSelector::Types(names) => names.join(", "),
    }
}

/// [`type_position_raw`] for the `via` position, a relationship set on the same
/// terms (ADR-032).
fn rel_position_raw(selector: &RelSelector) -> String {
    match selector {
        RelSelector::Any => WILDCARD.to_string(),
        RelSelector::Named(names) => names.join(", "),
    }
}

/// A comma-editor commit for an edge type position. `["*"]` is the wildcard and
/// not a type named `*`: the editor seeds from the position's own spelling, so
/// confirming an untouched wildcard has to give the wildcard back.
fn type_selector_from(names: Vec<String>) -> TypeSelector {
    if names == [WILDCARD] {
        return TypeSelector::Any;
    }
    TypeSelector::Types(names)
}

/// [`type_selector_from`] for `via`. A cycler over the declared relationship
/// names would keep the user inside the set strict load accepts, but `via` is a
/// disjunction over its members (ADR-032) and a single-position cycler cannot
/// spell one: it would silently narrow `via = ["a", "b"]` to one name on the
/// next press. So `via` shares the comma editor its two neighbours use, and
/// `EnumCycle`'s `&'static` variant carrier is left alone.
fn rel_selector_from(names: Vec<String>) -> RelSelector {
    if names == [WILDCARD] {
        return RelSelector::Any;
    }
    RelSelector::Named(names)
}

/// An edge selector position that names nothing matches nothing, and the loader
/// does not catch it: its declared-name checks iterate `names()`, and an empty
/// list iterates nothing. So the panel refuses the empty set at commit rather
/// than reading it as `*`, which is a different claim the user did not make.
/// ITERATION-391 replaces this editor for `from`/`to` and has to carry the same
/// refusal.
fn empty_edge_position_refusal(path: &FieldPath, names: &[String]) -> Option<String> {
    if !names.is_empty() {
        return None;
    }
    let (position, kind) = match path {
        FieldPath::Edge {
            key: EdgeKey::From, ..
        } => ("from", "type"),
        FieldPath::Edge {
            key: EdgeKey::To, ..
        } => ("to", "type"),
        FieldPath::Edge {
            key: EdgeKey::Via, ..
        } => ("via", "relationship"),
        _ => return None,
    };
    Some(format!("`{position}` must name a {kind}, or `*` for any"))
}

fn reserved_format_variant(f: &ReservedFormat) -> &'static str {
    match f {
        ReservedFormat::Incremental => "incremental",
        ReservedFormat::Sqids => "sqids",
    }
}

fn reserved_format_from_variant(v: &str) -> Option<ReservedFormat> {
    match v {
        "incremental" => Some(ReservedFormat::Incremental),
        "sqids" => Some(ReservedFormat::Sqids),
        _ => None,
    }
}

fn statusbar_raw(slot: Option<&Vec<String>>) -> String {
    slot.map(|v| v.join(", ")).unwrap_or_default()
}

pub struct CreateResult {
    pub path: PathBuf,
    pub doc_type: DocType,
}

/// One keystroke's worth of work for the background search worker. Carries its
/// own corpus snapshot so freshness is trivial: every request searches the
/// store exactly as it stood when the key was handled, and the worker never
/// borrows `Store` (which lives on the UI thread).
pub struct SearchRequest {
    pub corpus: crate::engine::store::SearchCorpus,
    pub query: String,
    pub generation: u64,
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
        state: crate::spinners::SpinnerState,
    },
    CreateComplete {
        result: Result<CreateResult, String>,
    },
    SearchResults {
        generation: u64,
        results: Vec<PathBuf>,
    },
    CacheRefresh {
        warnings: Vec<String>,
    },
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
        if !e.trim().is_empty() {
            return e.to_string();
        }
    }
    if let Some(v) = visual {
        if !v.trim().is_empty() {
            return v.to_string();
        }
    }
    "vi".to_string()
}

/// Resolve `$EDITOR`/`$VISUAL` into a command vector, splitting on whitespace so
/// `EDITOR="code --wait"` spawns `code` with `--wait` rather than a binary named
/// literally `code --wait`. Mirrors the viewer split in [`App::plan_open`]. The
/// fallback (`vi`) guarantees a non-empty result.
pub fn resolve_editor_command_from(editor: Option<&str>, visual: Option<&str>) -> Vec<String> {
    resolve_editor_from(editor, visual)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

pub fn resolve_editor_command() -> Vec<String> {
    resolve_editor_command_from(
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

/// The graph view's pivot selection: the whole-store forest, a forest re-rooted
/// on a document type, or one re-rooted on a tag. Indices point into
/// `App::doc_types` / `App::available_tags`. The sidebar renders these in the
/// flat order All, types…, tags…; [`anchor_to_flat`]/[`flat_to_anchor`] convert
/// between an anchor and its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphAnchor {
    All,
    Type(usize),
    Tag(usize),
}

/// The sidebar row index for an anchor, given the type count: All is 0, types
/// occupy `1..=nt`, tags follow at `nt + 1`.
pub fn anchor_to_flat(anchor: GraphAnchor, nt: usize) -> usize {
    match anchor {
        GraphAnchor::All => 0,
        GraphAnchor::Type(i) => 1 + i,
        GraphAnchor::Tag(i) => 1 + nt + i,
    }
}

/// The anchor for a sidebar row index, clamping a tag index into range.
pub(crate) fn flat_to_anchor(flat: usize, nt: usize, ntags: usize) -> GraphAnchor {
    if flat == 0 {
        GraphAnchor::All
    } else if flat <= nt {
        GraphAnchor::Type(flat - 1)
    } else {
        GraphAnchor::Tag((flat - 1 - nt).min(ntags.saturating_sub(1)))
    }
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

/// Clamp a list viewport's top-row `offset` so `selected` stays visible with
/// `SCROLL_PADDING` rows of margin above and below. Shared by the documents
/// table and the graph table (DICTUM-006: two concrete uses). When the viewport
/// is unmeasured or the list is empty the offset is left unchanged, matching the
/// pre-extraction `adjust_viewport` early return.
pub(crate) fn clamp_viewport_offset(
    selected: usize,
    mut offset: usize,
    visible: usize,
    count: usize,
) -> usize {
    if visible == 0 || count == 0 {
        return offset;
    }
    if selected < offset + SCROLL_PADDING {
        offset = selected.saturating_sub(SCROLL_PADDING);
    }
    if visible > SCROLL_PADDING && selected >= offset + visible - SCROLL_PADDING {
        offset = selected + SCROLL_PADDING + 1 - visible;
    }
    offset.min(count.saturating_sub(visible))
}

pub struct App {
    pub fs: Box<dyn FileSystem>,
    pub store: Store,
    pub selected_type: usize,
    pub selected_doc: usize,
    pub doc_types: Vec<DocType>,
    /// Graph-view pivot anchor: the whole-store forest, a type, or a tag. The
    /// TUI only selects the anchor; the re-rooting lives in `resolve_forest` /
    /// `resolve_forest_by_tag` (engine).
    pub graph_anchor: GraphAnchor,
    /// Active graph-view sibling sort column id (ITERATION-209): `path` (topo
    /// identity), `status`, or a declared attribute name. Seeded from
    /// `tui.graph.sort`. Cycled by `o`; presentation-only and sibling-scoped.
    pub graph_sort_col: String,
    /// Whether the active graph sort is reversed (toggled by `O`). Missing
    /// attribute values still sort last regardless.
    pub graph_sort_rev: bool,
    pub should_quit: bool,
    pub fullscreen_doc: bool,
    pub scroll_offset: u16,
    pub search_mode: bool,
    pub search_query: String,
    pub search_results: Vec<std::path::PathBuf>,
    pub search_selected: usize,
    /// True while a search request is in flight on the worker; the overlay
    /// renders a spinner until the matching-generation results land.
    pub search_pending: bool,
    /// Monotonic id stamped onto each dispatched search; results carrying an
    /// older generation are dropped so a slow search never overwrites a newer
    /// query's results.
    pub search_generation: u64,
    /// Sender to the background search worker (BUG-011). Like `event_tx`, a
    /// throwaway channel at construction; the event loop rebinds it when it
    /// spawns the worker, so state code just sends and ignores errors.
    pub search_tx: crossbeam_channel::Sender<SearchRequest>,
    pub show_help: bool,
    pub help_scroll: u16,
    /// Maximum legal `help_scroll` for the current help content + viewport,
    /// written by `draw_help_overlay` each frame (render-feeds-state, like
    /// `doc_list_height`/`fullscreen_height`). 0 when the content fits.
    pub help_max_scroll: u16,
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
    /// Top row of the graph viewport and its visible height, mirroring
    /// `doc_list_offset`/`doc_list_height` so the graph table scrolls with the
    /// same scrolloff padding via [`clamp_viewport_offset`]. `graph_list_height`
    /// is render-fed each frame by `draw_graph`.
    pub graph_offset: usize,
    pub graph_list_height: usize,
    pub editor_request: Option<PathBuf>,
    pub open_request: Option<OpenRequest>,
    pub open_message: Option<String>,
    pub filter_focused: FilterField,
    pub filter_status: Option<Status>,
    pub filter_tag: Option<String>,
    pub available_tags: Vec<String>,
    /// The union of every configured type's lifecycle states, in first-seen
    /// order. Drives the status filter cycle so custom DAGs are reachable.
    pub available_statuses: Vec<String>,
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
    pub gh_fetch_warnings: Vec<String>,
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
    pub git_branch: Option<String>,
    pub git_status_cache: GitStatusCache,
    pub gh_conflict_message: Option<String>,
    pub gh_push_in_flight: Arc<AtomicBool>,
    /// Mirror of the event loop's local `refresh_in_flight` atomic, refreshed
    /// each frame so the header sync face can reflect an in-flight poll.
    pub refresh_in_flight: bool,
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
    /// Field cursor within the current settings field-view (0-based). Reset to 0
    /// on category change. Unused by entry-list views (those track `settings_entry`).
    pub settings_field: usize,
    /// The in-memory edit buffer the settings view renders from, so future edits
    /// show immediately. Seeded from the session config; while clean it tracks
    /// config reloads (see `apply_config`).
    pub settings_buffer: Config,
    pub settings_dirty: bool,
    pub settings_editing: bool,
    pub settings_edit_input: String,
    pub settings_edit_error: Option<String>,
    pub settings_footer_error: Option<String>,
    /// The save/discard/cancel prompt shown when quitting Settings with unsaved
    /// buffer edits (AC10).
    pub settings_quit_prompt: SettingsQuitPrompt,
    /// A pending dependency-scaffold offer raised by `settings_cycle_enum` when a
    /// just-cycled enum value (numbering = sqids/reserved, store = github-issues)
    /// auto-inserts its required config section. When the section carries a
    /// required-but-empty field (only the sqids salt), the offer prompts a jump to
    /// fill it; any non-accept key dismisses it (the buffer-state flag persists).
    pub settings_scaffold_offer: Option<ScaffoldResult>,
    /// The confirm prompt for removing a settings collection entry from the
    /// buffer. Removal is buffer-only and happens only on confirm (no disk).
    pub settings_delete_confirm: SettingsDeleteConfirm,
    /// The confirm prompt raised before a save that orphans existing documents by
    /// altering a load-bearing `[[types]]` field (`dir`/`prefix`/`store`) on a type
    /// with docs on disk (RFC-023 slice 6). When active, the save is paused: nothing
    /// is written until the user confirms.
    pub settings_impact_confirm: SettingsImpactConfirm,
    /// The spec-path key prompt for seeding a new certification override. The key
    /// is entered before the override is inserted into the buffer.
    pub override_key_prompt: OverrideKeyPrompt,
    /// The two-pane status-bar zone ordering editor, active while a
    /// `statusbar.left/center/right` row is being reordered. `Some` routes keys to
    /// the zone editor instead of the field list; commit writes the chosen order
    /// back into `settings_buffer` (RFC-023 slice 7).
    pub settings_zone_editor: Option<ZoneOrderingEditor>,
    /// The enum variant-picker overlay, active while an enum field (numbering,
    /// store, or reserved format) is being edited via `Enter`. `Some` routes keys
    /// to the picker; selecting writes the chosen variant back into
    /// `settings_buffer` (RFC-023 / STORY-144).
    pub settings_variant_picker: Option<SettingsVariantPicker>,
    /// Free-running render-loop counter, set each frame before `terminal.draw`.
    /// Drives spinner animation phase; shared with the header sync/push face.
    pub frame_idx: u64,
}

impl App {
    pub fn new(
        store: Store,
        config: &Config,
        picker: ratatui_image::picker::Picker,
        fs: Box<dyn FileSystem>,
    ) -> Self {
        let (event_tx, _event_rx) = crossbeam_channel::unbounded();
        let (search_tx, _search_rx) = crossbeam_channel::unbounded();
        let git_branch = query_git_branch(store.root());
        let git_status_cache = GitStatusCache::new(store.root());
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
            graph_anchor: GraphAnchor::All,
            graph_sort_col: config.ui.graph.sort.clone(),
            graph_sort_rev: false,
            should_quit: false,
            fullscreen_doc: false,
            scroll_offset: 0,
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_pending: false,
            search_generation: 0,
            search_tx,
            show_help: false,
            help_scroll: 0,
            help_max_scroll: 0,
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
            graph_offset: 0,
            graph_list_height: 0,
            editor_request: None,
            open_request: None,
            open_message: None,
            filter_focused: FilterField::Status,
            filter_status: None,
            filter_tag: None,
            available_tags: Vec::new(),
            available_statuses: Vec::new(),
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
            gh_fetch_warnings: Vec::new(),
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
            git_branch,
            git_status_cache,
            gh_conflict_message: None,
            gh_push_in_flight: Arc::new(AtomicBool::new(false)),
            refresh_in_flight: false,
            last_sync: if crate::tui::has_pollable_types(config) {
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
            settings_field: 0,
            settings_buffer: config.clone(),
            settings_dirty: false,
            settings_editing: false,
            settings_edit_input: String::new(),
            settings_edit_error: None,
            settings_footer_error: None,
            settings_quit_prompt: SettingsQuitPrompt::new(),
            settings_scaffold_offer: None,
            settings_delete_confirm: SettingsDeleteConfirm::new(),
            settings_impact_confirm: SettingsImpactConfirm::new(),
            override_key_prompt: OverrideKeyPrompt::new(),
            settings_zone_editor: None,
            settings_variant_picker: None,
            frame_idx: 0,
        };
        app.apply_config(config);
        app.refresh_available_tags();
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

        self.available_statuses.clear();
        for t in &config.documents.types {
            for state in &t.effective_lifecycle().states {
                if !self.available_statuses.contains(state) {
                    self.available_statuses.push(state.clone());
                }
            }
        }

        let (components, warnings) = StatusBarComponents::from_config(&config.ui.statusbar);
        self.status_bar_components = components;
        self.status_bar_warnings = warnings;
        self.status_bar_enabled = config.ui.statusbar.enabled;
        self.ascii_diagrams = config.ui.ascii_diagrams;
        self.rel_types = config.relationship_keywords();

        // A clean buffer follows external/session config reloads; a dirty buffer
        // (pending edits) is preserved.
        if !self.settings_dirty {
            self.settings_buffer = config.clone();
        }

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
        self.validation_warnings
            .extend(self.gh_fetch_warnings.iter().cloned());
        self.filtered_docs_cache = None;
    }

    pub fn cycle_mode(&mut self) {
        if self.view_mode == ViewMode::Filters {
            self.reset_filters();
        }
        self.view_mode = self.view_mode.next();
        if self.view_mode == ViewMode::Settings {
            self.settings_drill = None;
            self.settings_entry = 0;
            self.settings_field = 0;
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

    /// The [`KeyContext`] whose handler is currently live, mirroring
    /// `handle_key`'s precedence ladder exactly (`keys.rs:19-66`) plus the
    /// `handle_normal_key` view-mode dispatch (`keys.rs:1002-1009`) and the
    /// nested `handle_settings_key` sub-state order (`keys.rs:744-885`).
    ///
    /// The `show_help` short-circuit (`keys.rs:25-28`) is intentionally skipped:
    /// help is an overlay drawn *on top of* a context, so we resolve the context
    /// underneath it. Keeping this match structurally identical to the handlers
    /// is the drift guard the help renderer (T3) and parity test (T5) rely on.
    pub fn active_key_context(&self) -> KeyContext {
        if self.gh_conflict_message.is_some() {
            return KeyContext::GhConflict;
        }
        // NB: the `show_help` check at keys.rs:25 is skipped on purpose.
        if self.show_warnings {
            return KeyContext::Warnings;
        }
        if self.create_form.active {
            return KeyContext::CreateForm;
        }
        if self.delete_confirm.active {
            return KeyContext::DeleteConfirm;
        }
        if self.override_key_prompt.active {
            return KeyContext::OverrideKeyPrompt;
        }
        if self.settings_delete_confirm.active {
            return KeyContext::SettingsDeleteConfirm;
        }
        if self.settings_impact_confirm.active {
            return KeyContext::SettingsImpact;
        }
        if self.status_picker.active {
            return KeyContext::StatusPicker;
        }
        if self.link_editor.active {
            return KeyContext::LinkEditor;
        }
        if self.provenance_editor.active {
            return KeyContext::ProvenanceEditor;
        }
        #[cfg(feature = "agent")]
        if self.agent_dialog.active {
            return if self.agent_dialog.text_input.is_some() {
                KeyContext::AgentTextInput
            } else {
                KeyContext::AgentDialog
            };
        }
        if self.search_mode {
            return KeyContext::Search;
        }
        if self.fullscreen_doc {
            return KeyContext::Fullscreen;
        }
        match self.view_mode {
            ViewMode::Filters => KeyContext::Filters,
            ViewMode::Graph => KeyContext::Graph,
            ViewMode::Settings => self.active_settings_key_context(),
            #[cfg(feature = "agent")]
            ViewMode::Agents => KeyContext::Agents,
            _ => KeyContext::Types,
        }
    }

    /// The settings sub-state context, resolved in the same precedence
    /// `handle_settings_key` checks (`keys.rs:744-885`).
    fn active_settings_key_context(&self) -> KeyContext {
        if self.settings_quit_prompt.active {
            KeyContext::SettingsQuitPrompt
        } else if self.settings_editing {
            KeyContext::SettingsEditing
        } else if self.settings_zone_editor.is_some() {
            KeyContext::SettingsZoneEditor
        } else if self.settings_variant_picker.is_some() {
            KeyContext::SettingsVariantPicker
        } else if self.settings_scaffold_offer.is_some() {
            KeyContext::SettingsScaffoldOffer
        } else {
            KeyContext::Settings
        }
    }

    pub fn settings_categories() -> &'static [&'static str] {
        &[
            "General",
            "Document Types",
            "Relationships",
            "Edges",
            "Numbering",
            "GitHub",
            "Certification",
            "Agents",
            "Interface",
        ]
    }

    /// The nav index of the category called `name`. Every jump names the
    /// category it means and resolves it here, because an index literal does
    /// not fail when a category is inserted above it -- it silently addresses
    /// the neighbour (STORY-260 put `Edges` at index 3 and moved five
    /// categories down one). The names are literals in this module, so a
    /// lookup that misses is a typo, not a state a user can reach.
    pub(crate) fn settings_category_index(name: &str) -> usize {
        Self::settings_categories()
            .iter()
            .position(|cat| *cat == name)
            .unwrap_or_else(|| panic!("no settings category named {name}"))
    }

    /// The `EditableField` under the field cursor in the current settings
    /// field-view, read from `settings_buffer`. None for entry-list views or an
    /// out-of-range cursor.
    pub fn settings_focused_field(&self) -> Option<EditableField> {
        let fields = crate::tui::views::panels::settings_fields(
            self.settings_category,
            self.settings_entry,
            self.settings_drill,
            &self.settings_buffer,
        );
        fields.get(self.settings_field).cloned()
    }

    /// The current RAW editable string for the focused field, used to seed the
    /// edit input. Unlike the rendered `value`, an unset Nullable yields `""`
    /// (not `(unset)`) and a List yields a comma-joined string. Derived directly
    /// from the buffer via the focused `FieldPath`.
    pub(crate) fn settings_focused_raw(&self) -> String {
        let Some(focused) = self.settings_focused_field() else {
            return String::new();
        };
        let buf = &self.settings_buffer;
        match &focused.path {
            FieldPath::Naming => buf.documents.naming.pattern.clone(),
            FieldPath::RefCountCeiling => buf.ref_count_ceiling.to_string(),
            FieldPath::TemplatesDir => buf.filesystem.templates.dir.clone(),
            FieldPath::Type { index, key } => buf
                .documents
                .types
                .get(*index)
                .map(|t| match key {
                    TypeKey::Name => t.name.clone(),
                    TypeKey::Plural => t.plural.clone(),
                    TypeKey::Dir => t.dir.clone(),
                    TypeKey::Prefix => t.prefix.clone(),
                    TypeKey::Icon => t.icon.clone().unwrap_or_default(),
                    TypeKey::Numbering => numbering_variant(&t.numbering).to_string(),
                    TypeKey::Subdirectory => t.subdirectory.to_string(),
                    TypeKey::Store => t.store.to_string(),
                    TypeKey::Singleton => t.singleton.to_string(),
                    TypeKey::ParentType => t.parent_type.clone().unwrap_or_default(),
                    TypeKey::Agents => t.agents.join(", "),
                })
                .unwrap_or_default(),
            FieldPath::Rel { index, key } => buf
                .relationships
                .get(*index)
                .map(|r| match key {
                    RelKey::Name => r.name.clone(),
                    RelKey::Inverse => r.inverse.clone().unwrap_or_default(),
                })
                .unwrap_or_default(),
            FieldPath::Edge { index, key } => buf
                .edges
                .get(*index)
                .map(|e| match key {
                    EdgeKey::Name => e.name.clone(),
                    EdgeKey::From => type_position_raw(&e.from),
                    EdgeKey::To => type_position_raw(&e.to),
                    EdgeKey::Via => rel_position_raw(&e.via),
                    // An optional enum's raw string has to be a member of its
                    // variant list, so absence reads back as the unset entry
                    // the cycler indexes -- not as the empty string a Nullable
                    // text field would want.
                    EdgeKey::Required => e
                        .required
                        .as_ref()
                        .map(Severity::as_str)
                        .unwrap_or(UNSET_VARIANT)
                        .to_string(),
                    EdgeKey::Traversal => e
                        .traversal
                        .as_ref()
                        .map(Traversal::as_str)
                        .unwrap_or(UNSET_VARIANT)
                        .to_string(),
                })
                .unwrap_or_default(),
            FieldPath::SqidsSalt => buf
                .documents
                .sqids
                .as_ref()
                .map(|s| s.salt.clone())
                .unwrap_or_default(),
            FieldPath::SqidsMinLength => buf
                .documents
                .sqids
                .as_ref()
                .map(|s| s.min_length.to_string())
                .unwrap_or_default(),
            FieldPath::ReservedRemote => buf
                .documents
                .reserved
                .as_ref()
                .map(|r| r.remote.clone())
                .unwrap_or_default(),
            FieldPath::ReservedFormat => buf
                .documents
                .reserved
                .as_ref()
                .map(|r| reserved_format_variant(&r.format).to_string())
                .unwrap_or_default(),
            FieldPath::ReservedMaxRetries => buf
                .documents
                .reserved
                .as_ref()
                .map(|r| r.max_retries.to_string())
                .unwrap_or_default(),
            FieldPath::GithubRepo => buf
                .documents
                .github
                .as_ref()
                .and_then(|g| g.repo.clone())
                .unwrap_or_default(),
            FieldPath::GithubCacheTtl => buf
                .documents
                .github
                .as_ref()
                .map(|g| g.cache_ttl.to_string())
                .unwrap_or_default(),
            FieldPath::CertNormalize => buf.certification.normalize.to_string(),
            FieldPath::CertOverride { key } => buf
                .certification
                .overrides
                .get(key)
                .map(|o| o.normalize.to_string())
                .unwrap_or_default(),
            FieldPath::AgentsInteractive => buf.agents.interactive.clone().unwrap_or_default(),
            FieldPath::UiAsciiDiagrams => buf.ui.ascii_diagrams.to_string(),
            FieldPath::StatusbarEnabled => buf.ui.statusbar.enabled.to_string(),
            FieldPath::StatusbarLeft => statusbar_raw(buf.ui.statusbar.left.as_ref()),
            FieldPath::StatusbarCenter => statusbar_raw(buf.ui.statusbar.center.as_ref()),
            FieldPath::StatusbarRight => statusbar_raw(buf.ui.statusbar.right.as_ref()),
            FieldPath::MultilineMaxExpandedHeight => {
                buf.ui.multiline.max_expanded_height.to_string()
            }
            FieldPath::Unset => String::new(),
        }
    }

    /// Write a typed value into `settings_buffer` at the given `FieldPath`. The
    /// single, exhaustive read/write site for settings edits (Principle 6). The
    /// `SettingsValue` variant must match the field's editor kind; mismatches and
    /// `Unset`/`ReadOnly` paths are no-ops.
    fn settings_write(&mut self, path: &FieldPath, value: SettingsValue) {
        let buf = &mut self.settings_buffer;
        match path {
            FieldPath::Naming => {
                if let SettingsValue::Text(s) = value {
                    buf.documents.naming.pattern = s;
                }
            }
            FieldPath::RefCountCeiling => {
                if let SettingsValue::Num(n) = value {
                    buf.ref_count_ceiling = n as usize;
                }
            }
            FieldPath::TemplatesDir => {
                if let SettingsValue::Text(s) = value {
                    buf.filesystem.templates.dir = s;
                }
            }
            FieldPath::Type { index, key } => {
                if let Some(t) = buf.documents.types.get_mut(*index) {
                    match (key, value) {
                        (TypeKey::Name, SettingsValue::Text(s)) => t.name = s,
                        (TypeKey::Plural, SettingsValue::Text(s)) => t.plural = s,
                        (TypeKey::Dir, SettingsValue::Text(s)) => t.dir = s,
                        (TypeKey::Prefix, SettingsValue::Text(s)) => t.prefix = s,
                        (TypeKey::Icon, SettingsValue::OptText(o)) => t.icon = o,
                        (TypeKey::Numbering, SettingsValue::Numbering(n)) => t.numbering = n,
                        (TypeKey::Subdirectory, SettingsValue::Bool(b)) => t.subdirectory = b,
                        (TypeKey::Store, SettingsValue::Store(s)) => t.store = s,
                        (TypeKey::Singleton, SettingsValue::Bool(b)) => t.singleton = b,
                        (TypeKey::ParentType, SettingsValue::OptText(o)) => t.parent_type = o,
                        (TypeKey::Agents, SettingsValue::List(v)) => t.agents = v,
                        _ => {}
                    }
                }
            }
            FieldPath::Rel { index, key } => {
                if let Some(r) = buf.relationships.get_mut(*index) {
                    match (key, value) {
                        (RelKey::Name, SettingsValue::Text(s)) => r.name = s,
                        (RelKey::Inverse, SettingsValue::OptText(o)) => r.inverse = o,
                        _ => {}
                    }
                }
            }
            FieldPath::SqidsSalt => {
                if let (Some(s), SettingsValue::Text(v)) = (buf.documents.sqids.as_mut(), value) {
                    s.salt = v;
                }
            }
            FieldPath::SqidsMinLength => {
                if let (Some(s), SettingsValue::Num(n)) = (buf.documents.sqids.as_mut(), value) {
                    s.min_length = n as u8;
                }
            }
            FieldPath::ReservedRemote => {
                if let (Some(r), SettingsValue::Text(v)) = (buf.documents.reserved.as_mut(), value)
                {
                    r.remote = v;
                }
            }
            FieldPath::ReservedFormat => {
                if let (Some(r), SettingsValue::ReservedFormat(f)) =
                    (buf.documents.reserved.as_mut(), value)
                {
                    r.format = f;
                }
            }
            FieldPath::ReservedMaxRetries => {
                if let (Some(r), SettingsValue::Num(n)) = (buf.documents.reserved.as_mut(), value) {
                    r.max_retries = n as u8;
                }
            }
            FieldPath::GithubRepo => {
                if let (Some(g), SettingsValue::OptText(o)) = (buf.documents.github.as_mut(), value)
                {
                    g.repo = o;
                }
            }
            FieldPath::GithubCacheTtl => {
                if let (Some(g), SettingsValue::Num(n)) = (buf.documents.github.as_mut(), value) {
                    g.cache_ttl = n;
                }
            }
            FieldPath::CertNormalize => {
                if let SettingsValue::Bool(b) = value {
                    buf.certification.normalize = b;
                }
            }
            FieldPath::CertOverride { key } => {
                if let (Some(o), SettingsValue::Bool(b)) =
                    (buf.certification.overrides.get_mut(key), value)
                {
                    o.normalize = b;
                }
            }
            FieldPath::AgentsInteractive => {
                if let SettingsValue::OptText(o) = value {
                    buf.agents.interactive = o;
                }
            }
            FieldPath::UiAsciiDiagrams => {
                if let SettingsValue::Bool(b) = value {
                    buf.ui.ascii_diagrams = b;
                }
            }
            FieldPath::StatusbarEnabled => {
                if let SettingsValue::Bool(b) = value {
                    buf.ui.statusbar.enabled = b;
                }
            }
            FieldPath::MultilineMaxExpandedHeight => {
                if let SettingsValue::Num(n) = value {
                    buf.ui.multiline.max_expanded_height = n as usize;
                }
            }
            FieldPath::Edge { index, key } => {
                if let Some(e) = buf.edges.get_mut(*index) {
                    match (key, value) {
                        (EdgeKey::Name, SettingsValue::Text(s)) => e.name = s,
                        (EdgeKey::From, SettingsValue::List(v)) => e.from = type_selector_from(v),
                        (EdgeKey::To, SettingsValue::List(v)) => e.to = type_selector_from(v),
                        (EdgeKey::Via, SettingsValue::List(v)) => e.via = rel_selector_from(v),
                        (EdgeKey::Required, SettingsValue::OptSeverity(o)) => e.required = o,
                        (EdgeKey::Traversal, SettingsValue::OptTraversal(o)) => e.traversal = o,
                        _ => {}
                    }
                }
            }
            // Statusbar slot ordering and unset placeholders are not editable here.
            FieldPath::StatusbarLeft
            | FieldPath::StatusbarCenter
            | FieldPath::StatusbarRight
            | FieldPath::Unset => {}
        }
    }

    /// Begin text-entry editing on the focused field, seeding the input with its
    /// current raw string. No-op for Toggle/EnumCycle/ReadOnly (those edit via
    /// Space or not at all).
    pub fn settings_start_edit(&mut self) {
        let Some(focused) = self.settings_focused_field() else {
            return;
        };
        // A status-bar zone row opens the two-pane ordering editor, not text entry.
        if focused.editor == FieldEditor::ZoneOrdering {
            self.settings_open_zone_editor(focused.path);
            return;
        }
        let editable = matches!(
            focused.editor,
            FieldEditor::Text
                | FieldEditor::BoundedNum { .. }
                | FieldEditor::Nullable
                | FieldEditor::List
        );
        if !editable {
            return;
        }
        self.settings_editing = true;
        self.settings_edit_input = self.settings_focused_raw();
        self.settings_edit_error = None;
    }

    /// Open the status-bar zone ordering editor for `path`, seeding it from the
    /// matching buffer zone and its RFC-022 default names (so an unset zone starts
    /// from what the bar actually renders, not a blank).
    fn settings_open_zone_editor(&mut self, path: FieldPath) {
        use crate::tui::views::status_bar::{
            STATUS_BAR_DEFAULT_CENTER, STATUS_BAR_DEFAULT_LEFT, STATUS_BAR_DEFAULT_RIGHT,
        };
        let sb = &self.settings_buffer.ui.statusbar;
        let (current, defaults): (Option<&Vec<String>>, &[&str]) = match path {
            FieldPath::StatusbarLeft => (sb.left.as_ref(), STATUS_BAR_DEFAULT_LEFT),
            FieldPath::StatusbarCenter => (sb.center.as_ref(), STATUS_BAR_DEFAULT_CENTER),
            FieldPath::StatusbarRight => (sb.right.as_ref(), STATUS_BAR_DEFAULT_RIGHT),
            _ => return,
        };
        self.settings_zone_editor = Some(ZoneOrderingEditor::new(path.clone(), current, defaults));
    }

    /// Commit the active zone editor into the buffer at its `path`: a non-empty
    /// list writes `Some(order)`; an empty list writes `Some(vec![])` (an explicit
    /// clear, distinct from an untouched zone's `None`). Dirties the buffer and
    /// closes the editor.
    pub fn settings_commit_zone(&mut self) {
        let Some(editor) = self.settings_zone_editor.take() else {
            return;
        };
        let value = Some(editor.selected);
        let sb = &mut self.settings_buffer.ui.statusbar;
        match editor.path {
            FieldPath::StatusbarLeft => sb.left = value,
            FieldPath::StatusbarCenter => sb.center = value,
            FieldPath::StatusbarRight => sb.right = value,
            _ => return,
        }
        self.settings_dirty = true;
    }

    /// Close the zone editor without writing; the buffer is untouched (an untouched
    /// zone stays `None`).
    pub fn settings_cancel_zone(&mut self) {
        self.settings_zone_editor = None;
    }

    pub fn settings_cancel_edit(&mut self) {
        self.settings_editing = false;
        self.settings_edit_input.clear();
        self.settings_edit_error = None;
    }

    /// Commit the in-progress text edit into the buffer. Validating editors
    /// (BoundedNum/Duration) that reject the input keep edit mode active and set
    /// `settings_edit_error` without touching the buffer or dirty flag.
    pub fn settings_confirm_edit(&mut self) {
        let Some(focused) = self.settings_focused_field() else {
            return;
        };
        let input = self.settings_edit_input.trim().to_string();
        match focused.editor {
            FieldEditor::Text => {
                self.settings_write(&focused.path, SettingsValue::Text(input));
                self.settings_dirty = true;
                self.settings_editing = false;
            }
            FieldEditor::Nullable => {
                let opt = if input.is_empty() { None } else { Some(input) };
                self.settings_write(&focused.path, SettingsValue::OptText(opt));
                self.settings_dirty = true;
                self.settings_editing = false;
            }
            FieldEditor::List => {
                let list: Vec<String> = input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(message) = empty_edge_position_refusal(&focused.path, &list) {
                    self.settings_edit_error = Some(message);
                    return;
                }
                self.settings_write(&focused.path, SettingsValue::List(list));
                self.settings_dirty = true;
                self.settings_editing = false;
            }
            FieldEditor::BoundedNum { min, max } => match validate_bounded(&input, min, max) {
                Ok(n) => {
                    self.settings_write(&focused.path, SettingsValue::Num(n));
                    self.settings_dirty = true;
                    self.settings_editing = false;
                }
                Err(msg) => {
                    self.settings_edit_error = Some(msg);
                }
            },
            FieldEditor::Toggle
            | FieldEditor::EnumCycle { .. }
            | FieldEditor::ZoneOrdering
            | FieldEditor::ReadOnly => {}
        }
    }

    /// Space on the focused field: flip a Toggle or advance an EnumCycle in the
    /// buffer. No-op for text-entry and ReadOnly kinds.
    pub fn settings_space(&mut self) {
        let Some(focused) = self.settings_focused_field() else {
            return;
        };
        match focused.editor {
            FieldEditor::Toggle => self.settings_toggle_bool(),
            FieldEditor::EnumCycle { variants } => {
                self.settings_cycle_enum(&focused.path, variants);
            }
            _ => {}
        }
    }

    /// Flip the focused bool field in the buffer and mark it dirty (AC3). No-op
    /// when the focused field is not a Toggle.
    pub fn settings_toggle_bool(&mut self) {
        let Some(focused) = self.settings_focused_field() else {
            return;
        };
        if focused.editor != FieldEditor::Toggle {
            return;
        }
        let current = self.settings_focused_raw() == "true";
        self.settings_write(&focused.path, SettingsValue::Bool(!current));
        self.settings_dirty = true;
    }

    fn settings_cycle_enum(&mut self, path: &FieldPath, variants: &'static [&'static str]) {
        if variants.is_empty() {
            return;
        }
        let current = self.settings_focused_raw();
        let idx = variants.iter().position(|v| *v == current).unwrap_or(0);
        let next = variants[(idx + 1) % variants.len()];
        self.settings_set_enum_variant(path, next);
    }

    /// Write `variant` into the enum field at `path`, firing the same dependency
    /// auto-scaffolding the enum-cycle path does. This is the shared write the
    /// cycle path and the variant picker both call: the cycle computes `next` then
    /// delegates here, so cycle and pick converge on identical behaviour (RFC-023
    /// STORY-144).
    pub fn settings_set_enum_variant(&mut self, path: &FieldPath, variant: &str) {
        // Re-selecting the current variant is a no-op: no buffer write, no dirty.
        // The cycle path never reaches here unchanged (it advances the index); the
        // picker can, when the user re-picks the row already in force.
        if variant == self.settings_focused_raw() {
            return;
        }
        match path {
            FieldPath::Type {
                key: TypeKey::Numbering,
                ..
            } => {
                if let Some(n) = numbering_from_variant(variant) {
                    self.settings_write(path, SettingsValue::Numbering(n));
                    self.settings_dirty = true;
                    self.scaffold_for_cycled_value(path, variant);
                }
            }
            FieldPath::Type {
                key: TypeKey::Store,
                ..
            } => {
                if let Some(s) = store_from_variant(variant) {
                    self.settings_write(path, SettingsValue::Store(s));
                    self.settings_dirty = true;
                    self.scaffold_for_cycled_value(path, variant);
                }
            }
            FieldPath::ReservedFormat => {
                if let Some(f) = reserved_format_from_variant(variant) {
                    self.settings_write(path, SettingsValue::ReservedFormat(f));
                    self.settings_dirty = true;
                }
            }
            FieldPath::Edge {
                key: EdgeKey::Required,
                ..
            } => {
                if let Some(severity) = optional_variant(variant, severity_from_variant) {
                    self.settings_write(path, SettingsValue::OptSeverity(severity));
                    self.settings_dirty = true;
                }
            }
            FieldPath::Edge {
                key: EdgeKey::Traversal,
                ..
            } => {
                if let Some(traversal) = optional_variant(variant, traversal_from_variant) {
                    self.settings_write(path, SettingsValue::OptTraversal(traversal));
                    self.settings_dirty = true;
                }
            }
            _ => {}
        }
    }

    /// After a numbering/store enum is cycled to `next`, auto-insert the optional
    /// config section that value depends on. Only sqids/reserved/github-issues
    /// introduce a dependency; every other variant (and field) is a no-op. When the
    /// scaffolded section carries a required-but-empty field (only the sqids salt)
    /// the result is stashed as a jump offer; sections that scaffold complete
    /// (reserved/github) set no offer, since there is nothing to jump to. A section
    /// that was already present scaffolds nothing and sets no offer (AC6).
    fn scaffold_for_cycled_value(&mut self, path: &FieldPath, next: &str) {
        let dep = match (path, next) {
            (
                FieldPath::Type {
                    key: TypeKey::Numbering,
                    ..
                },
                "sqids",
            ) => Some(ConfigDep::NumberingSqids),
            (
                FieldPath::Type {
                    key: TypeKey::Numbering,
                    ..
                },
                "reserved",
            ) => Some(ConfigDep::NumberingReserved),
            (
                FieldPath::Type {
                    key: TypeKey::Store,
                    ..
                },
                "github-issues" | "github-milestones" | "github-projects",
            ) => Some(ConfigDep::Github),
            _ => None,
        };
        let Some(dep) = dep else {
            return;
        };
        if let Some(result) = scaffold_dependency(&mut self.settings_buffer, dep) {
            if result.required_empty_field.is_some() {
                self.settings_scaffold_offer = Some(result);
            }
        }
    }

    /// Save the settings buffer to `.lazyspec.toml`, pausing first when the edit
    /// would orphan existing documents. If the dirty buffer changes a load-bearing
    /// `[[types]]` field (`dir`/`prefix`/`store`) on a type that already has docs on
    /// disk, the save is held: the computed impacts are stashed on
    /// `settings_impact_confirm` (active) and NOTHING is written -- the buffer and
    /// dirty flag retain the pending edit until the user confirms or cancels (RFC-023
    /// slice 6). Otherwise (non-load-bearing edits, or load-bearing edits on
    /// zero-doc types) the save commits atomically via `settings_commit_write`.
    pub fn settings_save(&mut self, root: &Path, config_on_disk: &Config) {
        let impacts = settings_guard::detect_type_field_impacts(
            &self.settings_buffer,
            config_on_disk,
            &self.store,
        );
        if impacts.is_empty() {
            self.settings_commit_write(root);
            return;
        }
        self.settings_impact_confirm.impacts = impacts;
        self.settings_impact_confirm.active = true;
    }

    /// Confirm a paused document-impact save: clear the guard and commit the
    /// pending buffer to `.lazyspec.toml` atomically, exactly as a guard-free save
    /// would. No document files are moved/renamed/renumbered (RFC-023 slice 6, AC3).
    pub fn confirm_settings_impact(&mut self, root: &Path) {
        self.settings_impact_confirm.active = false;
        self.settings_impact_confirm.impacts.clear();
        self.settings_commit_write(root);
    }

    /// Cancel a paused document-impact save: clear the guard and write nothing.
    /// `.lazyspec.toml` is untouched and the buffer + `settings_dirty` retain the
    /// pending edit (RFC-023 slice 6, AC4).
    pub fn cancel_settings_impact(&mut self) {
        self.settings_impact_confirm.active = false;
        self.settings_impact_confirm.impacts.clear();
    }

    /// Validate the whole buffer, then write `.lazyspec.toml` once, atomically,
    /// preserving comments. Disk is touched only on success: the new file string
    /// is rendered in memory by `write_config_in_place` and validated by
    /// re-parsing those exact bytes via `Config::parse` before any write. On
    /// either failure path (writer error or re-parse error) nothing is written,
    /// `settings_dirty` stays true, and `settings_footer_error` is set; a parse
    /// failure also jumps focus to the offending field. On success the dirty flag
    /// and footer clear and `config_reload_request` is raised so the run loop
    /// re-loads the config, rebuilds the store, and re-seeds the clean buffer.
    ///
    /// This is the one place a buffer meets the loader's judgement, and so the
    /// answer to which of the panel's two commits STORY-260 AC4 means: the save,
    /// not the field commit (ITERATION-389). `Config::parse` is whole-config and
    /// all-or-nothing, and the save is the only whole-config action -- running
    /// the same check at field-commit time would refuse an edge edit for a
    /// violation elsewhere in the buffer (a salt the designer has not filled in
    /// yet), and narrowing it to the edited row would be a second spelling of
    /// the loader's predicates, which is the drift AC4 exists to prevent. It
    /// would also refuse legitimate waypoints: widening `from` to `*` before
    /// clearing `required` is a two-field edit whose intermediate does not load.
    ///
    /// So the engine seam ITERATION-389 offered -- lifting `parse_inner`'s
    /// invariant block into a `Config::check` a live buffer could be handed to
    /// -- was not taken, and needs no re-litigating on those grounds. What would
    /// reopen it is a check that is genuinely per-row rather than per-config.
    fn settings_commit_write(&mut self, root: &Path) {
        let path = root.join(".lazyspec.toml");
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            // A first save in a session whose file is gone falls back to a fresh
            // render so the write still succeeds.
            Err(_) => match self.settings_buffer.to_toml() {
                Ok(s) => s,
                Err(e) => {
                    self.settings_footer_error = Some(e.to_string());
                    return;
                }
            },
        };

        let new_src =
            match crate::engine::config_write::write_config_in_place(&src, &self.settings_buffer) {
                Ok(s) => s,
                Err(e) => {
                    self.settings_footer_error = Some(e.to_string());
                    return;
                }
            };

        // Validate the exact bytes destined for disk (catches every field-level
        // and cross-field constraint, plus any toml_edit slip), so the file never
        // holds an invalid intermediate.
        if let Err(e) = Config::parse(&new_src) {
            self.settings_footer_error = Some(e.to_string());
            self.settings_jump_to_violation();
            return;
        }

        if let Err(e) = std::fs::write(&path, &new_src) {
            self.settings_footer_error = Some(e.to_string());
            return;
        }

        self.settings_dirty = false;
        self.settings_footer_error = None;
        self.config_reload_request = true;
    }

    /// Move the settings nav onto the first buffer constraint a save can violate,
    /// in `Config::parse`'s order. The offending field is found by inspecting the
    /// buffer (more reliable than string-matching the error). Best-effort: an
    /// unrecognised violation lands on a relevant category without crashing, and
    /// the field cursor is clamped to the resolved field list.
    fn settings_jump_to_violation(&mut self) {
        self.settings_editing = false;

        // 1. An `[[edges]]` row the loader refuses. First, because `parse_inner`
        // checks the DAG before numbering and stores, and it bails on the first
        // violation it finds -- so the earliest violation in its order is the one
        // the footer is talking about.
        if let Some((row, key)) = self.first_edge_violation() {
            self.settings_jump_to_field("Edges", Some(row), |f| {
                f.path == (FieldPath::Edge { index: row, key })
            });
            return;
        }

        let buf = &self.settings_buffer;

        // 2. A Sqids type with no valid [numbering.sqids] salt.
        let needs_sqids = buf
            .documents
            .types
            .iter()
            .position(|t| t.numbering == NumberingStrategy::Sqids);
        if let Some(type_index) = needs_sqids {
            let salt_ok = buf
                .documents
                .sqids
                .as_ref()
                .is_some_and(|s| !s.salt.is_empty());
            if !salt_ok {
                // The salt is a focusable field only when the section exists.
                if buf.documents.sqids.is_some() {
                    self.settings_jump_to_field("Numbering", None, |f| {
                        matches!(f.path, FieldPath::SqidsSalt)
                    });
                } else {
                    self.settings_jump_to_field("Document Types", Some(type_index), |f| {
                        matches!(
                            f.path,
                            FieldPath::Type {
                                key: TypeKey::Numbering,
                                ..
                            }
                        )
                    });
                }
                return;
            }
        }

        // 3. A GithubIssues type with no [github] section.
        if buf.documents.github.is_none() {
            let offending = buf
                .documents
                .types
                .iter()
                .position(|t| t.store == StoreBackend::GithubIssues);
            if let Some(type_index) = offending {
                self.settings_jump_to_field("Document Types", Some(type_index), |f| {
                    matches!(
                        f.path,
                        FieldPath::Type {
                            key: TypeKey::Store,
                            ..
                        }
                    )
                });
                return;
            }
        }

        // 4. Any other constraint (reserved/relationships): best-effort landing on
        // a relevant category, clamped, never crashing. It must not land on an
        // edge field -- a violation no arm above claims is not the Edges panel's
        // to answer for. The retired `[[rules]]` refusal (STORY-259) never
        // reaches here at all: `write_config_in_place` deletes the table, so the
        // bytes this jump is explaining cannot still declare one.
        self.settings_jump_to_field("Numbering", None, |_| false);
    }

    /// The first `[[edges]]` violation `Config::parse` would bail on, as the row
    /// index and the field at fault -- attribution only. The *message* is the
    /// loader's, produced by re-parsing the bytes destined for disk; this walks
    /// the buffer in the same order `parse_inner` does so the field it names is
    /// the one that message is about.
    ///
    /// Each arm reads a predicate the engine already exposes -- the selectors'
    /// `names()` and `EdgeDef::overlaps`/`specificity` -- rather than re-deriving
    /// a check, because a re-derivation here would be the second spelling
    /// STORY-260 AC4 forbids, merely relocated from the message to the
    /// attribution. Every arm is tested against a real refused save for that
    /// reason: a divergence from the loader has to fail, not go unnoticed.
    fn first_edge_violation(&self) -> Option<(usize, EdgeKey)> {
        let buf = &self.settings_buffer;
        let declared_type = |name: &String| buf.type_by_name(name).is_some();
        let declared_rel = |name: &String| buf.relationship_by_name(name).is_some();

        for (row, edge) in buf.edges.iter().enumerate() {
            // A wildcard written inside a list and an undeclared name are two
            // messages but one field, so one arm covers both positions.
            for (key, names) in [
                (EdgeKey::From, edge.from.names()),
                (EdgeKey::To, edge.to.names()),
            ] {
                if names
                    .iter()
                    .any(|name| name == WILDCARD || !declared_type(name))
                {
                    return Some((row, key));
                }
            }
            if edge
                .via
                .names()
                .iter()
                .any(|via| via == WILDCARD || !declared_rel(via))
            {
                return Some((row, EdgeKey::Via));
            }
            // `from = "*"` is a legal position on its own; the row is refused
            // because `required` is set on it, and clearing `required` is the fix
            // that keeps the position the designer just declared.
            if edge.required.is_some() && edge.from == TypeSelector::Any {
                return Some((row, EdgeKey::Required));
            }
        }

        // A pairwise refusal has no single culprit -- either row can be narrowed
        // or changed to agree -- so the cursor goes to the row the loader's
        // message names first, which is the earlier row whether or not it is the
        // one just edited.
        Self::first_pairwise_disagreement(&buf.edges)
    }

    /// The earlier row of the first pair of `[[edges]]` rows that overlap and
    /// disagree, and the qualifier they disagree on: a requiredness tie at equal
    /// specificity, then a traversal disagreement, which is the order
    /// `parse_inner` asks in. Rows that omit the qualifier state nothing and so
    /// disagree with nothing.
    fn first_pairwise_disagreement(edges: &[EdgeDef]) -> Option<(usize, EdgeKey)> {
        let ties = edges.iter().enumerate().find_map(|(row, edge)| {
            let severity = edge.required.as_ref()?;
            edges[row + 1..]
                .iter()
                .filter(|other| other.overlaps(edge) && other.specificity() == edge.specificity())
                .any(|other| other.required.as_ref().is_some_and(|s| s != severity))
                .then_some((row, EdgeKey::Required))
        });
        if ties.is_some() {
            return ties;
        }
        edges.iter().enumerate().find_map(|(row, edge)| {
            let traversal = edge.traversal?;
            edges[row + 1..]
                .iter()
                .filter(|other| other.overlaps(edge))
                .any(|other| other.traversal.is_some_and(|t| t != traversal))
                .then_some((row, EdgeKey::Traversal))
        })
    }

    /// Land the settings nav on the scaffold offer's required-but-empty field
    /// (only the sqids salt: Numbering category, the salt field) so the user can
    /// fill it. Reuses `settings_jump_to_field` rather than re-deriving focus.
    pub(crate) fn settings_jump_to_scaffolded_field(&mut self, path: &FieldPath) {
        match path {
            FieldPath::SqidsSalt => {
                self.settings_jump_to_field("Numbering", None, |f| {
                    matches!(f.path, FieldPath::SqidsSalt)
                });
            }
            // No other scaffolded section produces a required-empty field today; a
            // best-effort landing keeps this total without crashing.
            _ => self.settings_jump_to_field("Numbering", None, |_| false),
        }
    }

    /// Land the settings nav on the category called `category` (optionally
    /// drilled into `drill`) and set the field cursor to the first field
    /// matching `pick`, clamped to the field list. With no match the cursor is
    /// 0. Callers name the category rather than numbering it; see
    /// [`App::settings_category_index`].
    fn settings_jump_to_field(
        &mut self,
        category: &str,
        drill: Option<usize>,
        pick: impl Fn(&EditableField) -> bool,
    ) {
        let category = Self::settings_category_index(category);
        self.settings_category = category;
        self.settings_drill = drill;
        if let Some(d) = drill {
            self.settings_entry = d;
        }
        let fields = crate::tui::views::panels::settings_fields(
            category,
            self.settings_entry,
            drill,
            &self.settings_buffer,
        );
        let field = fields.iter().position(pick).unwrap_or(0);
        self.settings_field = field.min(fields.len().saturating_sub(1));
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

    /// Recompute the sorted, de-duplicated set of tags across all docs. Feeds the
    /// filter tag cycle and the graph tag pivots, so it must be current whenever
    /// either is shown -- refreshed at construction, on reload, and on entering
    /// filters.
    pub fn refresh_available_tags(&mut self) {
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

    pub fn enter_filters_mode(&mut self) {
        self.refresh_available_tags();
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
                    None => self.available_statuses.first().map(|s| Status::new(s)),
                    Some(current) => {
                        let pos = self
                            .available_statuses
                            .iter()
                            .position(|s| s == current.as_str());
                        match pos {
                            Some(i) if i + 1 < self.available_statuses.len() => {
                                Some(Status::new(&self.available_statuses[i + 1]))
                            }
                            _ => None,
                        }
                    }
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
                    None => self.available_statuses.last().map(|s| Status::new(s)),
                    Some(current) => {
                        let pos = self
                            .available_statuses
                            .iter()
                            .position(|s| s == current.as_str());
                        match pos {
                            Some(0) | None => None,
                            Some(i) => Some(Status::new(&self.available_statuses[i - 1])),
                        }
                    }
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
        use crate::engine::context::{resolve_forest, resolve_forest_by_tag};
        // Keep the tag pivots current with the store before re-rooting.
        self.refresh_available_tags();
        let forest = match self.graph_anchor {
            GraphAnchor::All => resolve_forest(&self.store, None),
            GraphAnchor::Type(i) => {
                let ty = self.doc_types.get(i).map(|dt| dt.as_str());
                resolve_forest(&self.store, ty)
            }
            GraphAnchor::Tag(i) => match self.available_tags.get(i) {
                Some(tag) => resolve_forest_by_tag(&self.store, tag),
                None => resolve_forest(&self.store, None),
            },
        };
        let sort = super::graph::GraphSort {
            col: self.graph_sort_col.clone(),
            rev: self.graph_sort_rev,
        };
        self.graph_nodes = flatten_forest(&forest, &self.store, &sort);
        self.graph_selected = 0;
        self.graph_offset = 0;
    }

    /// The ordered sort-column cycle for `o`: `path`, then `status`, then every
    /// declared attribute name across the configured types (first-seen order,
    /// de-duplicated). `related` is a display-only column and is NOT a sort key.
    /// Built from config so the cycle tracks the project's attribute schema.
    pub fn graph_sort_cycle(&self, config: &Config) -> Vec<String> {
        let mut cols = vec!["path".to_string(), "status".to_string()];
        for ty in &config.documents.types {
            for attr in &ty.attributes {
                if !cols.contains(&attr.name) {
                    cols.push(attr.name.clone());
                }
            }
        }
        cols
    }

    /// Advance the graph sort column one step through `graph_sort_cycle`,
    /// wrapping at the end. An unknown current column (e.g. a config `sort`
    /// naming a since-removed attribute) restarts the cycle at `path`. Rebuilds
    /// the graph so the display reorders.
    pub fn cycle_graph_sort(&mut self, config: &Config) {
        let cycle = self.graph_sort_cycle(config);
        if cycle.is_empty() {
            return;
        }
        let next = match cycle.iter().position(|c| *c == self.graph_sort_col) {
            Some(idx) => (idx + 1) % cycle.len(),
            None => 0,
        };
        self.graph_sort_col = cycle[next].clone();
        self.rebuild_graph();
    }

    /// Toggle the graph sort direction (`O`) and rebuild so siblings reorder.
    pub fn reverse_graph_sort(&mut self) {
        self.graph_sort_rev = !self.graph_sort_rev;
        self.rebuild_graph();
    }

    /// Advance the graph pivot anchor one row down the sidebar (All -> types… ->
    /// tags…), clamped at the last row. No wraparound. Rebuilds after moving.
    pub fn move_graph_anchor_next(&mut self) {
        let nt = self.doc_types.len();
        let ntags = self.available_tags.len();
        let total = 1 + nt + ntags;
        let next = (anchor_to_flat(self.graph_anchor, nt) + 1).min(total - 1);
        self.graph_anchor = flat_to_anchor(next, nt, ntags);
        self.rebuild_graph();
    }

    /// Retreat the graph pivot anchor one row up the sidebar, clamped at All
    /// (the whole-store forest). No wraparound. Rebuilds after moving.
    pub fn move_graph_anchor_prev(&mut self) {
        let nt = self.doc_types.len();
        let ntags = self.available_tags.len();
        let prev = anchor_to_flat(self.graph_anchor, nt).saturating_sub(1);
        self.graph_anchor = flat_to_anchor(prev, nt, ntags);
        self.rebuild_graph();
    }

    pub fn graph_adjust_viewport(&mut self) {
        self.graph_offset = clamp_viewport_offset(
            self.graph_selected,
            self.graph_offset,
            self.graph_list_height,
            self.graph_nodes.len(),
        );
    }

    pub fn graph_move_down(&mut self) {
        let n = self.graph_nodes.len();
        if n > 0 && self.graph_selected < n - 1 {
            self.graph_selected += 1;
        }
        self.graph_adjust_viewport();
    }

    pub fn graph_move_up(&mut self) {
        self.graph_selected = self.graph_selected.saturating_sub(1);
        self.graph_adjust_viewport();
    }

    pub fn graph_half_page_down(&mut self) {
        let n = self.graph_nodes.len();
        if n == 0 {
            return;
        }
        let jump = self.graph_list_height / 2;
        self.graph_selected = (self.graph_selected + jump).min(n - 1);
        self.graph_adjust_viewport();
    }

    pub fn graph_half_page_up(&mut self) {
        let jump = self.graph_list_height / 2;
        self.graph_selected = self.graph_selected.saturating_sub(jump);
        self.graph_adjust_viewport();
    }

    pub fn graph_move_to_top(&mut self) {
        self.graph_selected = 0;
        self.graph_offset = 0;
    }

    pub fn graph_move_to_bottom(&mut self) {
        let n = self.graph_nodes.len();
        if n > 0 {
            self.graph_selected = n - 1;
            self.graph_offset = n.saturating_sub(self.graph_list_height);
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
        docs.sort_by(|a, b| DocMeta::sort_by_date(a, b));
        docs
    }

    pub fn selected_doc_meta(&self) -> Option<&DocMeta> {
        self.doc_tree
            .get(self.selected_doc)
            .and_then(|node| self.store.get(&node.path))
    }

    /// Decide how an [`OpenTarget`] opens: a web URL hands off to the browser; a
    /// file needs the configured viewer, whose command is split on whitespace so
    /// `viewer = "code -w"` yields `["code", "-w"]`. Absent a viewer, a file has
    /// no way to open and the error names the missing config.
    pub(crate) fn plan_open(
        target: OpenTarget,
        viewer: Option<&str>,
        root: &std::path::Path,
    ) -> Result<OpenRequest, String> {
        match target {
            OpenTarget::Url(url) => Ok(OpenRequest::Browser(url)),
            OpenTarget::File(path) => {
                let command: Vec<String> = viewer
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                if command.is_empty() {
                    Err(format!(
                        "cannot open {}: it has no web URL and no viewer is configured. \
                         Set `viewer` under [tui] in .lazyspec.toml (e.g. viewer = \"glow\").",
                        path.display()
                    ))
                } else {
                    Ok(OpenRequest::Viewer {
                        command,
                        path: root.join(path),
                    })
                }
            }
        }
    }

    /// Resolve the selected doc's open target and stage the request (browser or
    /// viewer). A missing viewer for a file-only target surfaces the transient
    /// `open_message` notice instead.
    pub(crate) fn request_open(&mut self, root: &std::path::Path, config: &Config) {
        let Some(doc) = self.selected_doc_meta().cloned() else {
            return;
        };
        let coords = crate::engine::github_url::resolve_repo_coords(config, root);
        let issue_map = crate::engine::issue_map::IssueMap::load(root).unwrap_or_default();
        let target = resolve_open_target(&doc, coords.as_ref(), config, &issue_map);
        match Self::plan_open(target, config.ui.viewer.as_deref(), root) {
            Ok(req) => self.open_request = Some(req),
            Err(msg) => self.open_message = Some(msg),
        }
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
        self.doc_list_offset = clamp_viewport_offset(
            self.selected_doc,
            self.doc_list_offset,
            self.doc_list_height,
            doc_count,
        );
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
        self.search_pending = false;
        self.search_generation = self.search_generation.wrapping_add(1);
    }

    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selected = 0;
        self.search_pending = false;
        self.search_generation = self.search_generation.wrapping_add(1);
    }

    /// Dispatch the current query to the background search worker (BUG-011):
    /// scoring 700+ full bodies per keystroke blocked the event loop, so the UI
    /// thread only snapshots the corpus (a cheap clone via the engine body
    /// cache) and stamps a fresh generation; results arrive later as
    /// [`AppEvent::SearchResults`] and are applied by [`apply_search_results`].
    /// The generation bump on an empty query invalidates any in-flight search
    /// so its late results cannot repopulate a cleared list.
    ///
    /// Fuzzy, ranked results still come from the shared engine scorer
    /// (STORY-129): the TUI never owns the matching algorithm.
    pub fn update_search(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_selected = 0;

        if self.search_query.is_empty() {
            self.search_results.clear();
            self.search_pending = false;
            return;
        }

        self.search_pending = true;
        let corpus = self.store.search_corpus(&*self.fs);
        let _ = self.search_tx.send(SearchRequest {
            corpus,
            query: self.search_query.clone(),
            generation: self.search_generation,
        });
    }

    /// Apply worker results, dropping them silently when the generation is
    /// stale (a newer keystroke has already superseded the query).
    pub fn apply_search_results(&mut self, generation: u64, results: Vec<std::path::PathBuf>) {
        if generation != self.search_generation {
            return;
        }
        self.search_results = results;
        self.search_selected = 0;
        self.search_pending = false;
    }

    /// Test-only synchronous search: dispatch, then run the corpus search
    /// inline and apply, so ranking tests exercise the exact production path
    /// (corpus snapshot -> engine scorer -> apply) without a worker thread.
    #[cfg(test)]
    pub(crate) fn run_search_now(&mut self) {
        self.update_search();
        if self.search_query.is_empty() {
            return;
        }
        let results = self
            .store
            .search_corpus(&*self.fs)
            .search(&self.search_query)
            .into_iter()
            .map(|r| r.path)
            .collect();
        self.apply_search_results(self.search_generation, results);
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
        let mut resolved = match crate::engine::context::resolve_chain(&self.store, &doc.id, 1) {
            Ok(r) => r,
            Err(_) => return RelationSections::default(),
        };
        crate::engine::context::merge_declared_related(&self.store, &mut resolved);

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
            self.create_form.state = crate::spinners::SpinnerState::Loading;
            self.create_form.status_message = Some("Reserving...".to_string());
            let _ = self.event_tx.send(AppEvent::CreateStarted);

            std::thread::spawn(move || {
                let thread_fs = crate::engine::fs::RealFileSystem;
                let progress_tx = tx.clone();
                let result = (|| -> Result<CreateResult, String> {
                    let store = Store::load(&root, &config).map_err(|e| e.to_string())?;
                    let path = crate::engine::ops::create::run(
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
                            let state = p.spinner_state();
                            let _ = progress_tx.send(AppEvent::CreateProgress { message, state });
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
                            crate::engine::ops::link::link_with_config(
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

        let path = crate::engine::ops::create::run(
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
            crate::engine::ops::link::link_with_config(
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
        crate::engine::ops::delete::run_with_config(
            root,
            &self.store,
            &doc_path_str,
            Some(config),
        )?;
        self.store.remove_file(&doc_path);
        self.filtered_docs_cache = None;

        self.close_delete_confirm();
        self.build_doc_tree();
        self.clamp_selected_doc();
        Ok(())
    }

    /// Seed a default entry into the current Vec-backed collection (Document
    /// Types / Relationships) and drill into it. Placeholder fields carry
    /// starter/default values so the new entry is immediately editable.
    /// Buffer-only; sets `settings_dirty`.
    pub fn settings_seed_entry(&mut self) {
        match self.settings_category {
            1 => {
                self.settings_buffer.documents.types.push(TypeDef {
                    name: "type".to_string(),
                    plural: "types".to_string(),
                    dir: "docs".to_string(),
                    prefix: "TYPE".to_string(),
                    icon: None,
                    numbering: NumberingStrategy::default(),
                    subdirectory: false,
                    store: StoreBackend::default(),
                    singleton: false,
                    parent_type: None,
                    agents: Vec::new(),
                    intent: None,
                    authorship: Default::default(),
                    lifecycle: crate::engine::config::default_lifecycle(),
                    attributes: Vec::new(),
                    label_override: None,
                    github_issue_tag: None,
                    github_issue_type: None,
                    status_authority: None,
                    clickup_list_id: None,
                    clickup_task_type: None,
                    clickup_custom_field_map: None,
                });
                self.settings_entry = self.settings_buffer.documents.types.len() - 1;
            }
            2 => {
                self.settings_buffer.relationships.push(RelationshipDef {
                    name: "relationship".to_string(),
                    inverse: None,
                    github_native: None,
                    traversal: None,
                });
                self.settings_entry = self.settings_buffer.relationships.len() - 1;
            }
            _ => return,
        }
        self.settings_drill = Some(self.settings_entry);
        self.settings_field = 0;
        self.settings_dirty = true;
    }

    /// Open the spec-path key prompt for a new certification override. The
    /// override is not inserted until the prompt is confirmed with a non-empty
    /// key (`settings_confirm_override`).
    pub fn settings_seed_override(&mut self) {
        self.override_key_prompt.active = true;
        self.override_key_prompt.input.clear();
    }

    pub fn settings_override_type_char(&mut self, c: char) {
        self.override_key_prompt.input.push(c);
    }

    pub fn settings_override_backspace(&mut self) {
        self.override_key_prompt.input.pop();
    }

    pub fn settings_cancel_override(&mut self) {
        self.override_key_prompt.active = false;
        self.override_key_prompt.input.clear();
    }

    /// Confirm the override key prompt: insert a new override under the trimmed
    /// key (with the default normalize) and drill into it. An empty key inserts
    /// nothing and leaves the prompt active. Buffer-only; sets `settings_dirty`.
    pub fn settings_confirm_override(&mut self) {
        let key = self.override_key_prompt.input.trim().to_string();
        if key.is_empty() {
            return;
        }
        self.settings_buffer.certification.overrides.insert(
            key.clone(),
            CertificationOverride {
                normalize: default_normalize(),
            },
        );
        self.settings_dirty = true;
        self.override_key_prompt.active = false;
        self.override_key_prompt.input.clear();

        let mut keys: Vec<&String> = self
            .settings_buffer
            .certification
            .overrides
            .keys()
            .collect();
        keys.sort();
        if let Some(index) = keys.iter().position(|k| **k == key) {
            self.settings_entry = index;
            self.settings_drill = Some(index);
            self.settings_field = 0;
        }
    }

    /// Open the confirm prompt for removing the selected settings collection
    /// entry. Resolves the target from `settings_entry` (sorted-key for cat 6).
    /// ADR-011: refuses to delete the last `[[relationships]]` entry (cat 2),
    /// returning without activating the confirm. No buffer mutation here.
    pub fn settings_open_delete_confirm(&mut self) {
        let category = self.settings_category;
        let (target, entry_label) = match category {
            1 => {
                let Some(t) = self
                    .settings_buffer
                    .documents
                    .types
                    .get(self.settings_entry)
                else {
                    return;
                };
                (
                    SettingsDeleteTarget::Index(self.settings_entry),
                    t.name.clone(),
                )
            }
            2 => {
                // ADR-011: a real config must keep at least one relationship.
                if self.settings_buffer.relationships.len() <= 1 {
                    return;
                }
                let Some(r) = self.settings_buffer.relationships.get(self.settings_entry) else {
                    return;
                };
                (
                    SettingsDeleteTarget::Index(self.settings_entry),
                    r.name.clone(),
                )
            }
            6 => {
                let mut keys: Vec<&String> = self
                    .settings_buffer
                    .certification
                    .overrides
                    .keys()
                    .collect();
                keys.sort();
                let Some(key) = keys.get(self.settings_entry) else {
                    return;
                };
                let key = (*key).clone();
                (SettingsDeleteTarget::Key(key.clone()), key)
            }
            _ => return,
        };
        self.settings_delete_confirm = SettingsDeleteConfirm {
            active: true,
            category,
            entry_label,
            target,
        };
    }

    pub fn settings_close_delete_confirm(&mut self) {
        self.settings_delete_confirm = SettingsDeleteConfirm::new();
    }

    /// Remove the targeted entry from the buffer (the only removal site, after
    /// confirm). Clamps `settings_entry` to the new collection length and closes
    /// the confirm. Buffer-only; sets `settings_dirty`.
    pub fn settings_confirm_delete(&mut self) {
        let category = self.settings_delete_confirm.category;
        let new_len = match self.settings_delete_confirm.target.clone() {
            SettingsDeleteTarget::Index(i) => match category {
                1 => {
                    if i < self.settings_buffer.documents.types.len() {
                        self.settings_buffer.documents.types.remove(i);
                    }
                    self.settings_buffer.documents.types.len()
                }
                2 => {
                    if i < self.settings_buffer.relationships.len() {
                        self.settings_buffer.relationships.remove(i);
                    }
                    self.settings_buffer.relationships.len()
                }
                _ => 0,
            },
            SettingsDeleteTarget::Key(k) => {
                self.settings_buffer.certification.overrides.remove(&k);
                self.settings_buffer.certification.overrides.len()
            }
        };
        self.settings_dirty = true;
        self.settings_entry = self.settings_entry.min(new_len.saturating_sub(1));
        self.settings_close_delete_confirm();
    }

    pub fn open_status_picker(&mut self, config: &Config) {
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

        // Offer only the moves the lifecycle permits: the current status (a
        // no-op, so the list is never empty and shows where the doc sits) plus
        // every state reachable from it by a declared edge.
        //
        // An unset status is not a state -- it is a board-bound doc the
        // authority board has not placed (STORY-248) -- so leading with it would
        // offer a blank first row and no edges lead out of it. Such a doc can
        // move anywhere in the lifecycle.
        let current = doc.status.as_str().to_string();
        let unset = current.is_empty();
        let mut states = if unset {
            Vec::new()
        } else {
            vec![current.clone()]
        };
        if let Some(type_def) = config.type_by_name(doc.doc_type.as_str()) {
            let lifecycle = type_def.effective_lifecycle();
            let candidates: Vec<String> = if unset {
                lifecycle.states.clone()
            } else {
                lifecycle
                    .targets_from(&current)
                    .into_iter()
                    .map(String::from)
                    .collect()
            };
            for target in candidates {
                if !states.contains(&target) {
                    states.push(target);
                }
            }
        }
        let path = doc.path.clone();

        self.status_picker.states = states;
        self.status_picker.selected = 0;
        self.status_picker.doc_path = path;
        self.status_picker.active = true;
    }

    pub fn close_status_picker(&mut self) {
        self.status_picker.active = false;
        self.status_picker.selected = 0;
        self.status_picker.doc_path = PathBuf::new();
        self.status_picker.error = None;
    }

    pub fn confirm_status_change(&mut self, root: &Path, config: &Config) -> Result<()> {
        let status = match self.status_picker.states.get(self.status_picker.selected) {
            Some(s) => Status::new(s),
            None => return Err(anyhow!("invalid status index")),
        };
        let doc_path = self.status_picker.doc_path.clone();
        let doc_path_str = doc_path.to_string_lossy().to_string();

        if let Err(e) = crate::engine::ops::update::run_with_config(
            root,
            &self.store,
            &doc_path_str,
            &[("status", &status.to_string())],
            Some(config),
        ) {
            self.status_picker.error = Some(e.to_string());
            return Err(e);
        }
        self.store.reload_file(root, &doc_path, &*self.fs)?;
        self.filtered_docs_cache = None;
        self.build_doc_tree();
        self.status_picker.error = None;
        self.close_status_picker();
        Ok(())
    }

    pub fn open_link_editor(&mut self, config: &Config) {
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

        // A `github-milestones` doc can never be the SOURCE of a relation, but it
        // can be the TARGET of `targets` (`github_native = "milestone"`). So from
        // a milestone doc we offer the inverse keyword(s) of every milestone-native
        // relation (e.g. `targeted-by`): selecting one plus an issue flips
        // direction in the core (link.rs:65), writing `targets: <milestone>` on the
        // issue. A milestone whose native relation declares no inverse has no legal
        // relation and falls back to the empty-state.
        //
        // A milestone-native relation may *only* originate on a github-issues doc
        // (the core guard rejects any other source). So a non-issue, non-milestone
        // source (e.g. a filesystem spec) is offered the global keyword list with
        // every milestone-native keyword (both the forward name and its inverse)
        // removed; a github-issues source keeps the full set.
        let store = self.store_of_path(&path, config);
        let is_milestone_doc = store == Some(StoreBackend::GithubMilestones);
        let is_issue_doc = store == Some(StoreBackend::GithubIssues);
        self.rel_types = if is_milestone_doc {
            config
                .relationships
                .iter()
                .filter(|r| r.github_native.as_deref() == Some("milestone"))
                .filter_map(|r| r.inverse.clone())
                .collect()
        } else if is_issue_doc {
            config.relationship_keywords()
        } else {
            let milestone_keywords: Vec<String> = config
                .relationships
                .iter()
                .filter(|r| r.github_native.as_deref() == Some("milestone"))
                .flat_map(|r| std::iter::once(r.name.clone()).chain(r.inverse.clone()))
                .collect();
            config
                .relationship_keywords()
                .into_iter()
                .filter(|kw| !milestone_keywords.contains(kw))
                .collect()
        };
        let source_blocked = !is_issue_doc && self.rel_types.is_empty();

        self.link_editor.active = true;
        self.link_editor.doc_path = path;
        self.link_editor.rel_type_index = 0;
        self.link_editor.query = String::new();
        self.link_editor.selected = 0;
        self.link_editor.error = None;
        self.link_editor.source_blocked = source_blocked;
        self.update_link_search(config);
    }

    /// Resolve the store backend of the doc at `path` via its declared type.
    /// Returns `None` when the doc or its type is unknown.
    fn store_of_path(&self, path: &Path, config: &Config) -> Option<StoreBackend> {
        let doc = self.store.get(path)?;
        config
            .type_by_name(doc.doc_type.as_str())
            .map(|t| t.store.clone())
    }

    pub fn close_link_editor(&mut self) {
        self.link_editor.active = false;
        self.link_editor.doc_path = PathBuf::new();
        self.link_editor.rel_type_index = 0;
        self.link_editor.query = String::new();
        self.link_editor.results = Vec::new();
        self.link_editor.selected = 0;
        self.link_editor.error = None;
        self.link_editor.source_blocked = false;
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
        self.build_doc_tree();
        self.close_provenance_editor();
        Ok(())
    }

    pub fn update_link_search(&mut self, config: &Config) {
        // A milestone-store viewed doc can never be a source, so it has no valid
        // relation and offers no candidates regardless of the query.
        if self.link_editor.source_blocked {
            self.link_editor.results = Vec::new();
            self.link_editor.selected = 0;
            return;
        }

        let query = self.link_editor.query.to_lowercase();
        let doc_path = self.link_editor.doc_path.clone();

        // The selected relation and the viewed doc's store together fix which
        // store a candidate may live in. A milestone-native relation (`targets`)
        // bridges github-issues <-> github-milestones: from an issue source the
        // candidate must be a milestone, from a milestone source (the
        // `targeted-by` inverse) it must be an issue. Every ordinary relation
        // excludes milestone docs (a milestone is only ever the target of
        // `targets`). The selected keyword may be an inverse, so resolve it to
        // its canonical relationship before reading `github_native`.
        let is_milestone_rel = self
            .rel_types
            .get(self.link_editor.rel_type_index)
            .and_then(|kw| config.resolve_relationship(kw).ok())
            .and_then(|(name, _)| {
                config
                    .relationship_by_name(&name)
                    .and_then(|r| r.github_native.as_deref())
                    .map(str::to_owned)
            })
            == Some("milestone".to_string());
        let source_is_milestone =
            self.store_of_path(&doc_path, config) == Some(StoreBackend::GithubMilestones);
        let required_store = is_milestone_rel.then_some({
            if source_is_milestone {
                StoreBackend::GithubIssues
            } else {
                StoreBackend::GithubMilestones
            }
        });

        let mut candidates: Vec<(String, PathBuf)> = self
            .store
            .all_docs()
            .iter()
            .filter(|d| d.path != doc_path)
            .filter(|d| {
                let store = config.type_by_name(d.doc_type.as_str()).map(|t| &t.store);
                match &required_store {
                    Some(want) => store == Some(want),
                    None => store != Some(&StoreBackend::GithubMilestones),
                }
            })
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
        let target_path = match self.link_editor.results.get(selected) {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        let from = self.link_editor.doc_path.to_string_lossy().to_string();
        let to = target_path.to_string_lossy().to_string();
        let rel_type = self
            .rel_types
            .get(self.link_editor.rel_type_index)
            .map(|s| s.as_str())
            .unwrap_or("related-to")
            .to_string();

        let outcome = match crate::engine::ops::link::link_with_config(
            root,
            &self.store,
            &from,
            &rel_type,
            &to,
            &*self.fs,
            Some(config),
        ) {
            Ok(o) => o,
            Err(e) => {
                self.link_editor.error = Some(e.to_string());
                return Err(e);
            }
        };
        // Inverse keywords flip direction, so the modified file is the target,
        // not the viewed doc. Reload whichever file actually changed.
        self.store.reload_file(root, &outcome.source, &*self.fs)?;
        self.filtered_docs_cache = None;
        self.build_doc_tree();
        self.link_editor.error = None;
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

/// Test-only observable-state fingerprint and per-context seeding, used by the
/// keybind parity test (`keybinds.rs`). `pub(crate)` so the sibling module's test
/// can build a seeded `App` and detect whether a keypress mutated it. None of this
/// compiles into the production binary.
#[cfg(test)]
impl App {
    /// A string capturing every observable `App` field a `handle_*_key` handler
    /// can mutate. A keypress is "handled" iff it changes this fingerprint. Errs
    /// toward including MORE fields: a missing mutation sink would let a real
    /// keypress read as a no-op (a false parity PASS, the dangerous direction).
    pub(crate) fn key_fingerprint(&self) -> String {
        // Expanded-parent set: size + sorted contents (Space toggles membership).
        let mut expanded: Vec<String> = self
            .expanded_parents
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        expanded.sort();

        // Zone editor inner state (pane / cursor / both lists), or None.
        let zone = self.settings_zone_editor.as_ref().map(|z| {
            format!(
                "{:?}|{}|{:?}|{:?}",
                z.pane, z.cursor, z.selected, z.available
            )
        });
        // Variant picker inner state (selected index), or None.
        let variant = self
            .settings_variant_picker
            .as_ref()
            .map(|p| format!("{}|{:?}", p.selected, p.path));

        #[cfg(feature = "agent")]
        let agent = format!(
            "agent_dialog.active={} sel={} text={:?} agent_idx={} resume={} interactive={}",
            self.agent_dialog.active,
            self.agent_dialog.selected_index,
            self.agent_dialog.text_input,
            self.agent_selected_index,
            self.resume_request.is_some(),
            self.interactive_request.is_some(),
        );
        #[cfg(not(feature = "agent"))]
        let agent = String::new();

        format!(
            concat!(
                "view_mode={:?} selected_type={} selected_doc={} doc_list_offset={} ",
                "graph_selected={} scroll_offset={} wrap_mode={} show_help={} help_scroll={} ",
                "search_mode={} fullscreen_doc={} should_quit={} preview_tab={:?} ",
                "filter_focused={:?} filter_status={:?} filter_tag={:?} selected_relation={} ",
                "expanded_len={} expanded={:?} ",
                "editor_request={} open_request={} open_message={} config_reload_request={} fix_request={} ",
                "create_form.active={} create_form.field={:?} create_form.title={} ",
                "create_form.author={} create_form.tags={} create_form.related={} ",
                "delete_confirm.active={} override_key_prompt.active={} override_input={} ",
                "settings_delete_confirm.active={} settings_impact_confirm.active={} ",
                "status_picker.active={} status_picker.selected={} ",
                "link_editor.active={} link_editor.selected={} link_editor.query_len={} ",
                "link_editor.rel_type_index={} link_editor.results_len={} ",
                "provenance_editor.active={} provenance_buf_len={} ",
                "gh_conflict={} show_warnings={} warnings_selected={} ",
                "search_query_len={} search_selected={} ",
                "settings_editing={} settings_edit_input_len={} settings_dirty={} ",
                "settings_category={} settings_field={} settings_entry={} settings_drill={:?} ",
                "settings_quit_prompt.active={} zone={:?} variant={:?} ",
                "scaffold_offer={} settings_footer_error={} settings_edit_error={} ",
                "graph_sort_col={} graph_sort_rev={} {}",
            ),
            self.view_mode,
            self.selected_type,
            self.selected_doc,
            self.doc_list_offset,
            self.graph_selected,
            self.scroll_offset,
            self.wrap_mode,
            self.show_help,
            self.help_scroll,
            self.search_mode,
            self.fullscreen_doc,
            self.should_quit,
            self.preview_tab,
            self.filter_focused,
            self.filter_status,
            self.filter_tag,
            self.selected_relation,
            expanded.len(),
            expanded,
            self.editor_request.is_some(),
            self.open_request.is_some(),
            self.open_message.is_some(),
            self.config_reload_request,
            self.fix_request,
            self.create_form.active,
            self.create_form.focused_field,
            self.create_form.title,
            self.create_form.author,
            self.create_form.tags,
            self.create_form.related,
            self.delete_confirm.active,
            self.override_key_prompt.active,
            self.override_key_prompt.input,
            self.settings_delete_confirm.active,
            self.settings_impact_confirm.active,
            self.status_picker.active,
            self.status_picker.selected,
            self.link_editor.active,
            self.link_editor.selected,
            self.link_editor.query.len(),
            self.link_editor.rel_type_index,
            self.link_editor.results.len(),
            self.provenance_editor.active,
            self.provenance_editor.input.len(),
            self.gh_conflict_message.is_some(),
            self.show_warnings,
            self.warnings_selected,
            self.search_query.len(),
            self.search_selected,
            self.settings_editing,
            self.settings_edit_input.len(),
            self.settings_dirty,
            self.settings_category,
            self.settings_field,
            self.settings_entry,
            self.settings_drill,
            self.settings_quit_prompt.active,
            zone,
            variant,
            self.settings_scaffold_offer.is_some(),
            self.settings_footer_error.is_some(),
            self.settings_edit_error.is_some(),
            self.graph_sort_col,
            self.graph_sort_rev,
            agent,
        )
    }
}

/// Test-only per-`KeyContext` seeding for the keybind parity test. Each seed is
/// built so that EVERY key the registry documents for that context is "live": no
/// boundary no-ops (lists carry >= 3 items with the cursor in the middle), and
/// every doc-dependent action has a real doc behind it. `pub(crate)` so the
/// parity test in `keybinds.rs` can call it. Not compiled into production.
#[cfg(test)]
pub(crate) mod parity_seed {
    use super::*;
    use crate::tui::views::keybinds::KeyContext;
    use std::sync::OnceLock;
    use tempfile::TempDir;

    /// Every `Picker` constructor probes the terminal and, under tmux, spawns
    /// `tmux set -p allow-passthrough on`. Pay that once per test binary.
    pub(crate) fn test_picker() -> ratatui_image::picker::Picker {
        static PICKER: OnceLock<ratatui_image::picker::Picker> = OnceLock::new();
        PICKER
            .get_or_init(ratatui_image::picker::Picker::halfblocks)
            .clone()
    }

    /// Markdown for one seeded doc, with an optional `related:` block.
    fn doc_md(id: &str, doc_type: &str, related: &str) -> String {
        let related_block = if related.is_empty() {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{id} title\"\ntype: {doc_type}\nstatus: draft\nauthor: tester\ndate: 2026-01-01\ntags:\n  - alpha\n{related_block}\n---\n\n{id} body line 1\n{id} body line 2\n"
        )
    }

    /// A bare `App` over a real, empty `Store` rooted at `tmp`, with the default
    /// config applied (7 doc types). The TempDir is returned so the root outlives
    /// the App (handlers that save touch `<root>/.lazyspec.toml`).
    pub(crate) fn bare_app() -> (TempDir, App) {
        let tmp = TempDir::new().unwrap();
        let store = Store::load(tmp.path(), &Config::default()).unwrap();
        let (tx, _rx) = crossbeam_channel::unbounded();
        let (search_tx, _search_rx) = crossbeam_channel::unbounded();
        #[cfg(feature = "agent")]
        let agent_spawner = AgentSpawner::new(store.root());
        let config = Config::default();
        let mut app = App {
            fs: Box::new(crate::engine::fs::RealFileSystem),
            store,
            selected_type: 0,
            selected_doc: 0,
            doc_types: Vec::new(),
            graph_anchor: GraphAnchor::All,
            graph_sort_col: config.ui.graph.sort.clone(),
            graph_sort_rev: false,
            should_quit: false,
            fullscreen_doc: false,
            scroll_offset: 0,
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_pending: false,
            search_generation: 0,
            search_tx,
            show_help: false,
            help_scroll: 0,
            help_max_scroll: 0,
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
            graph_offset: 0,
            graph_list_height: 0,
            editor_request: None,
            open_request: None,
            open_message: None,
            filter_focused: FilterField::Status,
            filter_status: None,
            filter_tag: None,
            available_tags: Vec::new(),
            available_statuses: Vec::new(),
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
            gh_fetch_warnings: Vec::new(),
            fix_request: false,
            config_reload_request: false,
            fix_result: None,
            doc_list_offset: 0,
            doc_list_height: 10,
            fullscreen_height: 20,
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
            picker: parity_seed::test_picker(),
            image_states: HashMap::new(),
            image_dimensions_cache: HashMap::new(),
            ascii_diagrams: false,
            diagram_blocks_cache: None,
            filtered_docs_cache: None,
            git_branch: None,
            git_status_cache: GitStatusCache::unqueried(tmp.path()),
            gh_conflict_message: None,
            gh_push_in_flight: Arc::new(AtomicBool::new(false)),
            refresh_in_flight: false,
            last_sync: None,
            gh_issue_map_stale: false,
            status_bar_enabled: true,
            status_bar_components: StatusBarComponents::default(),
            rel_types: config.relationship_keywords(),
            settings_category: 0,
            settings_entry: 0,
            settings_drill: None,
            settings_field: 0,
            settings_buffer: config.clone(),
            settings_dirty: false,
            settings_editing: false,
            settings_edit_input: String::new(),
            settings_edit_error: None,
            settings_footer_error: None,
            settings_quit_prompt: SettingsQuitPrompt::new(),
            settings_scaffold_offer: None,
            settings_delete_confirm: SettingsDeleteConfirm::new(),
            settings_impact_confirm: SettingsImpactConfirm::new(),
            override_key_prompt: OverrideKeyPrompt::new(),
            settings_zone_editor: None,
            settings_variant_picker: None,
            frame_idx: 0,
        };
        app.apply_config(&config);
        (tmp, app)
    }

    /// Write a doc set into `app`'s root and reload the store through the public
    /// `Store::load` (so forward/reverse links, children, and parents are built
    /// the real way). The set: 5 top-level rfcs; a `convention` subdirectory type
    /// (`subdirectory = true` in the default config) holding an `index.md` parent
    /// plus a child, so the rfc... actually the convention type carries the
    /// parent/child pair (so Space has a real parent to toggle); and a
    /// story+iteration relation chain off RFC-002 (so Relations/Graph can walk).
    pub(crate) fn populate_docs(app: &mut App) {
        let root = app.store.root.clone();
        let write = |rel: &str, contents: String| {
            let full = root.join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        };

        for i in 1..=5 {
            write(
                &format!("docs/rfcs/RFC-{i:03}-a.md"),
                doc_md(&format!("RFC-{i:03}"), "rfc", ""),
            );
        }
        // A relation chain so the selected rfc has children/related to walk.
        write(
            "docs/stories/STORY-001-s.md",
            doc_md("STORY-001", "story", "  - implements: RFC-002"),
        );
        write(
            "docs/iterations/ITERATION-001-i.md",
            doc_md("ITERATION-001", "iteration", "  - implements: STORY-001"),
        );
        // The `convention` type is `subdirectory = true`: a folder with an
        // index.md becomes a parent of its sibling .md files. Three such folders
        // give the convention docs list >= 3 top-level PARENT nodes -- so in that
        // type both `j`/`k` move (middle cursor) AND Space has a parent to toggle.
        for (i, slug) in ["style", "naming", "review"].iter().enumerate() {
            let n = i + 1;
            write(
                &format!("docs/convention/{slug}/index.md"),
                doc_md(&format!("CONVENTION-{n:03}"), "convention", ""),
            );
            write(
                &format!("docs/convention/{slug}/DICTUM-{n:03}-d.md"),
                doc_md(&format!("DICTUM-{n:03}"), "dictum", ""),
            );
        }

        app.store = Store::load(&root, &Config::default()).unwrap();
        app.build_doc_tree();
    }

    /// Build a fully-seeded `App` for `ctx`, returning the owning TempDir and the
    /// `Config` the parity test must pass to `handle_key` (it differs from the
    /// default only for the `agent`-feature Types `a` case, which needs the
    /// selected doc's type to carry an agent so `open_agent_dialog` opens).
    pub(crate) fn seed(ctx: KeyContext) -> (TempDir, App, Config) {
        let (tmp, mut app) = bare_app();
        // `config` is mutated only under the `agent` feature (Types `a` setup).
        #[cfg_attr(not(feature = "agent"), allow(unused_mut))]
        let mut config = Config::default();
        match ctx {
            KeyContext::GhConflict => {
                app.gh_conflict_message = Some("boom".to_string());
            }
            KeyContext::Warnings => {
                app.validation_warnings =
                    vec!["w1".to_string(), "w2".to_string(), "w3".to_string()];
                app.show_warnings = true;
                app.warnings_selected = 1; // middle: j and k both move
            }
            KeyContext::CreateForm => {
                app.open_create_form();
                app.create_form.title = "seed".to_string();
            }
            KeyContext::DeleteConfirm => {
                // A real doc behind the confirm so Enter (confirm_delete) lands a
                // deletion; RFC-005 is a leaf with no inbound relations.
                populate_docs(&mut app);
                app.delete_confirm.active = true;
                app.delete_confirm.doc_path = PathBuf::from("docs/rfcs/RFC-005-a.md");
            }
            KeyContext::OverrideKeyPrompt => {
                app.override_key_prompt.active = true;
                app.override_key_prompt.input = "spec/x".to_string();
            }
            KeyContext::SettingsDeleteConfirm => {
                app.settings_delete_confirm.active = true;
            }
            KeyContext::SettingsImpact => {
                app.settings_impact_confirm.active = true;
            }
            KeyContext::StatusPicker => {
                // A real doc behind the picker so Enter (confirm_status_change)
                // writes a status and reloads (changing the fingerprint). The
                // seeded doc is at `draft`; select `review` (index 1) so the move
                // is a valid lifecycle edge and the gate admits it.
                populate_docs(&mut app);
                app.status_picker.active = true;
                app.status_picker.states = crate::engine::config::default_lifecycle().states;
                app.status_picker.selected = 1; // draft -> review (a declared edge)
                app.status_picker.doc_path = PathBuf::from("docs/rfcs/RFC-001-a.md");
            }
            KeyContext::LinkEditor => {
                populate_docs(&mut app);
                app.link_editor.active = true;
                app.link_editor.doc_path = PathBuf::from("docs/rfcs/RFC-001-a.md");
                // A non-empty query so Backspace pops a char and re-filters the
                // result set (an empty query would make Backspace a no-op). "rfc"
                // still matches >= 2 RFC docs, so j/k/Enter stay live too.
                app.link_editor.query = "rfc".to_string();
                app.link_editor.rel_type_index = 0;
                app.update_link_search(&config); // results non-empty (other docs exist)
                app.link_editor.selected = 1; // middle: j and k both move
            }
            KeyContext::ProvenanceEditor => {
                // A real doc behind the editor so Enter (submit_provenance) writes
                // a citation and closes (changing the fingerprint).
                populate_docs(&mut app);
                app.provenance_editor.active = true;
                app.provenance_editor.doc_path = PathBuf::from("docs/rfcs/RFC-001-a.md");
                app.provenance_editor.input = "cite".to_string();
            }
            #[cfg(feature = "agent")]
            KeyContext::AgentDialog => {
                use crate::tui::state::forms::{AgentAction, AgentDialog};
                app.agent_dialog = AgentDialog {
                    active: true,
                    selected_index: 0,
                    actions: vec![AgentAction::Custom, AgentAction::Custom],
                    missing: Vec::new(),
                    doc_path: PathBuf::from("docs/rfcs/RFC-001-a.md"),
                    doc_title: "t".to_string(),
                    text_input: None,
                };
            }
            #[cfg(feature = "agent")]
            KeyContext::AgentTextInput => {
                use crate::tui::state::forms::{AgentAction, AgentDialog};
                app.agent_dialog = AgentDialog {
                    active: true,
                    selected_index: 0,
                    actions: vec![AgentAction::Custom],
                    missing: Vec::new(),
                    doc_path: PathBuf::from("docs/rfcs/RFC-001-a.md"),
                    doc_title: "t".to_string(),
                    text_input: Some("draft".to_string()),
                };
            }
            KeyContext::Search => {
                populate_docs(&mut app);
                app.search_mode = true;
                app.search_query = "title".to_string();
                app.run_search_now(); // results non-empty (search is async in prod)
                app.search_selected = 1; // middle: Up and Down both move
            }
            KeyContext::Fullscreen => {
                populate_docs(&mut app);
                app.fullscreen_doc = true;
                app.fullscreen_height = 20;
                app.scroll_offset = 10; // middle: j/k/Ctrl-d/Ctrl-u all change it
            }
            KeyContext::Types => {
                populate_docs(&mut app);
                app.view_mode = ViewMode::Types;
                // The `convention` type (index 5 of 7) holds three subdirectory
                // PARENT nodes. Index 5 is interior (h: 5->4, l: 5->6 both move).
                let conv_idx = app
                    .doc_types
                    .iter()
                    .position(|t| t.as_str() == "convention")
                    .expect("default config has a convention type");
                app.selected_type = conv_idx;
                app.build_doc_tree();
                // Cursor on the middle parent: Space toggles it and j/k both move
                // (>= 3 top-level parent nodes).
                let parents: Vec<usize> = app
                    .doc_tree
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.is_parent)
                    .map(|(i, _)| i)
                    .collect();
                assert!(
                    parents.len() >= 3,
                    "Types seeding needs >= 3 parent nodes, got {}",
                    parents.len()
                );
                app.selected_doc = parents[1];
                app.preview_tab = PreviewTab::Preview;

                // (agent) Make `a` live: the selected doc's type (convention)
                // must carry an agent whose template is loaded, so
                // open_agent_dialog resolves at least one action and opens.
                #[cfg(feature = "agent")]
                {
                    use crate::engine::prompt::{AgentPrompt, RunMode};
                    let conv_name = app.doc_types[conv_idx].as_str().to_string();
                    if let Some(t) = config
                        .documents
                        .types
                        .iter_mut()
                        .find(|t| t.name == conv_name)
                    {
                        t.agents = vec!["review".to_string()];
                    }
                    app.agent_prompts = vec![AgentPrompt {
                        name: "review".to_string(),
                        description: "review".to_string(),
                        mode: RunMode::Headless,
                        allowed_tools: None,
                        body_template: "body".to_string(),
                    }];
                }
            }
            KeyContext::Filters => {
                populate_docs(&mut app);
                app.view_mode = ViewMode::Filters;
                app.enter_filters_mode();
                app.filter_focused = FilterField::Tag; // Tab/BackTab change it; h/l cycle tag values
                app.filtered_docs_cache = None;
                let _ = app.filtered_docs_count();
                app.selected_doc = 1; // middle of filtered list: j/k both move
                app.preview_tab = PreviewTab::Preview;
            }
            KeyContext::Graph => {
                populate_docs(&mut app);
                app.view_mode = ViewMode::Graph;
                app.rebuild_graph();
                // Ensure >= 3 nodes with the cursor in the middle.
                assert!(
                    app.graph_nodes.len() >= 3,
                    "graph seeding needs >= 3 nodes, got {}",
                    app.graph_nodes.len()
                );
                app.graph_selected = 1;
                // A measured viewport so Ctrl-d/Ctrl-u (half-page) move the cursor.
                app.graph_list_height = 4;
            }
            #[cfg(feature = "agent")]
            KeyContext::Agents => {
                use crate::tui::agent::{AgentRecord, AgentStatus};
                app.view_mode = ViewMode::Agents;
                app.agent_spawner.records = (0..3)
                    .map(|i| AgentRecord {
                        session_id: format!("s{i}"),
                        doc_title: format!("doc {i}"),
                        doc_path: PathBuf::from(format!("docs/rfcs/RFC-{i:03}-a.md")),
                        action: "a".to_string(),
                        status: AgentStatus::Complete, // not Running, so `r` resumes
                        started_at: "t".to_string(),
                        finished_at: None,
                    })
                    .collect();
                app.agent_selected_index = 1; // middle: j/k both move
            }
            KeyContext::Settings => {
                populate_docs(&mut app);
                app.view_mode = ViewMode::Settings;
                // Category 1 (Document Types) is a collection: in its entry-list
                // (drill = None) j/k navigate entries, `n` seeds, `d` deletes.
                // Dirtying up-front makes Esc/q open the quit prompt (they would
                // otherwise no-op when clean and undrilled).
                app.settings_category = 1;
                app.settings_drill = None;
                app.settings_entry = 1; // middle of >= 3 type entries: j/k both move
                app.settings_dirty = true;
            }
            KeyContext::SettingsEditing => {
                app.view_mode = ViewMode::Settings;
                app.settings_category = 0; // General: field 0 is a Text field
                app.settings_field = 0;
                app.settings_editing = true;
                app.settings_edit_input = "abc".to_string();
            }
            KeyContext::SettingsQuitPrompt => {
                app.view_mode = ViewMode::Settings;
                app.settings_dirty = true;
                app.settings_quit_prompt.active = true;
            }
            KeyContext::SettingsZoneEditor => {
                use crate::tui::state::forms::{FieldPath, ZoneOrderingEditor, ZonePane};
                app.view_mode = ViewMode::Settings;
                // >= 2 selected + >= 2 available, cursor in the middle of selected
                // (so K/J move-up/down both act and j/k both move).
                let mut z = ZoneOrderingEditor::new(
                    FieldPath::StatusbarLeft,
                    Some(&vec![
                        "branch".to_string(),
                        "filter".to_string(),
                        "sync".to_string(),
                    ]),
                    &[],
                );
                z.pane = ZonePane::Selected;
                z.cursor = 1;
                assert!(z.available.len() >= 2, "zone editor needs >= 2 available");
                app.settings_zone_editor = Some(z);
            }
            KeyContext::SettingsVariantPicker => {
                use crate::tui::state::forms::{FieldPath, SettingsVariantPicker};
                app.view_mode = ViewMode::Settings;
                app.settings_variant_picker = Some(SettingsVariantPicker::new(
                    FieldPath::Naming,
                    &["incremental", "sqids", "reserved"],
                    1, // middle: j and k both move
                ));
            }
            KeyContext::SettingsScaffoldOffer => {
                use crate::tui::state::forms::FieldPath;
                app.view_mode = ViewMode::Settings;
                // `g` jumps to the required-empty field; a Some(field) makes `g` act.
                app.settings_scaffold_offer = Some(ScaffoldResult {
                    inserted: ConfigDep::NumberingSqids,
                    required_empty_field: Some(FieldPath::Naming),
                });
            }
        }
        (tmp, app, config)
    }
}

#[cfg(test)]
mod tests {
    use super::parity_seed::{bare_app, populate_docs};
    use super::*;
    use crate::engine::config::TypeDef;
    use crate::engine::store::Store;
    use crate::engine::traversal::TraversalWalk;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn make_dummy_node(index: usize) -> DocListNode {
        DocListNode {
            path: PathBuf::from(format!("docs/rfcs/RFC-{:03}.md", index)),
            id: format!("RFC-{:03}", index),
            title: format!("Doc {}", index),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
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
            traversal_walk: TraversalWalk::default(),
            body_cache: std::sync::Mutex::new(HashMap::new()),
        };

        let (tx, _rx) = crossbeam_channel::unbounded();
        let (search_tx, _search_rx) = crossbeam_channel::unbounded();
        let config = Config::default();

        #[cfg(feature = "agent")]
        let agent_spawner = AgentSpawner::new(store.root());

        let app = App {
            fs: Box::new(crate::engine::fs::RealFileSystem),
            store,
            selected_type: 0,
            selected_doc: 0,
            doc_types: vec![DocType::new("rfc")],
            graph_anchor: GraphAnchor::All,
            graph_sort_col: config.ui.graph.sort.clone(),
            graph_sort_rev: false,
            should_quit: false,
            fullscreen_doc: false,
            scroll_offset: 0,
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_pending: false,
            search_generation: 0,
            search_tx,
            show_help: false,
            help_scroll: 0,
            help_max_scroll: 0,
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
            graph_offset: 0,
            graph_list_height: 0,
            editor_request: None,
            open_request: None,
            open_message: None,
            filter_focused: FilterField::Status,
            filter_status: None,
            filter_tag: None,
            available_tags: Vec::new(),
            available_statuses: Vec::new(),
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
            gh_fetch_warnings: Vec::new(),
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
            picker: parity_seed::test_picker(),
            image_states: HashMap::new(),
            image_dimensions_cache: HashMap::new(),
            ascii_diagrams: false,
            diagram_blocks_cache: None,
            filtered_docs_cache: None,
            git_branch: None,
            git_status_cache: GitStatusCache::unqueried(Path::new(".")),
            gh_conflict_message: None,
            gh_push_in_flight: Arc::new(AtomicBool::new(false)),
            refresh_in_flight: false,
            last_sync: None,
            gh_issue_map_stale: false,
            status_bar_enabled: true,
            status_bar_components: StatusBarComponents::default(),
            rel_types: Config::default().relationship_keywords(),
            settings_category: 0,
            settings_entry: 0,
            settings_drill: None,
            settings_field: 0,
            settings_buffer: Config::default(),
            settings_dirty: false,
            settings_editing: false,
            settings_edit_input: String::new(),
            settings_edit_error: None,
            settings_footer_error: None,
            settings_quit_prompt: SettingsQuitPrompt::new(),
            settings_scaffold_offer: None,
            settings_delete_confirm: SettingsDeleteConfirm::new(),
            settings_impact_confirm: SettingsImpactConfirm::new(),
            override_key_prompt: OverrideKeyPrompt::new(),
            settings_zone_editor: None,
            settings_variant_picker: None,
            frame_idx: 0,
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
            traversal_walk: TraversalWalk::default(),
            body_cache: std::sync::Mutex::new(HashMap::new()),
        };

        let meta_a = DocMeta {
            path: PathBuf::from("docs/rfcs/RFC-001.md"),
            title: "First".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "test".to_string(),
            date: Utc::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "RFC-001".to_string(),
        };
        let meta_b = DocMeta {
            path: PathBuf::from("docs/rfcs/RFC-001-dup.md"),
            title: "Duplicate".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "test".to_string(),
            date: Utc::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
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
    fn refresh_validation_folds_in_gh_fetch_warnings() {
        use crate::engine::config::Config;
        let config = Config::default();
        let mut app = make_test_app(0);

        // Warnings carried on the CacheRefresh event survive re-validation
        // (which otherwise rebuilds validation_warnings from scratch).
        app.gh_fetch_warnings = vec!["could not refresh gh schema snapshot".to_string()];
        app.refresh_validation(&config);
        assert!(
            app.validation_warnings
                .iter()
                .any(|w| w.contains("could not refresh gh schema snapshot")),
            "gh fetch warnings should appear in the panel after refresh: {:?}",
            app.validation_warnings
        );

        // Empty gh warnings add nothing spurious.
        app.gh_fetch_warnings = vec![];
        app.refresh_validation(&config);
        assert!(
            !app.validation_warnings
                .iter()
                .any(|w| w.contains("gh schema snapshot")),
            "no gh warning should remain once the source is cleared"
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
    fn status_filter_cycle_spans_configured_statuses() {
        let mut app = make_test_app(0);
        app.available_statuses = vec!["draft".into(), "parked".into(), "done".into()];
        app.filter_focused = FilterField::Status;
        app.filter_status = None;

        app.cycle_filter_value_next();
        assert_eq!(
            app.filter_status.as_ref().map(Status::as_str),
            Some("draft")
        );
        app.cycle_filter_value_next();
        assert_eq!(
            app.filter_status.as_ref().map(Status::as_str),
            Some("parked")
        );
        app.cycle_filter_value_next();
        assert_eq!(app.filter_status.as_ref().map(Status::as_str), Some("done"));
        app.cycle_filter_value_next();
        assert_eq!(app.filter_status, None);

        app.cycle_filter_value_prev();
        assert_eq!(app.filter_status.as_ref().map(Status::as_str), Some("done"));
    }

    #[test]
    fn apply_config_builds_status_union_first_seen_order() {
        let mut app = make_test_app(0);
        let config = Config::default();
        app.apply_config(&config);
        // default config types all share the default lifecycle states.
        assert_eq!(
            app.available_statuses,
            crate::engine::config::default_lifecycle().states
        );
    }

    #[test]
    fn status_picker_navigates_all_seven_statuses() {
        let mut app = make_test_app(5);
        app.status_picker.active = true;
        app.status_picker.selected = 0;
        app.status_picker.states = crate::engine::config::default_lifecycle().states;

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

    // AC7 (ITER-229 / STORY-170): a failing status change keeps the picker OPEN
    // and records the error on the overlay state. `draft -> accepted` is not a
    // declared edge in the default lifecycle, so the gate bails.
    #[test]
    fn confirm_status_change_failure_keeps_picker_open_with_error() {
        let (_tmp, mut app) = bare_app();
        populate_docs(&mut app);
        let root = app.store.root.clone();
        let config = Config::default();

        app.status_picker.active = true;
        app.status_picker.states = crate::engine::config::default_lifecycle().states;
        app.status_picker.doc_path = PathBuf::from("docs/rfcs/RFC-001-a.md");
        // index 2 = "accepted": no edge from draft, so the transition gate rejects.
        app.status_picker.selected = 2;

        let result = app.confirm_status_change(&root, &config);
        assert!(result.is_err(), "an invalid transition must return Err");
        assert!(
            app.status_picker.active,
            "the picker stays open on a failed status change"
        );
        assert!(
            app.status_picker.error.is_some(),
            "the failure is surfaced on status_picker.error"
        );
    }

    // AC7: a succeeding status change closes the picker and clears any error.
    // RFC-001 is seeded at `draft`; `draft -> review` (index 1) is a valid edge.
    #[test]
    fn confirm_status_change_success_closes_picker_and_clears_error() {
        let (_tmp, mut app) = bare_app();
        populate_docs(&mut app);
        let root = app.store.root.clone();
        let config = Config::default();

        app.status_picker.active = true;
        app.status_picker.states = crate::engine::config::default_lifecycle().states;
        app.status_picker.doc_path = PathBuf::from("docs/rfcs/RFC-001-a.md");
        app.status_picker.selected = 1; // draft -> review (a declared edge)
                                        // A stale error must not survive a successful change.
        app.status_picker.error = Some("stale".to_string());

        let result = app.confirm_status_change(&root, &config);
        assert!(result.is_ok(), "a valid transition succeeds: {result:?}");
        assert!(
            !app.status_picker.active,
            "the picker closes on a successful status change"
        );
        assert!(
            app.status_picker.error.is_none(),
            "the error is cleared on success"
        );
    }

    // AC7: a failing link keeps the link editor OPEN and records the error.
    // A doc_path that resolves to no store doc makes `link_with_config` fail in
    // `resolve_to_path`.
    #[test]
    fn confirm_link_failure_keeps_editor_open_with_error() {
        let (_tmp, mut app) = bare_app();
        populate_docs(&mut app);
        let root = app.store.root.clone();
        let config = Config::default();

        app.link_editor.active = true;
        // No such doc in the store: resolve_to_path errors before any write.
        app.link_editor.doc_path = PathBuf::from("docs/rfcs/RFC-999-nope.md");
        app.link_editor.results = vec![PathBuf::from("docs/rfcs/RFC-002-a.md")];
        app.link_editor.selected = 0;
        app.link_editor.rel_type_index = 0;

        let result = app.confirm_link(&root, &config);
        assert!(result.is_err(), "an unresolvable source must return Err");
        assert!(
            app.link_editor.active,
            "the link editor stays open on a failed link"
        );
        assert!(
            app.link_editor.error.is_some(),
            "the failure is surfaced on link_editor.error"
        );
    }

    // AC7: a succeeding link closes the editor and clears any error. RFC-001
    // links to RFC-002 via the first relation keyword.
    #[test]
    fn confirm_link_success_closes_editor_and_clears_error() {
        let (_tmp, mut app) = bare_app();
        populate_docs(&mut app);
        let root = app.store.root.clone();
        let config = Config::default();

        app.link_editor.active = true;
        app.link_editor.doc_path = PathBuf::from("docs/rfcs/RFC-001-a.md");
        app.link_editor.results = vec![PathBuf::from("docs/rfcs/RFC-002-a.md")];
        app.link_editor.selected = 0;
        app.link_editor.rel_type_index = 0;
        app.link_editor.error = Some("stale".to_string());

        let result = app.confirm_link(&root, &config);
        assert!(result.is_ok(), "a resolvable link succeeds: {result:?}");
        assert!(
            !app.link_editor.active,
            "the link editor closes on a successful link"
        );
        assert!(
            app.link_editor.error.is_none(),
            "the error is cleared on success"
        );
    }

    /// Config for the store-aware milestone vocabulary tests (ITER-230): an
    /// `issue` type in the github-issues store, a `milestone` type in the
    /// github-milestones store, a `targets` rel mapped onto the `milestone`
    /// native edge, and an ordinary `related-to` rel.
    fn milestone_vocab_config() -> Config {
        let mut config = Config::default();
        let template = config.documents.types[0].clone();
        config.documents.types = vec![
            TypeDef {
                name: "issue".to_string(),
                plural: "issues".to_string(),
                dir: "docs/issues".to_string(),
                prefix: "ISSUE".to_string(),
                store: StoreBackend::GithubIssues,
                ..template.clone()
            },
            TypeDef {
                name: "milestone".to_string(),
                plural: "milestones".to_string(),
                dir: "docs/milestones".to_string(),
                prefix: "MILESTONE".to_string(),
                store: StoreBackend::GithubMilestones,
                ..template
            },
        ];
        config.relationships = vec![
            RelationshipDef {
                name: "targets".to_string(),
                inverse: None,
                github_native: Some("milestone".to_string()),
                traversal: None,
            },
            RelationshipDef {
                name: "related-to".to_string(),
                inverse: None,
                github_native: None,
                traversal: None,
            },
        ];
        config
    }

    /// Insert a `DocMeta` of `doc_type` at `path` straight into the store, so the
    /// milestone-vocab tests can stand up a mixed-store doc set without disk I/O.
    fn insert_doc(app: &mut App, path: &str, id: &str, doc_type: &str) {
        use chrono::NaiveDate;
        let path = PathBuf::from(path);
        app.store.docs.insert(
            path.clone(),
            DocMeta {
                path,
                title: id.to_string(),
                doc_type: DocType::new(doc_type),
                status: Status::new("draft"),
                id: id.to_string(),
                tags: Vec::new(),
                provenance: Vec::new(),
                author: String::new(),
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                related: Vec::new(),
                validate_ignore: false,
                virtual_doc: false,
                assignee: None,
                attributes: Default::default(),
            },
        );
    }

    // AC7: the candidate list is scoped to the selected rel's github_native edge.
    // A `targets` (github_native="milestone") rel offers ONLY milestone-store
    // docs; any other rel EXCLUDES every milestone-store doc.
    #[test]
    fn update_link_search_scopes_candidates_by_selected_rel_native() {
        let mut app = make_test_app(0);
        let config = milestone_vocab_config();
        app.apply_config(&config);

        insert_doc(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");
        insert_doc(&mut app, "docs/issues/ISSUE-002.md", "ISSUE-002", "issue");
        insert_doc(
            &mut app,
            "docs/milestones/MILESTONE-001.md",
            "MILESTONE-001",
            "milestone",
        );

        // Viewed doc is ISSUE-001 (a valid source). rel_types is the global list
        // ["targets", "related-to"].
        app.link_editor.doc_path = PathBuf::from("docs/issues/ISSUE-001.md");

        // `targets` (index 0, github_native="milestone") -> only milestone docs.
        let targets_index = app.rel_types.iter().position(|r| r == "targets").unwrap();
        app.link_editor.rel_type_index = targets_index;
        app.update_link_search(&config);
        let ids: Vec<String> = ids_for_paths(&app, &app.link_editor.results);
        assert_eq!(
            ids,
            vec!["MILESTONE-001".to_string()],
            "a milestone-native rel must offer only github-milestones docs"
        );

        // `related-to` (ordinary) -> exclude all milestone docs.
        let related_index = app
            .rel_types
            .iter()
            .position(|r| r == "related-to")
            .unwrap();
        app.link_editor.rel_type_index = related_index;
        app.update_link_search(&config);
        let ids: Vec<String> = ids_for_paths(&app, &app.link_editor.results);
        assert_eq!(
            ids,
            vec!["ISSUE-002".to_string()],
            "an ordinary rel must exclude every github-milestones doc"
        );
    }

    // From a github-milestones VIEWED doc the only legal relation is the inverse
    // of the milestone-native rel (`targeted-by`), and its other end must be a
    // github-issues doc. The candidate list must therefore exclude every
    // non-issue doc (filesystem specs, other milestones), not merely every
    // milestone -- this is the inverse-direction counterpart to the forward
    // issue -> milestone scoping.
    #[test]
    fn update_link_search_from_milestone_source_offers_only_issues() {
        let mut app = make_test_app(0);
        let mut config = milestone_vocab_config();
        config.relationships[0].inverse = Some("targeted-by".to_string());
        let template = config.documents.types[0].clone();
        config.documents.types.push(TypeDef {
            name: "spec".to_string(),
            plural: "specs".to_string(),
            dir: "docs/specs".to_string(),
            prefix: "SPEC".to_string(),
            store: StoreBackend::Filesystem,
            ..template
        });
        app.apply_config(&config);

        insert_doc(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");
        insert_doc(&mut app, "docs/specs/SPEC-001.md", "SPEC-001", "spec");
        insert_doc(
            &mut app,
            "docs/milestones/MILESTONE-002.md",
            "MILESTONE-002",
            "milestone",
        );

        app.doc_tree = vec![DocListNode {
            path: PathBuf::from("docs/milestones/MILESTONE-001.md"),
            id: "MILESTONE-001".to_string(),
            title: "MILESTONE-001".to_string(),
            doc_type: DocType::new("milestone"),
            status: Status::new("draft"),
            depth: 0,
            is_parent: false,
            is_virtual: false,
            has_duplicate_id: false,
        }];
        insert_doc(
            &mut app,
            "docs/milestones/MILESTONE-001.md",
            "MILESTONE-001",
            "milestone",
        );
        app.selected_doc = 0;
        app.view_mode = ViewMode::Types;

        app.open_link_editor(&config);
        assert_eq!(
            app.rel_types,
            vec!["targeted-by".to_string()],
            "a milestone source offers only the milestone-native inverse"
        );

        let ids: Vec<String> = ids_for_paths(&app, &app.link_editor.results);
        assert_eq!(
            ids,
            vec!["ISSUE-001".to_string()],
            "a milestone source must offer only github-issues candidates"
        );
    }

    // A github-milestones VIEWED doc whose native relation declares NO inverse
    // has no legal relation (a milestone can never be a source, and without an
    // inverse keyword it cannot host the flip-to-target flow either), so the
    // editor yields an empty rel-type list and flags the empty-state. The
    // `milestone_vocab_config` `targets` rel has `inverse: None`.
    #[test]
    fn open_link_editor_blocks_milestone_source_without_inverse() {
        let mut app = make_test_app(0);
        let config = milestone_vocab_config();
        app.apply_config(&config);

        insert_doc(
            &mut app,
            "docs/milestones/MILESTONE-001.md",
            "MILESTONE-001",
            "milestone",
        );
        insert_doc(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");

        // Point the viewed doc (doc_tree[selected_doc]) at the milestone.
        app.doc_tree = vec![DocListNode {
            path: PathBuf::from("docs/milestones/MILESTONE-001.md"),
            id: "MILESTONE-001".to_string(),
            title: "MILESTONE-001".to_string(),
            doc_type: DocType::new("milestone"),
            status: Status::new("draft"),
            depth: 0,
            is_parent: false,
            is_virtual: false,
            has_duplicate_id: false,
        }];
        app.selected_doc = 0;
        app.view_mode = ViewMode::Types;

        app.open_link_editor(&config);

        assert!(
            app.rel_types.is_empty(),
            "a milestone viewed doc offers no relation types"
        );
        assert!(
            app.link_editor.source_blocked,
            "the empty-state flag is set so the overlay shows the message"
        );
        assert!(
            app.link_editor.results.is_empty(),
            "no candidate is offered when the source is blocked"
        );

        // Re-opening against a non-milestone doc restores the global list.
        app.doc_tree = vec![DocListNode {
            path: PathBuf::from("docs/issues/ISSUE-001.md"),
            id: "ISSUE-001".to_string(),
            title: "ISSUE-001".to_string(),
            doc_type: DocType::new("issue"),
            status: Status::new("draft"),
            depth: 0,
            is_parent: false,
            is_virtual: false,
            has_duplicate_id: false,
        }];
        app.open_link_editor(&config);
        assert!(
            !app.link_editor.source_blocked,
            "a non-milestone source is not blocked"
        );
        assert_eq!(
            app.rel_types,
            config.relationship_keywords(),
            "the global rel-type list is restored for a non-milestone open"
        );
    }

    // A non-issue, non-milestone VIEWED doc (a filesystem spec) can never be the
    // source of the milestone-native `targets` edge (the core guard rejects it),
    // so the editor must not even offer that keyword. An issue source keeps it.
    #[test]
    fn open_link_editor_hides_milestone_keyword_for_non_issue_source() {
        let mut app = make_test_app(0);
        let mut config = milestone_vocab_config();
        // Give the milestone rel an inverse too, to confirm inverse keywords of
        // milestone-native rels are also withheld from a non-issue source.
        config.relationships[0].inverse = Some("targeted-by".to_string());
        let template = config.documents.types[0].clone();
        config.documents.types.push(TypeDef {
            name: "spec".to_string(),
            plural: "specs".to_string(),
            dir: "docs/specs".to_string(),
            prefix: "SPEC".to_string(),
            store: StoreBackend::Filesystem,
            ..template
        });
        app.apply_config(&config);

        insert_doc(&mut app, "docs/specs/SPEC-001.md", "SPEC-001", "spec");
        insert_doc(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");

        let view = |app: &mut App, path: &str, id: &str, doc_type: &str| {
            app.doc_tree = vec![DocListNode {
                path: PathBuf::from(path),
                id: id.to_string(),
                title: id.to_string(),
                doc_type: DocType::new(doc_type),
                status: Status::new("draft"),
                depth: 0,
                is_parent: false,
                is_virtual: false,
                has_duplicate_id: false,
            }];
            app.selected_doc = 0;
            app.view_mode = ViewMode::Types;
        };

        view(&mut app, "docs/specs/SPEC-001.md", "SPEC-001", "spec");
        app.open_link_editor(&config);
        assert!(
            !app.rel_types.iter().any(|r| r == "targets"),
            "a filesystem source must not be offered the milestone-native keyword"
        );
        assert!(
            !app.rel_types.iter().any(|r| r == "targeted-by"),
            "a filesystem source must not be offered a milestone-native inverse"
        );
        assert!(
            app.rel_types.iter().any(|r| r == "related-to"),
            "ordinary keywords are still offered to a filesystem source"
        );
        assert!(
            !app.link_editor.source_blocked,
            "a filesystem source with usable keywords is not blocked"
        );

        view(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");
        app.open_link_editor(&config);
        assert!(
            app.rel_types.iter().any(|r| r == "targets"),
            "a github-issues source keeps the milestone-native keyword"
        );
    }

    // From a milestone VIEWED doc whose native `targets` rel declares an inverse,
    // the editor offers that inverse (`targeted-by`) so the user can link an issue
    // *to* this milestone; confirm flips direction and writes the edge on the
    // issue. Candidates are the issues (the milestone can only be a target).
    #[test]
    fn open_link_editor_offers_inverse_on_milestone() {
        let mut app = make_test_app(0);
        let mut config = milestone_vocab_config();
        // Declare the milestone rel's inverse, mirroring the real `.lazyspec.toml`.
        config.relationships[0].inverse = Some("targeted-by".to_string());
        app.apply_config(&config);

        insert_doc(
            &mut app,
            "docs/milestones/MILESTONE-001.md",
            "MILESTONE-001",
            "milestone",
        );
        insert_doc(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");

        app.doc_tree = vec![DocListNode {
            path: PathBuf::from("docs/milestones/MILESTONE-001.md"),
            id: "MILESTONE-001".to_string(),
            title: "MILESTONE-001".to_string(),
            doc_type: DocType::new("milestone"),
            status: Status::new("draft"),
            depth: 0,
            is_parent: false,
            is_virtual: false,
            has_duplicate_id: false,
        }];
        app.selected_doc = 0;
        app.view_mode = ViewMode::Types;

        app.open_link_editor(&config);

        assert_eq!(
            app.rel_types,
            vec!["targeted-by".to_string()],
            "a milestone doc offers the inverse of its native relation"
        );
        assert!(
            !app.link_editor.source_blocked,
            "the inverse-relation flow is available, so the editor is not blocked"
        );
        assert_eq!(
            app.link_editor.results,
            vec![PathBuf::from("docs/issues/ISSUE-001.md")],
            "candidates are the issues that can target this milestone"
        );
    }

    // AC9: a core-guard rejection surfaces on link_editor.error and the editor
    // stays open with no panic and no partial write. Viewed github-issues doc +
    // ordinary rel + a milestone target is rejected by validate_milestone_relation.
    #[test]
    fn confirm_link_milestone_guard_rejection_surfaces_error() {
        let mut app = make_test_app(0);
        let config = milestone_vocab_config();
        app.apply_config(&config);
        let root = app.store.root.clone();

        insert_doc(&mut app, "docs/issues/ISSUE-001.md", "ISSUE-001", "issue");
        insert_doc(
            &mut app,
            "docs/milestones/MILESTONE-001.md",
            "MILESTONE-001",
            "milestone",
        );

        app.link_editor.active = true;
        app.link_editor.doc_path = PathBuf::from("docs/issues/ISSUE-001.md");
        app.link_editor.results = vec![PathBuf::from("docs/milestones/MILESTONE-001.md")];
        app.link_editor.selected = 0;
        // The ordinary `related-to` rel against a milestone target is illegal.
        app.link_editor.rel_type_index = app
            .rel_types
            .iter()
            .position(|r| r == "related-to")
            .unwrap();

        let result = app.confirm_link(&root, &config);
        assert!(result.is_err(), "the guard must reject the illegal triple");
        assert!(
            app.link_editor.active,
            "the editor stays open on a guard rejection"
        );
        let err = app
            .link_editor
            .error
            .as_deref()
            .expect("the guard message is surfaced on link_editor.error");
        assert!(
            err.contains("milestone docs can only be targeted by `targets`"),
            "the surfaced error carries the core guard message, got: {err}"
        );
    }

    #[test]
    fn open_status_picker_offers_only_valid_moves_from_current() {
        use crate::engine::document::DocMeta;
        use chrono::NaiveDate;

        let mut app = make_test_app(1);
        let path = PathBuf::from("docs/rfcs/RFC-001.md");
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let config = Config::default();
        let lifecycle = &config.type_by_name("rfc").unwrap().lifecycle;

        // The default lifecycle: each current status maps to itself plus the
        // edge targets out of it (including the `* -> superseded` wildcard).
        let cases = [
            ("draft", vec!["draft", "review", "superseded"]),
            (
                "review",
                vec!["review", "accepted", "rejected", "superseded"],
            ),
            ("accepted", vec!["accepted", "in-progress", "superseded"]),
            ("in-progress", vec!["in-progress", "complete", "superseded"]),
            ("complete", vec!["complete", "superseded"]),
            ("rejected", vec!["rejected", "superseded"]),
            ("superseded", vec!["superseded"]),
        ];

        for (status, expected_states) in &cases {
            app.store.docs.insert(
                path.clone(),
                DocMeta {
                    path: path.clone(),
                    title: "Test".to_string(),
                    doc_type: DocType::new("rfc"),
                    status: Status::new(status),
                    id: "RFC-001".to_string(),
                    tags: Vec::new(),
                    provenance: Vec::new(),
                    author: String::new(),
                    date,
                    related: Vec::new(),
                    validate_ignore: false,
                    virtual_doc: false,
                    assignee: None,
                    attributes: Default::default(),
                },
            );
            app.doc_tree[0].path = path.clone();
            app.selected_doc = 0;

            app.open_status_picker(&config);
            // Current status is always first (a no-op move).
            assert_eq!(app.status_picker.selected, 0);
            assert_eq!(
                app.status_picker.states, *expected_states,
                "status {status:?} should offer {expected_states:?}"
            );
            // Every offered move beyond the current must be a declared edge.
            for target in app.status_picker.states.iter().skip(1) {
                assert!(
                    lifecycle.has_edge(status, target),
                    "{status} -> {target} should be a valid edge"
                );
            }
            app.close_status_picker();
        }
    }

    // A doc whose status is unset -- a board-bound doc the authority board has
    // not placed (STORY-248) -- has no current state to lead with and no edges
    // out of one, so the picker offers the whole lifecycle instead of a blank row.
    #[test]
    fn open_status_picker_offers_every_state_for_an_unset_status() {
        use crate::engine::document::DocMeta;
        use chrono::NaiveDate;

        let mut app = make_test_app(1);
        let path = PathBuf::from("docs/rfcs/RFC-001.md");
        let config = Config::default();

        app.store.docs.insert(
            path.clone(),
            DocMeta {
                path: path.clone(),
                title: "Test".to_string(),
                doc_type: DocType::new("rfc"),
                status: Status::new(""),
                id: "RFC-001".to_string(),
                tags: Vec::new(),
                provenance: Vec::new(),
                author: String::new(),
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                related: Vec::new(),
                validate_ignore: false,
                virtual_doc: false,
                assignee: None,
                attributes: Default::default(),
            },
        );
        app.doc_tree[0].path = path.clone();
        app.selected_doc = 0;

        app.open_status_picker(&config);

        assert_eq!(
            app.status_picker.states,
            config.type_by_name("rfc").unwrap().lifecycle.states
        );
        assert!(
            !app.status_picker.states.iter().any(|s| s.is_empty()),
            "no blank row, got: {:?}",
            app.status_picker.states
        );
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
        app_with_store_config(files, &Config::default())
    }

    fn app_with_store_config(files: &[(&str, &str)], config: &Config) -> (tempfile::TempDir, App) {
        let tmp = tempfile::TempDir::new().unwrap();
        for (rel_path, contents) in files {
            let full = tmp.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        let store = Store::load(tmp.path(), config).unwrap();
        let mut app = make_test_app(0);
        app.store = store;
        (tmp, app)
    }

    /// An rfc doc with an explicit `title` and `body` and no tags, so a search
    /// test controls exactly which surface (title vs body) a query can match.
    fn titled_doc(title: &str, body: &str) -> String {
        format!(
            "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\n---\n\n{body}\n"
        )
    }

    /// Titles of the current `search_results`, in result order.
    fn result_titles(app: &App) -> Vec<String> {
        app.search_results
            .iter()
            .map(|p| app.store.get(p).unwrap().title.clone())
            .collect()
    }

    // AC: a query whose chars appear in a title in order but not contiguously
    // ("tff" in "tui fuzzy filter") is kept -- the old `.contains()` filter would
    // have dropped it.
    #[test]
    fn update_search_keeps_non_contiguous_title_subsequence() {
        let (_tmp, mut app) = app_with_store(&[(
            "docs/rfcs/RFC-001-a.md",
            &titled_doc("tui fuzzy filter", "unrelated body"),
        )]);

        app.search_query = "tff".to_string();
        app.run_search_now();

        assert_eq!(result_titles(&app), vec!["tui fuzzy filter".to_string()]);
    }

    // AC: rows are ordered by relevance score descending. The strong (exact)
    // match sits at the LATER-sorting path, so leading it proves the order comes
    // from score, not the path tie-break.
    #[test]
    fn update_search_orders_by_relevance_score_descending() {
        let (_tmp, mut app) = app_with_store(&[
            ("docs/rfcs/RFC-001-a.md", &titled_doc("xfxuxzxzxyx", "b1")),
            ("docs/rfcs/RFC-002-b.md", &titled_doc("fuzzy", "b2")),
        ]);

        app.search_query = "fuzzy".to_string();
        app.run_search_now();

        assert_eq!(
            result_titles(&app),
            vec!["fuzzy".to_string(), "xfxuxzxzxyx".to_string()]
        );
    }

    // AC: a query that fuzzy-matches only within a body (not title/tags/path)
    // still surfaces the document.
    #[test]
    fn update_search_surfaces_body_only_match() {
        let (_tmp, mut app) = app_with_store(&[(
            "docs/rfcs/RFC-001-a.md",
            &titled_doc("hello", "the quadratic solver lives here"),
        )]);

        app.search_query = "quadratic".to_string();
        app.run_search_now();

        assert_eq!(result_titles(&app), vec!["hello".to_string()]);
    }

    // AC: a query nothing fuzzy-matches yields an empty list -- non-matches are
    // dropped by the engine's score floor, not shown unhighlighted.
    #[test]
    fn update_search_empty_when_nothing_matches() {
        let (_tmp, mut app) =
            app_with_store(&[("docs/rfcs/RFC-001-a.md", &titled_doc("hello", "world body"))]);

        app.search_query = "zzqqww".to_string();
        app.run_search_now();

        assert!(app.search_results.is_empty());
    }

    // AC (BUG-011 a): each dispatch marks the search pending and bumps the
    // generation, and the request lands on the worker channel with that
    // generation and the current query.
    #[test]
    fn update_search_marks_pending_and_bumps_generation() {
        let (_tmp, mut app) =
            app_with_store(&[("docs/rfcs/RFC-001-a.md", &titled_doc("hello", "body"))]);
        let (tx, rx) = crossbeam_channel::unbounded();
        app.search_tx = tx;

        let before = app.search_generation;
        app.search_query = "hel".to_string();
        app.update_search();

        assert!(app.search_pending);
        assert_eq!(app.search_generation, before + 1);
        let req = rx.try_recv().expect("a request is dispatched");
        assert_eq!(req.query, "hel");
        assert_eq!(req.generation, app.search_generation);
    }

    // AC (BUG-011 b): results stamped with a stale generation are dropped
    // silently -- current results and the pending flag stay untouched.
    #[test]
    fn stale_generation_results_are_dropped() {
        let (_tmp, mut app) =
            app_with_store(&[("docs/rfcs/RFC-001-a.md", &titled_doc("hello", "body"))]);

        app.search_query = "hel".to_string();
        app.update_search();
        app.search_query = "hell".to_string();
        app.update_search();

        let stale = app.search_generation - 1;
        app.apply_search_results(stale, vec![PathBuf::from("docs/rfcs/RFC-001-a.md")]);

        assert!(app.search_results.is_empty(), "stale results not applied");
        assert!(app.search_pending, "still waiting on the live generation");
    }

    // AC (BUG-011 c): matching-generation results are applied and clear the
    // pending flag (which also clears the overlay spinner).
    #[test]
    fn matching_generation_results_apply_and_clear_pending() {
        let (_tmp, mut app) =
            app_with_store(&[("docs/rfcs/RFC-001-a.md", &titled_doc("hello", "body"))]);

        app.search_query = "hel".to_string();
        app.update_search();
        app.search_selected = 3;

        let path = PathBuf::from("docs/rfcs/RFC-001-a.md");
        app.apply_search_results(app.search_generation, vec![path.clone()]);

        assert_eq!(app.search_results, vec![path]);
        assert!(!app.search_pending);
        assert_eq!(app.search_selected, 0);
    }

    // AC (BUG-011 d): an empty query clears results immediately without going
    // pending or dispatching to the worker; the generation bump invalidates any
    // in-flight search so its late results cannot repopulate the cleared list.
    #[test]
    fn empty_query_clears_results_without_pending() {
        let (_tmp, mut app) =
            app_with_store(&[("docs/rfcs/RFC-001-a.md", &titled_doc("hello", "body"))]);
        let (tx, rx) = crossbeam_channel::unbounded();
        app.search_tx = tx;

        app.search_query = "hel".to_string();
        app.run_search_now();
        assert!(!app.search_results.is_empty());
        let in_flight = app.search_generation;
        let _ = rx.try_recv();

        app.search_query.clear();
        app.update_search();

        assert!(app.search_results.is_empty());
        assert!(!app.search_pending);
        assert!(rx.try_recv().is_err(), "empty query dispatches nothing");

        app.apply_search_results(in_flight, vec![PathBuf::from("docs/rfcs/RFC-001-a.md")]);
        assert!(
            app.search_results.is_empty(),
            "late results for the pre-clear query stay dropped"
        );
    }

    // AC: the characters that matched are visually highlighted in the rendered
    // row. The matched title chars render bold + yellow; an unmatched title char
    // does not.
    #[test]
    fn search_overlay_highlights_matched_title_chars() {
        use crate::tui::views::StatusPalette;
        use ratatui::{backend::TestBackend, Terminal};

        let (_tmp, mut app) =
            app_with_store(&[("docs/rfcs/RFC-001-a.md", &titled_doc("fuzzy", "body"))]);
        app.search_mode = true;
        app.search_query = "fzy".to_string(); // matches f(0) z(2) y(4) of "fuzzy"
        app.run_search_now();
        assert_eq!(app.search_results.len(), 1);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let palette = StatusPalette::default();
        terminal
            .draw(|f| crate::tui::views::overlays::draw_search_overlay(f, &app, &palette))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        let mut matched_cells = 0usize;
        let mut plain_u = false;
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
                let sym = cell.symbol();
                let highlighted = cell.fg == ratatui::style::Color::Yellow
                    && cell.modifier.contains(ratatui::style::Modifier::BOLD);
                if (sym == "f" || sym == "z" || sym == "y") && highlighted {
                    matched_cells += 1;
                }
                // The 'u' at index 1 of "fuzzy" is NOT part of the match, so it
                // must render without the highlight style.
                if sym == "u" && !highlighted {
                    plain_u = true;
                }
            }
        }

        assert!(
            matched_cells >= 3,
            "matched title chars f/z/y should render highlighted (yellow+bold)"
        );
        assert!(
            plain_u,
            "the unmatched 'u' should render without the highlight style"
        );
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

    // AC (BUG-013): a related-to relation declared in frontmatter surfaces in
    // the Relations tab even when the config's `related-to` entry carries no
    // `traversal = "related"` marker.
    #[test]
    fn relation_sections_includes_related_without_traversal_marker() {
        let mut config = Config::default();
        config
            .relationships
            .iter_mut()
            .find(|r| r.name == "related-to")
            .unwrap()
            .traversal = None;

        let (_tmp, app) = app_with_store_config(
            &[
                (
                    "docs/rfcs/RFC-001-anchor.md",
                    &relations_doc_md("Anchor", "rfc", "- related-to: RFC-002"),
                ),
                (
                    "docs/rfcs/RFC-002-near.md",
                    &relations_doc_md("Near", "rfc", "[]"),
                ),
            ],
            &config,
        );

        let doc = doc_by_id(&app, "RFC-001");
        let sections = app.relation_sections(&doc);
        let ids: std::collections::BTreeSet<String> =
            ids_for_paths(&app, &sections.related).into_iter().collect();

        assert!(
            ids.contains("RFC-002"),
            "declared related-to must appear without the traversal marker, got {ids:?}"
        );
    }

    // AC (BUG-013 guard): with the default config's `traversal = "related"`
    // marker in place, a declared related-to relation must appear exactly once
    // in the related section -- merging the declared list must not duplicate
    // what the related BFS already found.
    #[test]
    fn relation_sections_related_with_traversal_marker_appears_once() {
        let (_tmp, app) = app_with_store(&[
            (
                "docs/rfcs/RFC-001-anchor.md",
                &relations_doc_md("Anchor", "rfc", "- related-to: RFC-002"),
            ),
            (
                "docs/rfcs/RFC-002-near.md",
                &relations_doc_md("Near", "rfc", "[]"),
            ),
        ]);

        let doc = doc_by_id(&app, "RFC-001");
        let sections = app.relation_sections(&doc);
        let ids = ids_for_paths(&app, &sections.related);

        let occurrences = ids.iter().filter(|id| *id == "RFC-002").count();
        assert_eq!(
            occurrences, 1,
            "related-to target must appear exactly once in related, got {ids:?}"
        );
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
    fn settings_buffer_initialised_clean_from_config() {
        let app = make_test_app(0);
        assert!(!app.settings_dirty);
        assert_eq!(app.settings_field, 0);
        let buffer_types: Vec<String> = app
            .settings_buffer
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let config_types: Vec<String> = Config::default()
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert_eq!(buffer_types, config_types);
    }

    #[test]
    fn apply_config_reseeds_buffer_when_clean() {
        let mut app = make_test_app(0);
        assert!(!app.settings_dirty);
        app.apply_config(&config_with_types(&["rfc", "story", "adr"]));
        let buffer_types: Vec<&str> = app
            .settings_buffer
            .documents
            .types
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(buffer_types, vec!["rfc", "story", "adr"]);
    }

    #[test]
    fn apply_config_preserves_dirty_buffer() {
        let mut app = make_test_app(0);
        app.settings_dirty = true;
        let before: Vec<String> = app
            .settings_buffer
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect();
        app.apply_config(&config_with_types(&["rfc", "story", "adr"]));
        let after: Vec<String> = app
            .settings_buffer
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert_eq!(
            before, after,
            "dirty buffer must not be clobbered by reload"
        );
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
            github_native: None,
            traversal: None,
        }];

        app.apply_config(&config);

        assert_eq!(
            app.rel_types,
            vec!["derives-from".to_string(), "derived-by".to_string()],
            "rel_types must reflect the reloaded [[relationships]] (name then inverse)"
        );
    }

    #[test]
    fn link_editor_types_jk_into_query() {
        let mut app = make_test_app(0);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.link_editor.active = true;

        for c in ['j', 'a', 'c', 'k'] {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE, &root, &config);
        }

        assert_eq!(
            app.link_editor.query, "jack",
            "j/k must type into the search query, not navigate the result list"
        );
    }

    #[test]
    fn link_editor_left_right_cycle_rel_type() {
        let mut app = make_test_app(0);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.rel_types = vec![
            "implements".to_string(),
            "implemented-by".to_string(),
            "related-to".to_string(),
        ];
        app.link_editor.active = true;
        app.link_editor.rel_type_index = 0;

        app.handle_key(KeyCode::Right, KeyModifiers::NONE, &root, &config);
        assert_eq!(app.link_editor.rel_type_index, 1, "Right cycles forward");

        app.handle_key(KeyCode::Left, KeyModifiers::NONE, &root, &config);
        app.handle_key(KeyCode::Left, KeyModifiers::NONE, &root, &config);
        assert_eq!(
            app.link_editor.rel_type_index, 2,
            "Left wraps backward past index 0"
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

    // --- Settings field editors (ITERATION-188 Tasks 3/4/5) ---

    use crate::engine::config::{
        EdgeDef, GithubConfig, RelSelector, ReservedFormat, SqidsConfig, StoreBackend, TypeSelector,
    };

    /// Build a settings-edit-ready app: set the buffer to `config`, focus the
    /// named category and the field at `field`, leave it clean and not editing.
    fn settings_app(config: Config, category: &str, field: usize) -> App {
        let mut app = make_test_app(0);
        app.settings_buffer = config;
        app.settings_dirty = false;
        app.settings_category = App::settings_category_index(category);
        app.settings_entry = 0;
        app.settings_drill = None;
        app.settings_field = field;
        app
    }

    fn type_a() -> String {
        // A single editable type so drilled type-views are deterministic.
        "a".to_string()
    }

    fn config_one_type() -> Config {
        config_with_types(&[&type_a()])
    }

    fn type_chars(app: &mut App, s: &str) {
        for c in s.chars() {
            app.settings_edit_input.push(c);
        }
    }

    // --- AC3 / AC5 pure validators ---

    #[test]
    fn validate_bounded_rejects_out_of_range_and_nonnumeric() {
        assert!(validate_bounded("0", 1, 10).is_err());
        assert!(validate_bounded("11", 1, 10).is_err());
        assert!(validate_bounded("abc", 1, 10).is_err());
    }

    #[test]
    fn validate_bounded_accepts_in_range() {
        assert_eq!(validate_bounded("1", 1, 10).unwrap(), 1);
        assert_eq!(validate_bounded("10", 1, 10).unwrap(), 10);
        assert_eq!(validate_bounded("5", 1, 10).unwrap(), 5);
    }

    // --- AC1 Text ---

    #[test]
    fn ac1_text_edit_writes_to_buffer_and_dirties() {
        let mut app = settings_app(config_one_type(), "General", 0); // naming.pattern
        app.settings_start_edit();
        assert!(app.settings_editing);
        // Replace seeded input with a fresh value.
        app.settings_edit_input.clear();
        type_chars(&mut app, "{type}-{title}.md");
        app.settings_confirm_edit();

        assert_eq!(
            app.settings_buffer.documents.naming.pattern,
            "{type}-{title}.md"
        );
        assert!(app.settings_dirty);
        assert!(!app.settings_editing);
    }

    // --- AC2 Toggle ---

    #[test]
    fn ac2_toggle_statusbar_enabled_flips_and_dirties() {
        let mut config = config_one_type();
        config.ui.statusbar.enabled = true;
        let mut app = settings_app(config, "Interface", 1); // statusbar.enabled

        app.settings_space();
        assert!(!app.settings_buffer.ui.statusbar.enabled);
        assert!(app.settings_dirty);

        app.settings_space();
        assert!(
            app.settings_buffer.ui.statusbar.enabled,
            "second Space flips back"
        );
    }

    #[test]
    fn ac2_toggle_drilled_type_subdirectory_flips() {
        let config = config_one_type();
        let mut app = settings_app(config, "Document Types", 6); // subdirectory
        app.settings_drill = Some(0);

        let before = app.settings_buffer.documents.types[0].subdirectory;
        app.settings_space();
        assert_eq!(app.settings_buffer.documents.types[0].subdirectory, !before);
        assert!(app.settings_dirty);
    }

    // --- AC3 BoundedNum (app-state) ---

    #[test]
    fn ac3_bounded_num_rejects_keeps_buffer_then_accepts() {
        let mut config = config_one_type();
        config.documents.sqids = Some(SqidsConfig {
            salt: "seed".to_string(),
            min_length: 3,
        });
        let mut app = settings_app(config, "Numbering", 1); // sqids.min_length

        // Reject "0".
        app.settings_start_edit();
        app.settings_edit_input.clear();
        type_chars(&mut app, "0");
        app.settings_confirm_edit();
        assert_eq!(
            app.settings_buffer
                .documents
                .sqids
                .as_ref()
                .unwrap()
                .min_length,
            3
        );
        assert!(!app.settings_dirty);
        assert!(app.settings_edit_error.is_some());
        assert!(app.settings_editing, "rejected edit stays in edit mode");

        // Accept "7".
        app.settings_edit_input.clear();
        type_chars(&mut app, "7");
        app.settings_confirm_edit();
        assert_eq!(
            app.settings_buffer
                .documents
                .sqids
                .as_ref()
                .unwrap()
                .min_length,
            7
        );
        assert!(app.settings_dirty);
        assert!(!app.settings_editing);
    }

    // --- AC4 Nullable ---

    #[test]
    fn ac4_nullable_empty_is_none_value_is_some() {
        let mut config = config_one_type();
        config.documents.github = Some(GithubConfig {
            repo: Some("old/repo".to_string()),
            cache_ttl: 60,
        });
        let mut app = settings_app(config, "GitHub", 0); // repo

        // Empty => None (not Some("")).
        app.settings_start_edit();
        app.settings_edit_input.clear();
        app.settings_confirm_edit();
        assert_eq!(
            app.settings_buffer.documents.github.as_ref().unwrap().repo,
            None
        );
        assert!(app.settings_dirty);

        // Non-empty => Some.
        app.settings_start_edit();
        app.settings_edit_input.clear();
        type_chars(&mut app, "owner/repo");
        app.settings_confirm_edit();
        assert_eq!(
            app.settings_buffer
                .documents
                .github
                .as_ref()
                .unwrap()
                .repo
                .as_deref(),
            Some("owner/repo")
        );
    }

    // --- AC6 List ---

    #[test]
    fn ac6_list_splits_trims_drops_empties() {
        let config = config_one_type();
        let mut app = settings_app(config, "Document Types", 10); // agents
        app.settings_drill = Some(0);

        app.settings_start_edit();
        app.settings_edit_input.clear();
        type_chars(&mut app, "expand, create-children");
        app.settings_confirm_edit();
        assert_eq!(
            app.settings_buffer.documents.types[0].agents,
            vec!["expand".to_string(), "create-children".to_string()]
        );

        app.settings_start_edit();
        app.settings_edit_input.clear();
        app.settings_confirm_edit();
        assert!(app.settings_buffer.documents.types[0].agents.is_empty());

        app.settings_start_edit();
        app.settings_edit_input.clear();
        type_chars(&mut app, " a , b ,");
        app.settings_confirm_edit();
        assert_eq!(
            app.settings_buffer.documents.types[0].agents,
            vec!["a".to_string(), "b".to_string()]
        );
    }

    // --- AC7 EnumCycle ---

    #[test]
    fn ac7_numbering_cycles_and_wraps() {
        let config = config_one_type();
        let mut app = settings_app(config, "Document Types", 5); // numbering
        app.settings_drill = Some(0);
        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Incremental
        );

        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Sqids
        );
        assert!(app.settings_dirty);

        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Reserved
        );

        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Incremental,
            "wraps back to first after last"
        );
    }

    #[test]
    fn ac7_store_cycles_through_three_variants() {
        let config = config_one_type();
        let mut app = settings_app(config, "Document Types", 7); // store
        app.settings_drill = Some(0);
        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::Filesystem
        );

        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::GithubIssues
        );
        assert!(app.settings_dirty);
        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::GithubMilestones
        );
        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::GithubProjects
        );
        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::GitRef
        );
        app.settings_space();
        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::Filesystem
        );
    }

    #[test]
    fn ac7_reserved_format_cycles() {
        let mut config = config_one_type();
        config.documents.reserved = Some(crate::engine::config::ReservedConfig {
            remote: "origin".to_string(),
            format: ReservedFormat::Incremental,
            max_retries: 5,
        });
        // Numbering view: sqids absent => 2 ReadOnly fields, then reserved.remote(2),
        // reserved.format(3), reserved.max_retries(4).
        let mut app = settings_app(config, "Numbering", 3);

        app.settings_space();
        assert_eq!(
            app.settings_buffer
                .documents
                .reserved
                .as_ref()
                .unwrap()
                .format,
            ReservedFormat::Sqids
        );
        assert!(app.settings_dirty);
        app.settings_space();
        assert_eq!(
            app.settings_buffer
                .documents
                .reserved
                .as_ref()
                .unwrap()
                .format,
            ReservedFormat::Incremental,
            "wraps"
        );
    }

    // --- ITERATION-189 config dependency auto-scaffolding (Tasks 2/3) ---

    /// A drilled-type app focused on the type's `numbering` EnumCycle, clean and
    /// not editing. cat 1 (Document Types), drilled into type 0, field 5.
    fn numbering_app(config: Config) -> App {
        let mut app = settings_app(config, "Document Types", 5);
        app.settings_drill = Some(0);
        app
    }

    // AC1: cycling numbering to `sqids` (section absent) auto-inserts the
    // [numbering.sqids] section with an empty salt + min_length 3, dirties the
    // buffer, and raises a scaffold offer pointing at the salt.
    #[test]
    fn ac1_numbering_to_sqids_scaffolds_section_and_offers_salt() {
        let mut app = numbering_app(config_one_type());
        assert!(app.settings_buffer.documents.sqids.is_none());

        app.settings_space();

        let sqids = app
            .settings_buffer
            .documents
            .sqids
            .as_ref()
            .expect("sqids section scaffolded");
        assert_eq!(sqids.salt, "", "salt scaffolded empty");
        assert_eq!(
            sqids.min_length, 3,
            "min_length scaffolded to parser default"
        );
        assert!(app.settings_dirty);
        assert_eq!(
            app.settings_scaffold_offer,
            Some(ScaffoldResult {
                inserted: ConfigDep::NumberingSqids,
                required_empty_field: Some(FieldPath::SqidsSalt),
            })
        );
    }

    // AC4: cycling numbering to `reserved` (section absent) scaffolds the
    // [numbering.reserved] section with defaults, dirties the buffer, and sets NO
    // required-empty offer (the section scaffolds complete).
    #[test]
    fn ac4_numbering_to_reserved_scaffolds_complete_section_no_offer() {
        // sqids is the first numbering variant after incremental, so cycle twice to
        // reach reserved; the first hop scaffolds sqids, so clear the offer between.
        let mut config = config_one_type();
        config.documents.sqids = Some(SqidsConfig {
            salt: "seed".to_string(),
            min_length: 3,
        });
        let mut app = numbering_app(config);

        app.settings_space(); // -> sqids (section present, no scaffold)
        assert!(app.settings_scaffold_offer.is_none());
        app.settings_space(); // -> reserved

        let reserved = app
            .settings_buffer
            .documents
            .reserved
            .as_ref()
            .expect("reserved section scaffolded");
        assert_eq!(reserved.remote, "origin");
        assert_eq!(reserved.format, ReservedFormat::Incremental);
        assert_eq!(reserved.max_retries, 5);
        assert!(app.settings_dirty);
        assert!(
            app.settings_scaffold_offer.is_none(),
            "reserved has no required-empty field, so no offer is raised"
        );
    }

    // AC5: cycling store to `github-issues` (section absent) scaffolds the [github]
    // section with repo None / cache_ttl 60, dirties the buffer, and sets no
    // required-empty offer.
    #[test]
    fn ac5_store_to_github_issues_scaffolds_section_no_offer() {
        let mut app = settings_app(config_one_type(), "Document Types", 7); // store field
        app.settings_drill = Some(0);
        assert!(app.settings_buffer.documents.github.is_none());

        app.settings_space(); // filesystem -> github-issues

        let github = app
            .settings_buffer
            .documents
            .github
            .as_ref()
            .expect("github section scaffolded");
        assert_eq!(github.repo, None);
        assert_eq!(github.cache_ttl, 60);
        assert!(app.settings_dirty);
        assert!(
            app.settings_scaffold_offer.is_none(),
            "github has no required-empty field, so no offer is raised"
        );
    }

    // AC6: cycling numbering to `sqids` when the section already exists leaves it
    // untouched and raises no offer.
    #[test]
    fn ac6_numbering_to_sqids_with_existing_section_is_skip() {
        let mut config = config_one_type();
        config.documents.sqids = Some(SqidsConfig {
            salt: "x".to_string(),
            min_length: 7,
        });
        let mut app = numbering_app(config);

        app.settings_space(); // incremental -> sqids

        let sqids = app.settings_buffer.documents.sqids.as_ref().unwrap();
        assert_eq!(sqids.salt, "x", "existing salt untouched");
        assert_eq!(sqids.min_length, 7, "existing min_length untouched");
        assert!(
            app.settings_scaffold_offer.is_none(),
            "an already-present section raises no offer (AC6)"
        );
    }

    // AC3 (accept): with a pending sqids offer, `g` jumps focus to the sqids salt
    // field and clears the offer.
    #[test]
    fn ac3_accept_key_jumps_to_salt_and_clears_offer() {
        let mut app = numbering_app(config_one_type());
        app.settings_space(); // scaffold sqids, raise offer
        assert!(app.settings_scaffold_offer.is_some());
        let config = Config::default();

        app.handle_settings_key(
            KeyCode::Char('g'),
            KeyModifiers::NONE,
            Path::new("."),
            &config,
        );

        assert_eq!(
            App::settings_categories()[app.settings_category],
            "Numbering",
            "landed on the Numbering category"
        );
        assert_eq!(app.settings_drill, None);
        let fields = crate::tui::views::panels::settings_fields(
            app.settings_category,
            app.settings_entry,
            app.settings_drill,
            &app.settings_buffer,
        );
        assert_eq!(
            fields[app.settings_field].path,
            FieldPath::SqidsSalt,
            "cursor resolves to the sqids salt field"
        );
        assert!(
            app.settings_scaffold_offer.is_none(),
            "offer cleared on accept"
        );
    }

    // AC3 (decline): with a pending sqids offer, a non-accept key (`j`) clears the
    // offer without jumping to the salt.
    #[test]
    fn ac3_non_accept_key_declines_offer_without_jumping() {
        let mut app = numbering_app(config_one_type());
        app.settings_space(); // scaffold sqids, raise offer
        assert!(app.settings_scaffold_offer.is_some());
        let config = Config::default();
        let before_category = app.settings_category;

        app.handle_settings_key(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            Path::new("."),
            &config,
        );

        assert!(
            app.settings_scaffold_offer.is_none(),
            "any non-accept key dismisses the offer"
        );
        assert_eq!(
            app.settings_category, before_category,
            "decline does not jump to the Numbering category"
        );
    }

    // --- RFC-023 STORY-144: Enter dispatch + variant picker (Task 5) ---

    // AC3: Enter on a bool field flips the buffer value and marks it dirty.
    #[test]
    fn ac3_enter_on_bool_flips_buffer_and_dirties() {
        let mut config = config_one_type();
        config.ui.statusbar.enabled = true;
        let mut app = settings_app(config, "Interface", 1); // statusbar.enabled
        let config = Config::default();

        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        assert!(
            !app.settings_buffer.ui.statusbar.enabled,
            "Enter flips the bool"
        );
        assert!(app.settings_dirty, "the flip marks the buffer dirty");
    }

    // AC4: Enter on an enum field opens the variant picker, carrying the numbering
    // variant set and pre-selecting the current value's index.
    #[test]
    fn ac4_enter_on_enum_opens_picker_with_current_selected() {
        let mut app = numbering_app(config_one_type()); // type numbering, value incremental
        let config = Config::default();

        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        let picker = app
            .settings_variant_picker
            .as_ref()
            .expect("Enter on an enum opens the picker");
        assert_eq!(
            picker.variants,
            &["incremental", "sqids", "reserved"],
            "picker carries the numbering variant set"
        );
        assert_eq!(
            picker.selected, 0,
            "picker pre-selects the current value (incremental)"
        );
    }

    // AC2: Enter on a text field begins inline editing.
    #[test]
    fn ac2_enter_on_text_starts_editing() {
        let mut app = settings_app(config_one_type(), "General", 0); // naming.pattern (Text)
        let config = Config::default();

        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        assert!(app.settings_editing, "Enter on a text field starts editing");
    }

    // AC7: Enter on a ReadOnly field changes nothing -- no edit, no picker, clean.
    #[test]
    fn ac7_enter_on_readonly_is_noop() {
        // Numbering view with no sqids section: field 0 is ReadOnly.
        let mut app = settings_app(config_one_type(), "Numbering", 0);
        let config = Config::default();

        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        assert!(!app.settings_editing, "ReadOnly does not start editing");
        assert!(
            app.settings_variant_picker.is_none(),
            "ReadOnly does not open a picker"
        );
        assert!(!app.settings_dirty, "ReadOnly leaves the buffer clean");
    }

    // AC8: Space is dropped -- it makes no state change on any field.
    #[test]
    fn ac8_space_is_inert_on_enum_field() {
        let mut app = numbering_app(config_one_type());
        let before = app.settings_buffer.documents.types[0].numbering.clone();
        let config = Config::default();

        app.handle_settings_key(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            Path::new("."),
            &config,
        );

        assert_eq!(
            app.settings_buffer.documents.types[0].numbering, before,
            "Space does not cycle the enum"
        );
        assert!(
            app.settings_variant_picker.is_none(),
            "Space does not open a picker"
        );
        assert!(!app.settings_editing, "Space does not start editing");
        assert!(!app.settings_dirty, "Space leaves the buffer clean");
    }

    // AC9: Enter on an entry in a collection list drills into that entry.
    #[test]
    fn ac9_enter_on_entry_list_drills_into_entry() {
        let config = config_with_types(&["rfc", "story"]);
        let mut app = settings_app(config, "Document Types", 0); // the collection, not drilled
        app.settings_entry = 1;
        let config = Config::default();

        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        assert_eq!(
            app.settings_drill,
            Some(1),
            "Enter drills into the selected entry"
        );
    }

    // AC5: committing `sqids` through the picker path produces the same scaffold
    // post-conditions as the cycle path (mirrors
    // `ac1_numbering_to_sqids_scaffolds_section_and_offers_salt`).
    #[test]
    fn ac5_picker_commit_sqids_scaffolds_section_like_cycle() {
        let mut app = numbering_app(config_one_type());
        assert!(app.settings_buffer.documents.sqids.is_none());
        let config = Config::default();

        // Open the picker, move incremental -> sqids, commit.
        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);
        app.handle_settings_key(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            Path::new("."),
            &config,
        );
        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Sqids,
            "the chosen variant is written"
        );
        let sqids = app
            .settings_buffer
            .documents
            .sqids
            .as_ref()
            .expect("sqids section scaffolded via the picker path");
        assert_eq!(sqids.salt, "", "salt scaffolded empty (cycle parity)");
        assert_eq!(
            sqids.min_length, 3,
            "min_length scaffolded to parser default (cycle parity)"
        );
        assert!(app.settings_dirty);
        assert_eq!(
            app.settings_scaffold_offer,
            Some(ScaffoldResult {
                inserted: ConfigDep::NumberingSqids,
                required_empty_field: Some(FieldPath::SqidsSalt),
            }),
            "the same scaffold offer the cycle path raises"
        );
        assert!(
            app.settings_variant_picker.is_none(),
            "the picker closes on commit"
        );
    }

    // AC4: Esc closes the picker and leaves the buffer untouched.
    #[test]
    fn ac4_picker_esc_closes_without_writing() {
        let mut app = numbering_app(config_one_type());
        let before = app.settings_buffer.documents.types[0].numbering.clone();
        let config = Config::default();

        // Open the picker, move selection, then cancel.
        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);
        app.handle_settings_key(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            Path::new("."),
            &config,
        );
        app.handle_settings_key(KeyCode::Esc, KeyModifiers::NONE, Path::new("."), &config);

        assert!(
            app.settings_variant_picker.is_none(),
            "Esc closes the picker"
        );
        assert_eq!(
            app.settings_buffer.documents.types[0].numbering, before,
            "Esc leaves the numbering value unchanged"
        );
    }

    // Re-picking the variant already in force is a no-op: the picker commit must
    // not dirty the buffer, so quitting afterward raises no save prompt.
    #[test]
    fn repicking_current_variant_does_not_dirty() {
        let mut app = numbering_app(config_one_type()); // numbering = incremental
        let config = Config::default();

        // Open the picker (pre-selects the current value) and commit without moving.
        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);
        app.handle_settings_key(KeyCode::Enter, KeyModifiers::NONE, Path::new("."), &config);

        assert!(
            !app.settings_dirty,
            "re-picking the current variant leaves the buffer clean"
        );
        assert!(
            app.settings_variant_picker.is_none(),
            "the picker closes on commit"
        );
    }

    // Re-selecting the current variant directly is a no-op on the buffer, while a
    // genuinely different variant still writes + dirties.
    #[test]
    fn set_enum_variant_noop_on_current_writes_on_change() {
        let mut app = numbering_app(config_one_type()); // numbering = incremental
        let path = FieldPath::Type {
            index: 0,
            key: TypeKey::Numbering,
        };

        app.settings_set_enum_variant(&path, "incremental");
        assert!(!app.settings_dirty, "same variant does not dirty");

        app.settings_set_enum_variant(&path, "sqids");
        assert!(app.settings_dirty, "a different variant dirties");
        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Sqids
        );
    }

    // --- start_edit / space no-op on ReadOnly ---

    #[test]
    fn readonly_field_start_edit_and_space_are_noops() {
        // Numbering view with no sqids section: sqids.salt is ReadOnly/Unset.
        let mut app = settings_app(config_one_type(), "Numbering", 0);
        app.settings_start_edit();
        assert!(!app.settings_editing, "start_edit no-op on ReadOnly");
        app.settings_space();
        assert!(!app.settings_dirty, "space no-op on ReadOnly");
    }

    /// A config whose DAG is one fully-stated edge row, for the Edges category.
    fn config_one_edge() -> Config {
        Config {
            edges: vec![EdgeDef {
                name: "a-implements-a".to_string(),
                from: TypeSelector::Types(vec![type_a()]),
                to: TypeSelector::Any,
                via: RelSelector::Named(vec!["implements".to_string()]),
                required: Some(Severity::Error),
                traversal: Some(Traversal::Chain),
            }],
            ..config_one_type()
        }
    }

    /// A settings app focused on the labelled field of drilled edge row 0.
    /// Tests address the field by label because an index does not fail when
    /// `EdgeDef` gains a key -- it silently addresses the neighbour.
    fn edge_field_app(config: Config, label: &str) -> App {
        let mut app = settings_app(config, "Edges", 0);
        focus_edge_field(&mut app, label);
        app
    }

    /// Drill into edge row 0 and focus its labelled field, on an app built any
    /// way (`settings_app` for buffer-only tests, `save_app` when the save has
    /// to reach a real file).
    fn focus_edge_field(app: &mut App, label: &str) {
        focus_edge_row_field(app, 0, label);
    }

    /// [`focus_edge_field`] for a chosen row, so a pairwise refusal can be
    /// provoked from the row that is not the one the cursor is expected to land
    /// on.
    fn focus_edge_row_field(app: &mut App, row: usize, label: &str) {
        app.settings_category = App::settings_category_index("Edges");
        app.settings_entry = row;
        app.settings_drill = Some(row);
        let fields = crate::tui::views::panels::settings_fields(
            app.settings_category,
            app.settings_entry,
            app.settings_drill,
            &app.settings_buffer,
        );
        app.settings_field = fields
            .iter()
            .position(|f| f.label == label)
            .unwrap_or_else(|| panic!("no edge field labelled {label}: {fields:?}"));
    }

    fn edge_path(key: EdgeKey) -> FieldPath {
        FieldPath::Edge { index: 0, key }
    }

    // STORY-260 AC2: `name` is a plain string on the row, so it takes the text
    // write and reads straight back.
    #[test]
    fn edge_name_write_lands_in_the_buffer() {
        let mut app = edge_field_app(config_one_edge(), "name");

        app.settings_write(
            &edge_path(EdgeKey::Name),
            SettingsValue::Text("a-relates-a".to_string()),
        );

        assert_eq!(app.settings_buffer.edges[0].name, "a-relates-a");
        assert_eq!(app.settings_focused_raw(), "a-relates-a");
    }

    // A type position is a set or the wildcard, and the comma editor's seed has
    // to round-trip both -- `*` back to the wildcard, names back to the set.
    #[test]
    fn edge_type_position_write_round_trips_set_and_wildcard() {
        let mut app = edge_field_app(config_one_edge(), "to");
        let to = edge_path(EdgeKey::To);

        app.settings_write(
            &to,
            SettingsValue::List(vec!["story".to_string(), "bug".to_string()]),
        );
        assert_eq!(
            app.settings_buffer.edges[0].to,
            TypeSelector::Types(vec!["story".to_string(), "bug".to_string()])
        );
        assert_eq!(
            app.settings_focused_raw(),
            "story, bug",
            "the raw seed is what the comma editor splits, so it carries no list brackets"
        );

        app.settings_write(&to, SettingsValue::List(vec!["*".to_string()]));
        assert_eq!(
            app.settings_buffer.edges[0].to,
            TypeSelector::Any,
            "`*` is the wildcard, not a type named `*`"
        );
        assert_eq!(app.settings_focused_raw(), "*");
    }

    // `via` is a relationship set on the same terms (ADR-032), so it takes the
    // same editor and the same wildcard round-trip.
    #[test]
    fn edge_via_write_round_trips_set_and_wildcard() {
        let mut app = edge_field_app(config_one_edge(), "via");
        let via = edge_path(EdgeKey::Via);

        app.settings_write(
            &via,
            SettingsValue::List(vec!["blocks".to_string(), "implements".to_string()]),
        );
        assert_eq!(
            app.settings_buffer.edges[0].via,
            RelSelector::Named(vec!["blocks".to_string(), "implements".to_string()])
        );
        assert_eq!(app.settings_focused_raw(), "blocks, implements");

        app.settings_write(&via, SettingsValue::List(vec!["*".to_string()]));
        assert_eq!(app.settings_buffer.edges[0].via, RelSelector::Any);
    }

    // An absent `required` states no requiredness at all (RFC-067), so the
    // write has to be able to clear the key and read the cleared state back as
    // the cycler's unset position.
    #[test]
    fn edge_required_write_lands_and_reads_back_unset() {
        let mut app = edge_field_app(config_one_edge(), "required");
        let required = edge_path(EdgeKey::Required);

        app.settings_write(
            &required,
            SettingsValue::OptSeverity(Some(Severity::Warning)),
        );
        assert_eq!(
            app.settings_buffer.edges[0].required,
            Some(Severity::Warning)
        );
        assert_eq!(app.settings_focused_raw(), "warning");

        app.settings_write(&required, SettingsValue::OptSeverity(None));
        assert_eq!(app.settings_buffer.edges[0].required, None);
        assert_eq!(app.settings_focused_raw(), UNSET_VARIANT);
    }

    // An absent `traversal` names no role, leaving the triple to any other
    // matching row (ADR-030) -- again a claim, not a default.
    #[test]
    fn edge_traversal_write_lands_and_reads_back_unset() {
        let mut app = edge_field_app(config_one_edge(), "traversal");
        let traversal = edge_path(EdgeKey::Traversal);

        app.settings_write(
            &traversal,
            SettingsValue::OptTraversal(Some(Traversal::Related)),
        );
        assert_eq!(
            app.settings_buffer.edges[0].traversal,
            Some(Traversal::Related)
        );
        assert_eq!(app.settings_focused_raw(), "related");

        app.settings_write(&traversal, SettingsValue::OptTraversal(None));
        assert_eq!(app.settings_buffer.edges[0].traversal, None);
        assert_eq!(app.settings_focused_raw(), UNSET_VARIANT);
    }

    // The cycler has to reach unset, because absence is reachable in the file
    // and means something the panel would otherwise be unable to say.
    #[test]
    fn cycling_required_passes_through_unset_and_wraps_to_error() {
        let mut app = edge_field_app(config_one_edge(), "required");
        assert_eq!(app.settings_buffer.edges[0].required, Some(Severity::Error));

        app.settings_space();
        assert_eq!(
            app.settings_buffer.edges[0].required,
            Some(Severity::Warning)
        );

        app.settings_space();
        assert_eq!(
            app.settings_buffer.edges[0].required, None,
            "the unset position is reachable by cycling"
        );

        app.settings_space();
        assert_eq!(
            app.settings_buffer.edges[0].required,
            Some(Severity::Error),
            "cycling wraps past unset back to the first severity"
        );
    }

    // Cycling to unset must remove the key rather than write a default.
    // `EdgeDef.required` is `skip_serializing_if = "Option::is_none"`, so the
    // rendered TOML is where the difference is visible.
    #[test]
    fn cycling_required_to_unset_renders_no_required_key() {
        let mut app = edge_field_app(config_one_edge(), "required");
        app.settings_space();
        app.settings_space();

        let toml = app.settings_buffer.to_toml().expect("buffer renders");

        assert!(
            !toml.contains("required ="),
            "an unset qualifier writes no key: {toml}"
        );
    }

    // Clearing a type position yields the empty set, which matches nothing. The
    // loader does not catch it -- its declared-type check iterates `names()`,
    // and an empty list iterates nothing -- so the panel refuses it at commit
    // rather than reading it as `*`, a claim the user did not make.
    #[test]
    fn clearing_an_edge_target_set_is_refused_and_leaves_the_buffer_alone() {
        let mut app = edge_field_app(config_one_edge(), "to");
        let before = app.settings_buffer.edges[0].to.clone();

        app.settings_start_edit();
        app.settings_edit_input.clear();
        app.settings_confirm_edit();

        assert_eq!(app.settings_buffer.edges[0].to, before);
        assert!(!app.settings_dirty, "a refused edit writes nothing");
        assert!(app.settings_edit_error.is_some());
        assert!(app.settings_editing, "a refused edit stays in edit mode");
    }

    // The refusal is scoped to edge positions: `types[].agents` shares the
    // comma editor, and an empty agents list is how that key is unset.
    #[test]
    fn clearing_the_agents_list_is_still_accepted() {
        let mut config = config_one_type();
        config.documents.types[0].agents = vec!["claude".to_string()];
        let mut app = settings_app(config, "Document Types", 10); // agents
        app.settings_drill = Some(0);

        app.settings_start_edit();
        app.settings_edit_input.clear();
        app.settings_confirm_edit();

        assert!(app.settings_buffer.documents.types[0].agents.is_empty());
        assert!(app.settings_dirty);
        assert!(app.settings_edit_error.is_none());
    }

    // A field's editor picks the commit path, and `settings_write` no-ops on a
    // carrier mismatch by design -- so an editor paired with the wrong carrier
    // is a silently dropped edit. Drive every edge key through the editor its
    // own row declares and assert the buffer moved.
    #[test]
    fn every_edge_field_commits_through_the_editor_its_row_declares() {
        for label in ["name", "from", "to", "via", "required", "traversal"] {
            let mut app = edge_field_app(config_one_edge(), label);
            let before = app.settings_buffer.edges[0].clone();
            let editor = app
                .settings_focused_field()
                .expect("a drilled edge row has fields")
                .editor;

            match editor {
                FieldEditor::Text | FieldEditor::List => {
                    app.settings_start_edit();
                    assert!(app.settings_editing, "{label} opens the text editor");
                    app.settings_edit_input = "changed".to_string();
                    app.settings_confirm_edit();
                }
                FieldEditor::EnumCycle { .. } => app.settings_space(),
                other => panic!("{label} carries an editor with no edge commit path: {other:?}"),
            }

            assert_ne!(
                app.settings_buffer.edges[0], before,
                "the {label} edit was dropped"
            );
            assert!(
                app.settings_dirty,
                "the {label} edit did not dirty the buffer"
            );
        }
    }

    #[test]
    fn start_edit_noop_on_toggle_and_enumcycle() {
        let config = config_one_type();
        // Toggle field (Interface > ascii_diagrams at index 0).
        let mut app = settings_app(config.clone(), "Interface", 0);
        app.settings_start_edit();
        assert!(!app.settings_editing, "Toggle does not use edit mode");

        // EnumCycle field (drilled type numbering).
        let mut app = settings_app(config, "Document Types", 5);
        app.settings_drill = Some(0);
        app.settings_start_edit();
        assert!(!app.settings_editing, "EnumCycle does not use edit mode");
    }

    // --- Settings save (ITERATION-188 Tasks 6/8) ---

    /// A valid `.lazyspec.toml` carrying a standalone comment, an inline trailing
    /// comment, `[[types]]`, and `[[relationships]]` -- the comment-survival and
    /// single-edit assertions key off these distinctive lines.
    const SAVE_SRC: &str = r#"# lazyspec save fixture
[naming]
pattern = "{type}-{n:03}-{title}.md"  # filename template

[templates]
dir = ".lazyspec/templates"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;

    /// Write `SAVE_SRC` (or `src`) into a fresh TempDir's `.lazyspec.toml`, build
    /// an app whose buffer is the parsed config, and return both. The TempDir is
    /// returned so it outlives the app/file.
    fn save_app(src: &str) -> (tempfile::TempDir, App) {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".lazyspec.toml"), src).unwrap();
        let mut app = make_test_app(0);
        app.settings_buffer = Config::parse(src).unwrap();
        app.settings_dirty = false;
        (tmp, app)
    }

    fn read_config_file(tmp: &tempfile::TempDir) -> String {
        std::fs::read_to_string(tmp.path().join(".lazyspec.toml")).unwrap()
    }

    // AC8: a valid save writes once atomically, preserves comments, changes only
    // the edited value, clears dirty + footer, and raises the reload flag.
    #[test]
    fn ac8_save_writes_atomically_preserves_comments_and_triggers_reload() {
        let (tmp, mut app) = save_app(SAVE_SRC);
        app.settings_buffer.documents.naming.pattern = "{type}-{title}.md".to_string();
        app.settings_dirty = true;

        app.settings_save(tmp.path(), &Config::default());

        let out = read_config_file(&tmp);
        // Re-parses OK -- the file on disk is a valid config.
        Config::parse(&out).unwrap();
        // The standalone and inline trailing comments survive.
        assert!(out.contains("# lazyspec save fixture"));
        assert!(out.contains("# filename template"));
        // Only the edited value changed: the new pattern is present, the old gone,
        // and every other distinctive line is byte-identical to the source.
        assert!(out.contains("{type}-{title}.md"));
        assert!(!out.contains("{type}-{n:03}-{title}.md"));
        for line in [
            r#"dir = ".lazyspec/templates""#,
            r#"name = "rfc""#,
            r#"prefix = "RFC""#,
            r#"name = "implements""#,
            r#"inverse = "implemented-by""#,
        ] {
            assert!(out.contains(line), "unchanged line vanished: {line}");
        }
        assert_eq!(
            out.lines().count(),
            SAVE_SRC.lines().count(),
            "an in-place scalar edit must not change the line count"
        );

        assert!(!app.settings_dirty, "dirty clears on success");
        assert_eq!(app.settings_footer_error, None, "footer clears on success");
        assert!(app.config_reload_request, "reload is triggered on success");
    }

    /// A save fixture whose DAG is declared: a comment above the block, an
    /// inline comment on a key inside it, and a section after it.
    const EDGES_SAVE_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[relationships]]
name = "implements"
inverse = "implemented-by"

# every story hangs off an RFC
[[edges]]
name = "stories-need-rfcs"
from = "story"
to = "rfc"
via = "implements"  # the only relationship that realizes it
required = "warning"

[github]
repo = "owner/repo"
"#;

    // STORY-260 AC5 through the save protocol: an edge edit reaches disk on the
    // same terms as any other field, and a footer error left by an earlier
    // failed save clears with it.
    #[test]
    fn saving_a_drilled_edge_edit_writes_it_and_asks_for_a_reload() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        focus_edge_field(&mut app, "to");
        app.settings_start_edit();
        app.settings_edit_input = "rfc, story".to_string();
        app.settings_confirm_edit();
        assert!(app.settings_dirty, "the edit dirtied the buffer");
        app.settings_footer_error = Some("an earlier save failed".to_string());

        app.settings_save(tmp.path(), &Config::parse(EDGES_SAVE_SRC).unwrap());

        let out = read_config_file(&tmp);
        assert_eq!(
            Config::parse(&out).expect("the saved file loads").edges[0].to,
            TypeSelector::Types(vec!["rfc".to_string(), "story".to_string()])
        );
        assert_eq!(
            changed_config_lines(EDGES_SAVE_SRC, &out),
            vec![(r#"to = "rfc""#, r#"to = ["rfc", "story"]"#)],
            "got: {out}"
        );
        assert!(!app.settings_dirty, "dirty clears on success");
        assert_eq!(app.settings_footer_error, None, "footer clears on success");
        assert!(app.config_reload_request, "reload is triggered on success");
    }

    // Edges joining the writer's set is the moment a table that used to be left
    // alone starts being rewritten on every save, so a save that carries no
    // edit has to leave the file exactly as it found it -- a reformat of an
    // untouched block would otherwise go unnoticed.
    #[test]
    fn saving_a_clean_buffer_leaves_a_declared_dag_byte_identical() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);

        app.settings_save(tmp.path(), &Config::parse(EDGES_SAVE_SRC).unwrap());

        assert_eq!(app.settings_footer_error, None, "the save succeeded");
        assert_eq!(read_config_file(&tmp), EDGES_SAVE_SRC);
    }

    /// The `(before, after)` pairs of every line a save changed, positionally.
    /// Line-for-line, because a save that reflows an untouched block is exactly
    /// what these tests are looking for.
    fn changed_config_lines<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        assert_eq!(
            before.lines().count(),
            after.lines().count(),
            "line count moved:\n{after}"
        );
        before
            .lines()
            .zip(after.lines())
            .filter(|(b, a)| b != a)
            .collect()
    }

    // --- STORY-260 AC4: an edge edit the loader refuses (ITERATION-389) ---

    /// A save fixture declaring two rows that overlap and agree on both
    /// qualifiers, so it loads, and an edit to either qualifier produces a
    /// pairwise refusal -- a requiredness tie, a traversal disagreement -- and
    /// nothing else.
    const TWO_EDGES_SAVE_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[edges]]
name = "stories-need-rfcs"
from = "story"
to = "rfc"
via = "implements"
required = "warning"
traversal = "chain"

[[edges]]
name = "implementers-need-rfcs"
from = ["story", "rfc"]
to = "rfc"
via = "implements"
required = "warning"
traversal = "chain"
"#;

    /// The message `Config::parse` gives for the exact bytes `app`'s buffer
    /// would write over `src` -- the text the footer has to carry. Derived, not
    /// spelled: a literal expectation here would be the second spelling AC4
    /// forbids, written into the assertion meant to forbid it.
    fn loader_refusal(src: &str, app: &App) -> String {
        let destined =
            crate::engine::config_write::write_config_in_place(src, &app.settings_buffer)
                .expect("the writer renders the buffer");
        Config::parse(&destined)
            .expect_err("the bytes destined for disk do not load")
            .to_string()
    }

    /// Commit `input` into the labelled field of drilled edge row 0 the way the
    /// panel does: open the editor, type, confirm.
    fn commit_edge_edit(app: &mut App, label: &str, input: &str) {
        focus_edge_field(app, label);
        app.settings_start_edit();
        app.settings_edit_input = input.to_string();
        app.settings_confirm_edit();
        assert!(app.settings_dirty, "the edit dirtied the buffer");
    }

    /// The buffer path under the settings cursor, read through the same field
    /// list the render uses. `FieldPath::Edge` carries the drilled row, so the
    /// path alone pins category, drill and field cursor together.
    fn focused_path(app: &App) -> FieldPath {
        app.settings_focused_field()
            .expect("the cursor is on a field")
            .path
    }

    fn save_edges(app: &mut App, tmp: &tempfile::TempDir, src: &str) {
        app.settings_save(tmp.path(), &Config::parse(src).unwrap());
    }

    #[test]
    fn saving_an_edge_naming_an_unknown_type_is_refused_in_the_loaders_words() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "to", "nonsense");
        let refusal = loader_refusal(EDGES_SAVE_SRC, &app);

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(app.settings_footer_error.as_deref(), Some(&*refusal));
    }

    #[test]
    fn saving_an_edge_naming_an_unknown_relationship_is_refused_in_the_loaders_words() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "via", "nonsense");
        let refusal = loader_refusal(EDGES_SAVE_SRC, &app);

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(app.settings_footer_error.as_deref(), Some(&*refusal));
    }

    #[test]
    fn saving_a_required_edge_widened_to_a_wildcard_from_is_refused_in_the_loaders_words() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "from", "*");
        let refusal = loader_refusal(EDGES_SAVE_SRC, &app);

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(app.settings_footer_error.as_deref(), Some(&*refusal));
    }

    #[test]
    fn saving_edges_that_disagree_on_traversal_is_refused_in_the_loaders_words() {
        let (tmp, mut app) = save_app(TWO_EDGES_SAVE_SRC);
        cycle_edge_qualifier(&mut app, 1, EdgeKey::Traversal, "related");
        let refusal = loader_refusal(TWO_EDGES_SAVE_SRC, &app);

        save_edges(&mut app, &tmp, TWO_EDGES_SAVE_SRC);

        assert_eq!(app.settings_footer_error.as_deref(), Some(&*refusal));
    }

    /// Set a drilled row's `required`/`traversal` through the enum cycler's
    /// shared write, which reads the focused field, so the focus moves first.
    fn cycle_edge_qualifier(app: &mut App, row: usize, key: EdgeKey, variant: &str) {
        let label = match key {
            EdgeKey::Required => "required",
            EdgeKey::Traversal => "traversal",
            other => panic!("{other:?} is not a cycled qualifier"),
        };
        focus_edge_row_field(app, row, label);
        app.settings_set_enum_variant(&FieldPath::Edge { index: row, key }, variant);
        assert!(app.settings_dirty, "the edit dirtied the buffer");
    }

    // The refusal is the whole outcome: the file keeps its bytes and the buffer
    // keeps the edit, so the designer corrects it in place rather than retyping
    // it.
    #[test]
    fn a_refused_edge_edit_writes_nothing_and_keeps_the_edit() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "to", "nonsense");

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(
            read_config_file(&tmp),
            EDGES_SAVE_SRC,
            "nothing was written"
        );
        assert!(app.settings_dirty, "the buffer stays dirty");
        assert_eq!(
            app.settings_buffer.edges[0].to,
            TypeSelector::Types(vec!["nonsense".to_string()]),
            "the buffer still holds the refused edit"
        );
        assert!(!app.config_reload_request, "no reload on a refusal");
    }

    // --- ITERATION-389 Task 4: the cursor lands on the edge field at fault ---

    #[test]
    fn a_refused_unknown_type_on_from_lands_the_cursor_on_from() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "from", "nonsense");

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(focused_path(&app), edge_path(EdgeKey::From));
    }

    #[test]
    fn a_refused_unknown_type_on_to_lands_the_cursor_on_to() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "to", "nonsense");

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(focused_path(&app), edge_path(EdgeKey::To));
    }

    #[test]
    fn a_refused_unknown_relationship_lands_the_cursor_on_via() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "via", "nonsense");

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(focused_path(&app), edge_path(EdgeKey::Via));
    }

    // A wildcard `from` is a legal position on its own; the row is refused
    // because `required` is set on it, and clearing `required` is the fix that
    // keeps the position the designer just declared.
    #[test]
    fn a_refused_required_wildcard_from_lands_the_cursor_on_required() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        commit_edge_edit(&mut app, "from", "*");

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert_eq!(focused_path(&app), edge_path(EdgeKey::Required));
    }

    // A pairwise refusal has no single culprit -- either row can be narrowed or
    // changed -- so the cursor goes to the row the message names first, which is
    // the earlier row whether or not it is the one just edited.
    #[test]
    fn a_refused_requiredness_tie_lands_the_cursor_on_the_first_rows_required() {
        let (tmp, mut app) = save_app(TWO_EDGES_SAVE_SRC);
        cycle_edge_qualifier(&mut app, 1, EdgeKey::Required, "error");

        save_edges(&mut app, &tmp, TWO_EDGES_SAVE_SRC);

        assert_eq!(focused_path(&app), edge_path(EdgeKey::Required));
    }

    #[test]
    fn a_refused_traversal_disagreement_lands_the_cursor_on_the_first_rows_traversal() {
        let (tmp, mut app) = save_app(TWO_EDGES_SAVE_SRC);
        cycle_edge_qualifier(&mut app, 1, EdgeKey::Traversal, "related");

        save_edges(&mut app, &tmp, TWO_EDGES_SAVE_SRC);

        assert_eq!(focused_path(&app), edge_path(EdgeKey::Traversal));
    }

    // A violation no edge arm claims must not be attributed to an edge field:
    // the sqids arm owns it, and the buffer's edges are all sound.
    #[test]
    fn a_non_edge_violation_does_not_land_the_cursor_on_an_edge_field() {
        let (tmp, mut app) = save_app(EDGES_SAVE_SRC);
        app.settings_buffer.documents.types[0].numbering = NumberingStrategy::Sqids;
        app.settings_buffer.documents.sqids = None;
        app.settings_dirty = true;

        save_edges(&mut app, &tmp, EDGES_SAVE_SRC);

        assert!(
            !matches!(focused_path(&app), FieldPath::Edge { .. }),
            "landed on {:?}",
            focused_path(&app)
        );
    }

    // AC9: a save that would violate a cross-field constraint shows the footer,
    // jumps to the offending field, leaves the buffer dirty, and writes NOTHING.
    #[test]
    fn ac9_failed_save_shows_footer_jumps_and_does_not_write() {
        // github-issues store with no [github] section is a parse-time violation.
        let (tmp, mut app) = save_app(SAVE_SRC);
        app.settings_buffer.documents.types[0].store = StoreBackend::GithubIssues;
        app.settings_dirty = true;
        let before = read_config_file(&tmp);

        app.settings_save(tmp.path(), &Config::default());

        // No write happened -- the file is byte-for-byte unchanged.
        assert_eq!(read_config_file(&tmp), before, "no write on failed save");

        let msg = app
            .settings_footer_error
            .as_deref()
            .expect("footer error set on failed save");
        assert!(
            msg.contains("github") || msg.contains("[github]"),
            "footer should name the github constraint, got: {msg}"
        );

        // Focus jumped to the offending type's `store` field: cat 1, drilled into
        // type 0, cursor on the Store field.
        assert_eq!(app.settings_category, 1);
        assert_eq!(app.settings_drill, Some(0));
        let fields =
            crate::tui::views::panels::settings_fields(1, 0, Some(0), &app.settings_buffer);
        assert!(
            matches!(
                fields[app.settings_field].path,
                FieldPath::Type {
                    key: TypeKey::Store,
                    ..
                }
            ),
            "cursor should land on the store field"
        );

        assert!(app.settings_dirty, "buffer stays dirty after a failed save");
        assert!(
            !app.config_reload_request,
            "no reload is triggered on a failed save"
        );
    }

    // AC9 (sqids variant): a Sqids type with no sqids section jumps to that type's
    // numbering field and writes nothing.
    #[test]
    fn ac9_failed_save_sqids_missing_section_jumps_to_numbering() {
        let (tmp, mut app) = save_app(SAVE_SRC);
        app.settings_buffer.documents.types[0].numbering = NumberingStrategy::Sqids;
        app.settings_buffer.documents.sqids = None;
        app.settings_dirty = true;
        let before = read_config_file(&tmp);

        app.settings_save(tmp.path(), &Config::default());

        assert_eq!(read_config_file(&tmp), before, "no write on failed save");
        let msg = app
            .settings_footer_error
            .as_deref()
            .expect("footer error set");
        assert!(
            msg.contains("sqids") || msg.contains("salt"),
            "footer should name the sqids constraint, got: {msg}"
        );

        // Section absent => salt isn't focusable, so we land on the type's
        // numbering field (cat 1, drill 0).
        assert_eq!(app.settings_category, 1);
        assert_eq!(app.settings_drill, Some(0));
        let fields =
            crate::tui::views::panels::settings_fields(1, 0, Some(0), &app.settings_buffer);
        assert!(
            matches!(
                fields[app.settings_field].path,
                FieldPath::Type {
                    key: TypeKey::Numbering,
                    ..
                }
            ),
            "cursor should land on the numbering field"
        );
        assert!(app.settings_dirty);
    }

    // AC7 (ITERATION-189): auto-scaffolding does NOT weaken the salt requirement.
    // A buffer mirroring the AC1 end-state (type numbering = sqids, scaffolded
    // [numbering.sqids] with an empty salt) is rejected by save: the file is not
    // written, the footer error names the salt, and the buffer stays dirty.
    #[test]
    fn ac7_save_still_rejects_scaffolded_empty_salt() {
        const SQIDS_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "seed"
min_length = 3

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;
        let (tmp, mut app) = save_app(SQIDS_SRC);
        // Mirror the scaffold end-state: the salt is blank.
        app.settings_buffer.documents.sqids.as_mut().unwrap().salt = String::new();
        app.settings_dirty = true;
        let before = read_config_file(&tmp);

        app.settings_save(tmp.path(), &Config::default());

        assert_eq!(
            read_config_file(&tmp),
            before,
            "no write on a save rejected for an empty salt"
        );
        let msg = app
            .settings_footer_error
            .as_deref()
            .expect("footer error set on rejected save");
        assert!(
            msg.contains("salt"),
            "footer should name the salt constraint, got: {msg}"
        );
        assert!(
            app.settings_dirty,
            "buffer stays dirty after a rejected save"
        );
        assert!(
            !app.config_reload_request,
            "no reload triggered on a rejected save"
        );
    }

    // ITERATION-189: cycling a type's store to `github-issues` (no [github] in
    // source) scaffolds the section into the buffer; the save writer must now
    // fabricate the absent [github] section so the rendered TOML re-parses.
    #[test]
    fn save_fabricates_github_section_after_store_cycled_to_github_issues() {
        let (tmp, mut app) = save_app(SAVE_SRC);
        // Cycle the rfc type's store via the editor path: cat 1, drill 0, field 7.
        app.settings_category = 1;
        app.settings_drill = Some(0);
        app.settings_field = 7;
        app.settings_space(); // filesystem -> github-issues, scaffolds github buffer

        assert_eq!(
            app.settings_buffer.documents.types[0].store,
            StoreBackend::GithubIssues
        );
        assert!(app.settings_buffer.documents.github.is_some());

        app.settings_save(tmp.path(), &Config::default());

        let out = read_config_file(&tmp);
        assert!(
            out.contains("[github]"),
            "writer must fabricate the absent [github] section, got:\n{out}"
        );
        let reparsed = Config::parse(&out).expect("fabricated config re-parses");
        let gh = reparsed
            .documents
            .github
            .expect("re-parsed config carries the github section");
        assert_eq!(gh.cache_ttl, 60, "scaffolded cache_ttl round-trips");
        assert_eq!(gh.repo, None, "no repo key fabricated when None");
        assert!(!app.settings_dirty, "dirty clears on a successful save");
        assert_eq!(app.settings_footer_error, None, "footer clears on success");
    }

    // ITERATION-189: cycling a type's numbering to `reserved` (no
    // [numbering.reserved] in source) scaffolds the section; the writer must
    // fabricate it (as a sub-table of [numbering]) so the save re-parses.
    #[test]
    fn save_fabricates_reserved_section_after_numbering_cycled_to_reserved() {
        let (tmp, mut app) = save_app(SAVE_SRC);
        // Numbering EnumCycle is cat 1, drill 0, field 5. Two hops reach reserved
        // (incremental -> sqids -> reserved); the first hop scaffolds sqids.
        app.settings_category = 1;
        app.settings_drill = Some(0);
        app.settings_field = 5;
        app.settings_space(); // -> sqids (scaffolds sqids buffer)
        app.settings_space(); // -> reserved (scaffolds reserved buffer)

        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Reserved
        );
        // The dangling sqids scaffold (empty salt) would otherwise reject the save;
        // drop it so we isolate the reserved-fabrication behaviour.
        app.settings_buffer.documents.sqids = None;
        let reserved = app
            .settings_buffer
            .documents
            .reserved
            .as_ref()
            .expect("reserved scaffolded");
        assert_eq!(reserved.remote, "origin");

        app.settings_save(tmp.path(), &Config::default());

        let out = read_config_file(&tmp);
        assert!(
            out.contains("[numbering.reserved]"),
            "writer must fabricate the absent [numbering.reserved] section, got:\n{out}"
        );
        let reparsed = Config::parse(&out).expect("fabricated config re-parses");
        let r = reparsed
            .documents
            .reserved
            .expect("re-parsed config carries the reserved section");
        assert_eq!(r.remote, "origin");
        assert_eq!(r.format, ReservedFormat::Incremental);
        assert_eq!(r.max_retries, 5);
        assert!(!app.settings_dirty, "dirty clears on a successful save");
        assert_eq!(app.settings_footer_error, None, "footer clears on success");
    }

    // ITERATION-189: cycling numbering to `sqids` then filling the scaffolded salt
    // produces a buffer whose [numbering.sqids] section is absent from source; the
    // writer must fabricate it (sub-table of [numbering]) so the save re-parses.
    #[test]
    fn save_fabricates_sqids_section_after_salt_filled() {
        let (tmp, mut app) = save_app(SAVE_SRC);
        app.settings_category = 1;
        app.settings_drill = Some(0);
        app.settings_field = 5;
        app.settings_space(); // incremental -> sqids, scaffolds empty-salt sqids buffer

        assert_eq!(
            app.settings_buffer.documents.types[0].numbering,
            NumberingStrategy::Sqids
        );
        // Fill the scaffolded salt so the save is not rejected for an empty salt.
        app.settings_buffer.documents.sqids.as_mut().unwrap().salt = "filled-seed".to_string();

        app.settings_save(tmp.path(), &Config::default());

        let out = read_config_file(&tmp);
        assert!(
            out.contains("[numbering.sqids]"),
            "writer must fabricate the absent [numbering.sqids] section, got:\n{out}"
        );
        let reparsed = Config::parse(&out).expect("fabricated config re-parses");
        let s = reparsed
            .documents
            .sqids
            .expect("re-parsed config carries the sqids section");
        assert_eq!(s.salt, "filled-seed");
        assert_eq!(s.min_length, 3);
        assert!(!app.settings_dirty, "dirty clears on a successful save");
        assert_eq!(app.settings_footer_error, None, "footer clears on success");
    }

    // --- Document-impact save guard (RFC-023 slice 6 / ITERATION-191) ---

    fn impact_doc_md(title: &str, ty: &str) -> String {
        format!(
            concat!(
                "---\n",
                "title: \"{title}\"\n",
                "type: {ty}\n",
                "status: draft\n",
                "author: \"test\"\n",
                "date: 2026-01-01\n",
                "tags: []\n",
                "---\n",
                "Body.\n",
            ),
            title = title,
            ty = ty,
        )
    }

    /// Build an `App` whose store has `count` docs of `ty` written on disk under a
    /// fresh TempDir, with `.lazyspec.toml` rendered from `Config::default()`. The
    /// buffer is seeded from the on-disk config (clean). Returns the TempDir (so it
    /// outlives the app), the app, and a clone of the on-disk config to pass to
    /// `settings_save` as the session config.
    fn impact_app(docs: &[(&str, usize)]) -> (tempfile::TempDir, App, Config) {
        let on_disk = Config::default();
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), on_disk.to_toml().unwrap()).unwrap();
        for (ty_name, count) in docs {
            let td = on_disk
                .documents
                .types
                .iter()
                .find(|t| &t.name == ty_name)
                .expect("type in config");
            let dir = root.join(&td.dir);
            std::fs::create_dir_all(&dir).unwrap();
            for i in 1..=*count {
                let file = dir.join(format!("{}-{:03}-doc.md", td.prefix, i));
                std::fs::write(&file, impact_doc_md(&format!("{ty_name} {i}"), ty_name)).unwrap();
            }
        }
        let store = Store::load(root, &on_disk).unwrap();
        let mut app = make_test_app(0);
        app.store = store;
        app.settings_buffer = on_disk.clone();
        app.settings_dirty = false;
        (tmp, app, on_disk)
    }

    fn buffer_type_dir<'a>(app: &'a mut App, name: &str) -> &'a mut String {
        &mut app
            .settings_buffer
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == name)
            .unwrap()
            .dir
    }

    /// AC1: a load-bearing dir change on a type WITH docs pauses the save -- the
    /// guard activates and `.lazyspec.toml` is left byte-unchanged.
    #[test]
    fn ac1_load_bearing_change_with_docs_pauses_save_without_writing() {
        let (tmp, mut app, on_disk) = impact_app(&[("rfc", 3)]);
        let before = read_config_file(&tmp);
        *buffer_type_dir(&mut app, "rfc") = "docs/proposals".to_string();
        app.settings_dirty = true;

        app.settings_save(tmp.path(), &on_disk);

        assert!(
            app.settings_impact_confirm.active,
            "guard must be flagged for a load-bearing change on a type with docs"
        );
        assert_eq!(
            read_config_file(&tmp),
            before,
            "no write may happen while the impact guard is pending"
        );
        assert!(app.settings_dirty, "pending edit is retained");
    }

    /// AC3: confirming the guard commits the buffer atomically (new dir on disk),
    /// clears the guard + dirty flag, and moves/renames NO document files.
    #[test]
    fn ac3_confirm_commits_buffer_and_leaves_docs_in_place() {
        let (tmp, mut app, on_disk) = impact_app(&[("rfc", 3)]);
        let root = tmp.path();
        let original_files: Vec<PathBuf> = (1..=3)
            .map(|i| root.join("docs/rfcs").join(format!("RFC-{:03}-doc.md", i)))
            .collect();
        for f in &original_files {
            assert!(f.exists(), "fixture doc must exist before save: {f:?}");
        }

        *buffer_type_dir(&mut app, "rfc") = "docs/proposals".to_string();
        app.settings_dirty = true;
        app.settings_save(root, &on_disk);
        assert!(app.settings_impact_confirm.active);

        app.confirm_settings_impact(root);

        let out = read_config_file(&tmp);
        Config::parse(&out).unwrap();
        assert!(
            out.contains("docs/proposals"),
            "confirmed save must persist the new dir, got:\n{out}"
        );
        assert!(
            !app.settings_impact_confirm.active,
            "guard clears after confirm"
        );
        assert!(!app.settings_dirty, "dirty clears after a confirmed commit");
        assert!(app.config_reload_request, "reload raised on commit");

        // No document file was moved/renamed/renumbered: every original path still
        // exists, and the (new) dir holds no migrated docs.
        for f in &original_files {
            assert!(
                f.exists(),
                "document files must NOT be moved on confirm: {f:?}"
            );
        }
        assert!(
            !root.join("docs/proposals").exists(),
            "confirm must not create or populate the new dir"
        );
    }

    /// AC4: cancelling the guard writes nothing -- `.lazyspec.toml` is byte-identical
    /// to its pre-save snapshot, the buffer keeps the pending value, and the buffer
    /// stays dirty.
    #[test]
    fn ac4_cancel_writes_nothing_and_retains_pending_edit() {
        let (tmp, mut app, on_disk) = impact_app(&[("rfc", 3)]);
        *buffer_type_dir(&mut app, "rfc") = "docs/proposals".to_string();
        app.settings_dirty = true;
        app.settings_save(tmp.path(), &on_disk);
        assert!(app.settings_impact_confirm.active);
        let snapshot = read_config_file(&tmp);

        app.cancel_settings_impact();

        assert!(
            !app.settings_impact_confirm.active,
            "guard clears after cancel"
        );
        assert_eq!(
            read_config_file(&tmp),
            snapshot,
            "cancel must not write .lazyspec.toml"
        );
        assert_eq!(
            app.settings_buffer
                .documents
                .types
                .iter()
                .find(|t| t.name == "rfc")
                .unwrap()
                .dir,
            "docs/proposals",
            "buffer retains the pending dir after cancel"
        );
        assert!(app.settings_dirty, "buffer stays dirty after cancel");
    }

    /// AC5a: a change to only non-load-bearing fields (icon/plural) commits with no
    /// guard, identical to a guard-free save.
    #[test]
    fn ac5a_non_load_bearing_change_commits_without_guard() {
        let (tmp, mut app, on_disk) = impact_app(&[("rfc", 3)]);
        let rfc = app
            .settings_buffer
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == "rfc")
            .unwrap();
        rfc.icon = Some("★".to_string());
        rfc.plural = "requests".to_string();
        app.settings_dirty = true;

        app.settings_save(tmp.path(), &on_disk);

        assert!(
            !app.settings_impact_confirm.active,
            "non-load-bearing edits must not raise the guard"
        );
        let out = read_config_file(&tmp);
        assert!(
            out.contains("requests"),
            "non-load-bearing edit must be written, got:\n{out}"
        );
        assert!(!app.settings_dirty, "dirty clears on the committed save");
    }

    /// AC5b: a load-bearing change on a type with ZERO docs commits with no guard.
    #[test]
    fn ac5b_load_bearing_change_on_zero_doc_type_commits_without_guard() {
        // Only rfc docs on disk; story has zero docs.
        let (tmp, mut app, on_disk) = impact_app(&[("rfc", 3)]);
        *buffer_type_dir(&mut app, "story") = "docs/tickets".to_string();
        app.settings_dirty = true;

        app.settings_save(tmp.path(), &on_disk);

        assert!(
            !app.settings_impact_confirm.active,
            "a load-bearing change on a zero-doc type must not raise the guard"
        );
        let out = read_config_file(&tmp);
        assert!(
            out.contains("docs/tickets"),
            "zero-doc load-bearing edit must be written, got:\n{out}"
        );
        assert!(!app.settings_dirty, "dirty clears on the committed save");
    }

    // --- AC3/AC4 impact-guard key routing (ITERATION-191 Task 4) ---

    /// Arm the document-impact guard via a real `settings_save`: a load-bearing dir
    /// change on a type with docs. Returns the app parked with the guard active and
    /// a pending dirty buffer, ready for the key path.
    fn armed_impact_app() -> (tempfile::TempDir, App, Config) {
        let (tmp, mut app, on_disk) = impact_app(&[("rfc", 3)]);
        *buffer_type_dir(&mut app, "rfc") = "docs/proposals".to_string();
        app.settings_dirty = true;
        app.settings_save(tmp.path(), &on_disk);
        assert!(
            app.settings_impact_confirm.active,
            "fixture must arm the impact guard"
        );
        (tmp, app, on_disk)
    }

    /// AC3 via keys: Enter while the guard is active routes to confirm -- the buffer
    /// is written and the guard clears.
    #[test]
    fn impact_guard_enter_routes_to_confirm() {
        let (tmp, mut app, config) = armed_impact_app();

        app.handle_key(KeyCode::Enter, KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            !app.settings_impact_confirm.active,
            "Enter must clear the guard via confirm"
        );
        assert!(
            read_config_file(&tmp).contains("docs/proposals"),
            "Enter must commit the pending buffer"
        );
        assert!(!app.settings_dirty, "confirmed commit clears dirty");
    }

    /// AC3 via keys: 'y' while the guard is active routes to confirm.
    #[test]
    fn impact_guard_y_routes_to_confirm() {
        let (tmp, mut app, config) = armed_impact_app();

        app.handle_key(KeyCode::Char('y'), KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            !app.settings_impact_confirm.active,
            "'y' must clear the guard via confirm"
        );
        assert!(
            read_config_file(&tmp).contains("docs/proposals"),
            "'y' must commit the pending buffer"
        );
    }

    /// AC4 via keys: Esc while the guard is active routes to cancel -- nothing is
    /// written, the guard clears, and the buffer keeps its pending edit.
    #[test]
    fn impact_guard_esc_routes_to_cancel() {
        let (tmp, mut app, config) = armed_impact_app();
        let snapshot = read_config_file(&tmp);

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            !app.settings_impact_confirm.active,
            "Esc must clear the guard via cancel"
        );
        assert_eq!(
            read_config_file(&tmp),
            snapshot,
            "Esc (cancel) must not write .lazyspec.toml"
        );
        assert_eq!(
            buffer_type_dir(&mut app, "rfc"),
            "docs/proposals",
            "buffer retains the pending edit after cancel"
        );
        assert!(app.settings_dirty, "buffer stays dirty after cancel");
    }

    /// AC4 via keys: 'n' while the guard is active routes to cancel.
    #[test]
    fn impact_guard_n_routes_to_cancel() {
        let (tmp, mut app, config) = armed_impact_app();
        let snapshot = read_config_file(&tmp);

        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            !app.settings_impact_confirm.active,
            "'n' must clear the guard via cancel"
        );
        assert_eq!(
            read_config_file(&tmp),
            snapshot,
            "'n' (cancel) must not write .lazyspec.toml"
        );
        assert!(app.settings_dirty, "buffer stays dirty after cancel");
    }

    /// A non-handled key while the guard is active is swallowed: the gate keeps the
    /// guard up and nothing is written (no fall-through to normal nav / save).
    #[test]
    fn impact_guard_swallows_unhandled_key() {
        let (tmp, mut app, config) = armed_impact_app();
        let snapshot = read_config_file(&tmp);

        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            app.settings_impact_confirm.active,
            "an unhandled key must leave the guard active"
        );
        assert_eq!(
            read_config_file(&tmp),
            snapshot,
            "an unhandled key must not write .lazyspec.toml"
        );
        assert!(app.settings_dirty, "buffer stays dirty");
    }

    // --- AC10 save/discard quit prompt (ITERATION-188 Task 7) ---

    /// A settings app parked in the Settings view, not editing, not drilled, with
    /// its buffer set to the parsed `src` (clean). The TempDir holds the
    /// `.lazyspec.toml` so it outlives the app/file.
    fn quit_prompt_app(src: &str) -> (tempfile::TempDir, App) {
        let (tmp, mut app) = save_app(src);
        app.view_mode = ViewMode::Settings;
        app.settings_editing = false;
        app.settings_drill = None;
        (tmp, app)
    }

    #[test]
    fn ac10_quit_with_dirty_buffer_activates_prompt() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        app.settings_dirty = true;
        let config = Config::parse(SAVE_SRC).unwrap();

        app.handle_settings_key(KeyCode::Char('q'), KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            app.settings_quit_prompt.active,
            "q with dirty buffer opens the prompt"
        );
        assert!(
            !app.should_quit,
            "the quit is deferred until the prompt is answered"
        );
        assert_eq!(
            app.view_mode,
            ViewMode::Settings,
            "still in settings, not exited"
        );
    }

    #[test]
    fn ac10_esc_with_dirty_buffer_activates_prompt() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        app.settings_dirty = true;
        let config = Config::parse(SAVE_SRC).unwrap();

        app.handle_settings_key(KeyCode::Esc, KeyModifiers::NONE, tmp.path(), &config);

        assert!(
            app.settings_quit_prompt.active,
            "Esc with dirty buffer opens the prompt"
        );
        assert!(!app.should_quit);
        assert_eq!(app.view_mode, ViewMode::Settings);
    }

    #[test]
    fn ac10_esc_undrills_before_prompting_even_when_dirty() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        app.settings_dirty = true;
        app.settings_drill = Some(0);
        let config = Config::parse(SAVE_SRC).unwrap();

        app.handle_settings_key(KeyCode::Esc, KeyModifiers::NONE, tmp.path(), &config);

        assert_eq!(app.settings_drill, None, "Esc undrills first");
        assert!(
            !app.settings_quit_prompt.active,
            "no prompt while a drill is open"
        );
        assert!(!app.should_quit);
    }

    #[test]
    fn ac10_discard_drops_buffer_edits_quits_and_leaves_file_untouched() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        let config = Config::parse(SAVE_SRC).unwrap();
        // A dirty buffer that differs from the session config.
        app.settings_buffer.documents.naming.pattern = "edited-{title}.md".to_string();
        app.settings_dirty = true;
        app.settings_quit_prompt.active = true;
        let before = read_config_file(&tmp);

        app.handle_settings_key(KeyCode::Char('d'), KeyModifiers::NONE, tmp.path(), &config);

        // Buffer re-seeded to the session config (edit dropped).
        assert_eq!(
            app.settings_buffer.documents.naming.pattern, config.documents.naming.pattern,
            "discard re-seeds the buffer from the session config"
        );
        assert!(!app.settings_dirty, "dirty clears on discard");
        assert!(!app.settings_quit_prompt.active, "prompt closes on discard");
        assert!(app.should_quit, "discard honours the quit");
        assert_eq!(app.settings_footer_error, None);
        assert_eq!(
            read_config_file(&tmp),
            before,
            "discard performs no write -- the file is untouched"
        );
    }

    #[test]
    fn ac10_save_valid_writes_clears_dirty_quits_and_reloads() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        let config = Config::parse(SAVE_SRC).unwrap();
        app.settings_buffer.documents.naming.pattern = "{type}-{title}.md".to_string();
        app.settings_dirty = true;
        app.settings_quit_prompt.active = true;

        app.handle_settings_key(KeyCode::Char('s'), KeyModifiers::NONE, tmp.path(), &config);

        let out = read_config_file(&tmp);
        Config::parse(&out).unwrap();
        assert!(
            out.contains("{type}-{title}.md"),
            "the valid edit was written"
        );
        assert!(!app.settings_dirty, "dirty clears on a valid save");
        assert!(
            !app.settings_quit_prompt.active,
            "prompt closes on a valid save"
        );
        assert!(app.should_quit, "a successful save honours the quit");
        assert!(
            app.config_reload_request,
            "save runs the same reload-raising path as w"
        );
    }

    #[test]
    fn ac10_save_invalid_keeps_dirty_stays_in_settings_with_footer() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        let config = Config::parse(SAVE_SRC).unwrap();
        // github-issues store with no [github] section is a parse-time violation.
        app.settings_buffer.documents.types[0].store = StoreBackend::GithubIssues;
        app.settings_dirty = true;
        app.settings_quit_prompt.active = true;
        let before = read_config_file(&tmp);

        app.handle_settings_key(KeyCode::Char('s'), KeyModifiers::NONE, tmp.path(), &config);

        assert_eq!(read_config_file(&tmp), before, "no write on a failed save");
        assert!(
            app.settings_footer_error.is_some(),
            "the failed save sets the footer error"
        );
        assert!(app.settings_dirty, "buffer stays dirty after a failed save");
        assert!(
            !app.settings_quit_prompt.active,
            "prompt closes after a failed save"
        );
        assert!(!app.should_quit, "a failed save cancels the quit");
        assert_eq!(
            app.view_mode,
            ViewMode::Settings,
            "stays in settings to fix and retry"
        );
    }

    #[test]
    fn ac10_cancel_closes_prompt_keeps_buffer_and_stays() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        let config = Config::parse(SAVE_SRC).unwrap();
        app.settings_buffer.documents.naming.pattern = "edited-{title}.md".to_string();
        app.settings_dirty = true;
        app.settings_quit_prompt.active = true;

        app.handle_settings_key(KeyCode::Esc, KeyModifiers::NONE, tmp.path(), &config);

        assert!(!app.settings_quit_prompt.active, "Esc cancels the prompt");
        assert!(!app.should_quit, "cancel keeps the app running");
        assert!(app.settings_dirty, "cancel leaves the buffer dirty");
        assert_eq!(
            app.settings_buffer.documents.naming.pattern, "edited-{title}.md",
            "cancel leaves the buffer untouched"
        );
    }

    #[test]
    fn ac10_quit_when_clean_quits_without_prompting() {
        let (tmp, mut app) = quit_prompt_app(SAVE_SRC);
        app.settings_dirty = false;
        let config = Config::parse(SAVE_SRC).unwrap();

        app.handle_settings_key(KeyCode::Char('q'), KeyModifiers::NONE, tmp.path(), &config);

        assert!(app.should_quit, "a clean quit exits immediately");
        assert!(
            !app.settings_quit_prompt.active,
            "no prompt when the buffer is clean"
        );
    }

    // AC1: scaffolding NumberingSqids into a buffer with no [numbering.sqids]
    // inserts the section with parser defaults and points at the empty salt.
    #[test]
    fn scaffold_sqids_inserts_defaults_and_targets_salt() {
        let mut buffer = Config::default();
        assert!(buffer.documents.sqids.is_none());

        let result = scaffold_dependency(&mut buffer, ConfigDep::NumberingSqids);

        assert_eq!(
            result,
            Some(ScaffoldResult {
                inserted: ConfigDep::NumberingSqids,
                required_empty_field: Some(FieldPath::SqidsSalt),
            })
        );
        assert_eq!(
            buffer.documents.sqids,
            Some(SqidsConfig {
                salt: String::new(),
                min_length: 3,
            })
        );
    }

    // AC4: scaffolding NumberingReserved into a buffer with no
    // [numbering.reserved] inserts parser defaults; no required-empty field.
    #[test]
    fn scaffold_reserved_inserts_defaults_no_empty_field() {
        let mut buffer = Config::default();
        assert!(buffer.documents.reserved.is_none());

        let result = scaffold_dependency(&mut buffer, ConfigDep::NumberingReserved);

        assert_eq!(
            result,
            Some(ScaffoldResult {
                inserted: ConfigDep::NumberingReserved,
                required_empty_field: None,
            })
        );
        assert_eq!(
            buffer.documents.reserved,
            Some(ReservedConfig {
                remote: "origin".to_string(),
                format: ReservedFormat::Incremental,
                max_retries: 5,
            })
        );
    }

    // AC5: scaffolding Github into a buffer with no [github] inserts parser
    // defaults (repo None, cache_ttl 60); no required-empty field.
    #[test]
    fn scaffold_github_inserts_defaults_no_empty_field() {
        let mut buffer = Config::default();
        assert!(buffer.documents.github.is_none());

        let result = scaffold_dependency(&mut buffer, ConfigDep::Github);

        assert_eq!(
            result,
            Some(ScaffoldResult {
                inserted: ConfigDep::Github,
                required_empty_field: None,
            })
        );
        assert_eq!(
            buffer.documents.github,
            Some(GithubConfig {
                repo: None,
                cache_ttl: 60,
            })
        );
    }

    // AC6: an already-present section is left untouched and scaffolding returns
    // None, for each of the three dependencies.
    #[test]
    fn scaffold_skips_and_does_not_mutate_when_present() {
        let mut buffer = Config::default();

        let existing_sqids = SqidsConfig {
            salt: "user-salt".to_string(),
            min_length: 7,
        };
        buffer.documents.sqids = Some(existing_sqids.clone());
        assert_eq!(
            scaffold_dependency(&mut buffer, ConfigDep::NumberingSqids),
            None
        );
        assert_eq!(buffer.documents.sqids, Some(existing_sqids));

        let existing_reserved = ReservedConfig {
            remote: "upstream".to_string(),
            format: ReservedFormat::Sqids,
            max_retries: 9,
        };
        buffer.documents.reserved = Some(existing_reserved.clone());
        assert_eq!(
            scaffold_dependency(&mut buffer, ConfigDep::NumberingReserved),
            None
        );
        assert_eq!(buffer.documents.reserved, Some(existing_reserved));

        let existing_github = GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 120,
        };
        buffer.documents.github = Some(existing_github.clone());
        assert_eq!(scaffold_dependency(&mut buffer, ConfigDep::Github), None);
        assert_eq!(buffer.documents.github, Some(existing_github));
    }

    // --- ITERATION-190: settings collection management (AC1-AC7) ---

    // AC1: seeding a Document Type appends a default TypeDef and drills in.
    #[test]
    fn ac1_seed_document_type_appends_default_and_drills() {
        let mut app = settings_app(Config::default(), "Document Types", 0);
        let before = app.settings_buffer.documents.types.len();

        app.settings_seed_entry();

        assert_eq!(app.settings_buffer.documents.types.len(), before + 1);
        let last = app.settings_buffer.documents.types.last().unwrap();
        assert_eq!(last.numbering, NumberingStrategy::Incremental);
        assert_eq!(last.store, StoreBackend::Filesystem);
        assert!(last.agents.is_empty());
        assert!(last.icon.is_none());
        assert!(last.parent_type.is_none());
        assert!(!last.name.is_empty());
        assert_eq!(app.settings_entry, before);
        assert_eq!(app.settings_drill, Some(before));
        assert!(app.settings_dirty);
    }

    // AC3: seeding an override opens the key prompt without inserting; confirming
    // a non-empty key inserts the override (default normalize) and drills in.
    #[test]
    fn ac3_seed_override_prompts_then_inserts_on_confirm() {
        let mut app = settings_app(Config::default(), "Certification", 0);
        let before = app.settings_buffer.certification.overrides.len();

        app.settings_seed_override();
        assert!(app.override_key_prompt.active);
        assert_eq!(
            app.settings_buffer.certification.overrides.len(),
            before,
            "no insert before confirm"
        );
        assert!(!app.settings_dirty);

        for c in "docs/specs/SPEC-007".chars() {
            app.settings_override_type_char(c);
        }
        app.settings_confirm_override();

        let ov = app
            .settings_buffer
            .certification
            .overrides
            .get("docs/specs/SPEC-007")
            .expect("override inserted");
        assert!(ov.normalize, "default normalize is true");
        assert!(!app.override_key_prompt.active);
        assert!(app.settings_dirty);

        let mut keys: Vec<&String> = app.settings_buffer.certification.overrides.keys().collect();
        keys.sort();
        let expected = keys
            .iter()
            .position(|k| **k == "docs/specs/SPEC-007")
            .unwrap();
        assert_eq!(app.settings_entry, expected);
        assert_eq!(app.settings_drill, Some(expected));
    }

    // AC3 edge: an empty key on confirm inserts nothing.
    #[test]
    fn ac3_empty_override_key_inserts_nothing() {
        let mut app = settings_app(Config::default(), "Certification", 0);
        app.settings_seed_override();
        app.settings_confirm_override();
        assert!(app.settings_buffer.certification.overrides.is_empty());
    }

    // AC4: opening the delete confirm targets the selected entry without mutating
    // the buffer; confirming removes it and dirties.
    #[test]
    fn ac4_delete_confirm_targets_then_removes_on_confirm() {
        let mut app = settings_app(Config::default(), "Document Types", 0);
        app.settings_entry = 0;
        let target_name = app.settings_buffer.documents.types[0].name.clone();
        let before = app.settings_buffer.documents.types.len();

        app.settings_open_delete_confirm();
        assert!(app.settings_delete_confirm.active);
        assert_eq!(app.settings_delete_confirm.entry_label, target_name);
        assert_eq!(
            app.settings_delete_confirm.target,
            SettingsDeleteTarget::Index(0)
        );
        assert_eq!(
            app.settings_buffer.documents.types.len(),
            before,
            "buffer unchanged until confirm"
        );

        app.settings_confirm_delete();
        assert_eq!(app.settings_buffer.documents.types.len(), before - 1);
        assert!(!app
            .settings_buffer
            .documents
            .types
            .iter()
            .any(|t| t.name == target_name));
        assert!(app.settings_dirty);
        assert!(!app.settings_delete_confirm.active);
    }

    // AC5: cancelling the delete confirm leaves the buffer intact.
    #[test]
    fn ac5_delete_confirm_cancel_leaves_buffer_intact() {
        let mut app = settings_app(Config::default(), "Document Types", 0);
        let before = app.settings_buffer.documents.types.clone();

        app.settings_open_delete_confirm();
        assert!(app.settings_delete_confirm.active);

        app.settings_close_delete_confirm();
        assert_eq!(app.settings_buffer.documents.types, before);
        assert!(!app.settings_delete_confirm.active);
        assert_eq!(app.settings_drill, None);
    }

    // AC6: ADR-011 guard refuses to delete the last relationship, with no confirm.
    #[test]
    fn ac6_delete_last_relationship_is_refused() {
        let config = Config {
            relationships: vec![RelationshipDef {
                name: "related-to".to_string(),
                inverse: None,
                github_native: None,
                traversal: None,
            }],
            ..Config::default()
        };
        let mut app = settings_app(config, "Relationships", 0);
        app.settings_entry = 0;

        app.settings_open_delete_confirm();

        assert!(
            !app.settings_delete_confirm.active,
            "no confirm shown for last relationship"
        );
        assert_eq!(app.settings_buffer.relationships.len(), 1);
    }

    // AC7: with >=2 relationships, delete is allowed and removes one on confirm.
    #[test]
    fn ac7_delete_relationship_allowed_when_more_than_one() {
        let config = Config {
            relationships: vec![
                RelationshipDef {
                    name: "implements".to_string(),
                    inverse: Some("implemented-by".to_string()),
                    github_native: None,
                    traversal: None,
                },
                RelationshipDef {
                    name: "related-to".to_string(),
                    inverse: None,
                    github_native: None,
                    traversal: None,
                },
            ],
            ..Config::default()
        };
        let mut app = settings_app(config, "Relationships", 0);
        app.settings_entry = 0;
        let removed = app.settings_buffer.relationships[0].name.clone();

        app.settings_open_delete_confirm();
        assert!(app.settings_delete_confirm.active);

        app.settings_confirm_delete();
        assert_eq!(app.settings_buffer.relationships.len(), 1);
        assert!(!app
            .settings_buffer
            .relationships
            .iter()
            .any(|r| r.name == removed));
    }

    // A save fixture with two relationships so a non-last delete is legal under the
    // ADR-011 guard.
    const COLLECTION_SAVE_SRC: &str = r#"# lazyspec collection fixture
[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"
"#;

    // AC8 (STORY-141): a save persists buffer-only collection add/remove. Seed a
    // Document Type, give it save-valid non-colliding fields, delete a non-last
    // relationship, then save -- the written file re-parses, holds the new type,
    // drops the deleted relationship (>=1 remaining), and dirty clears.
    #[test]
    fn ac8_save_persists_collection_add_and_remove() {
        let (tmp, mut app) = save_app(COLLECTION_SAVE_SRC);

        // Seed a Document Type (cat 1) and make its placeholder fields save-valid
        // and non-colliding with the existing `rfc`.
        app.settings_category = 1;
        app.settings_seed_entry();
        let seeded = app.settings_buffer.documents.types.last_mut().unwrap();
        seeded.name = "adr".to_string();
        seeded.plural = "adrs".to_string();
        seeded.dir = "docs/adrs".to_string();
        seeded.prefix = "ADR".to_string();

        // Delete the FIRST (non-last) relationship via the confirm flow (cat 2).
        app.settings_category = 2;
        app.settings_entry = 0;
        let removed = app.settings_buffer.relationships[0].name.clone();
        app.settings_open_delete_confirm();
        assert!(app.settings_delete_confirm.active);
        app.settings_confirm_delete();

        app.settings_save(tmp.path(), &Config::default());

        // The written file is a valid config.
        let out = read_config_file(&tmp);
        let reparsed = Config::parse(&out).unwrap();

        // The added type survived the round-trip.
        assert!(
            reparsed.type_by_name("adr").is_some(),
            "added type should be persisted"
        );
        // The deleted relationship is gone, but at least one remains.
        assert!(
            reparsed.relationship_by_name(&removed).is_none(),
            "deleted relationship should be absent"
        );
        assert!(
            !reparsed.relationships.is_empty(),
            "at least one relationship remains"
        );

        assert!(!app.settings_dirty, "dirty clears on a successful save");
        assert_eq!(app.settings_footer_error, None);
    }

    // ITERATION-382 asserted that a save left a rules-carrying config's rules
    // intact, the panel having lost its rules editor. STORY-259 deletes the
    // question rather than the answer: strict load refuses such a config, so
    // the panel — which only ever renders a `Config` the load path handed it —
    // can no longer be given one. `strict_load_refuses_a_config_declaring_rules_and_names_fix_config`
    // in `engine::config` is where that now lives.

    // --- RFC-023 slice 7 / ITERATION-192: Interface category + zone ordering ---

    // AC2: the ascii_diagrams boolean toggles through the slice-3 Space path and
    // dirties the buffer. (statusbar.enabled is covered by
    // ac2_toggle_statusbar_enabled_flips_and_dirties.)
    #[test]
    fn iter192_ac2_ascii_diagrams_toggle_flips_and_dirties() {
        let mut config = config_one_type();
        config.ui.ascii_diagrams = false;
        let mut app = settings_app(config, "Interface", 0); // ascii_diagrams

        app.settings_space();
        assert!(app.settings_buffer.ui.ascii_diagrams);
        assert!(app.settings_dirty);

        app.settings_space();
        assert!(!app.settings_buffer.ui.ascii_diagrams, "Space flips back");
    }

    // AC3: max_expanded_height bounded-numeric edit rejects out-of-range (keeping
    // the prior value) and accepts a valid value.
    #[test]
    fn iter192_ac3_max_expanded_height_rejects_then_accepts() {
        let config = config_one_type(); // [tui.multiline] absent -> default 5
        let mut app = settings_app(config, "Interface", 5); // multiline.max_expanded_height
        assert_eq!(app.settings_buffer.ui.multiline.max_expanded_height, 5);

        // Reject "0" (below min 1): prior value retained, error set, still editing.
        app.settings_start_edit();
        app.settings_edit_input.clear();
        type_chars(&mut app, "0");
        app.settings_confirm_edit();
        assert_eq!(app.settings_buffer.ui.multiline.max_expanded_height, 5);
        assert!(app.settings_edit_error.is_some());
        assert!(app.settings_editing);

        // Accept "8".
        app.settings_edit_input.clear();
        type_chars(&mut app, "8");
        app.settings_confirm_edit();
        assert_eq!(app.settings_buffer.ui.multiline.max_expanded_height, 8);
        assert!(app.settings_dirty);
        assert!(!app.settings_editing);
    }

    // AC4: opening a zone editor on an unset zone seeds it from the RFC-022 default
    // names; add/remove/reorder then commit writes the chosen order into the buffer
    // and dirties it.
    #[test]
    fn iter192_ac4_zone_ordering_round_trip_into_buffer() {
        use crate::tui::state::forms::ZonePane;
        use crate::tui::views::status_bar::STATUS_BAR_DEFAULT_LEFT;

        let config = config_one_type(); // statusbar.left is None
        let mut app = settings_app(config, "Interface", 2); // statusbar.left

        // Enter opens the zone editor (routed via start_edit).
        app.settings_start_edit();
        let editor = app
            .settings_zone_editor
            .as_ref()
            .expect("zone editor opens for a ZoneOrdering field");
        let defaults: Vec<String> = STATUS_BAR_DEFAULT_LEFT
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            editor.selected, defaults,
            "unset zone seeds from RFC-022 defaults"
        );

        // Remove the first selected name (default left = [mode, type_filter, doc_count]).
        let z = app.settings_zone_editor.as_mut().unwrap();
        z.pane = ZonePane::Selected;
        z.cursor = 0;
        let removed = z.selected[0].clone();
        z.remove();
        assert!(!z.selected.contains(&removed));
        assert!(z.available.contains(&removed));

        // Add git_branch from Available, then move it up one.
        let z = app.settings_zone_editor.as_mut().unwrap();
        z.pane = ZonePane::Available;
        z.cursor = z
            .available
            .iter()
            .position(|n| n == "git_branch")
            .expect("git_branch available");
        z.add();
        z.pane = ZonePane::Selected;
        z.cursor = z.selected.len() - 1; // git_branch landed at the end
        z.move_up();
        let expected = z.selected.clone();

        app.settings_commit_zone();
        assert!(
            app.settings_zone_editor.is_none(),
            "commit closes the editor"
        );
        assert_eq!(
            app.settings_buffer.ui.statusbar.left,
            Some(expected),
            "committed order is written to the buffer"
        );
        assert!(app.settings_dirty);
    }

    // AC4: a zone the user never opens stays None; an explicitly cleared zone
    // persists as Some(vec![]); both survive an atomic save round-trip.
    #[test]
    fn iter192_ac4_untouched_vs_cleared_zone_persist() {
        use crate::tui::state::forms::ZonePane;

        const SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"

[tui.statusbar]
enabled = true
left = ["mode"]
center = ["warnings"]
"#;
        let (tmp, mut app) = save_app(SRC);
        app.settings_category = App::settings_category_index("Interface");

        // Clear `center`: open its editor, remove all, commit -> Some(vec![]).
        app.settings_field = 3; // statusbar.center
        app.settings_start_edit();
        let z = app.settings_zone_editor.as_mut().expect("center editor");
        z.pane = ZonePane::Selected;
        while !z.selected.is_empty() {
            z.cursor = 0;
            z.remove();
        }
        app.settings_commit_zone();
        assert_eq!(
            app.settings_buffer.ui.statusbar.center,
            Some(vec![]),
            "cleared zone is an explicit empty list"
        );

        // `right` is never opened -> stays None (untouched).
        assert_eq!(app.settings_buffer.ui.statusbar.right, None);

        app.settings_dirty = true;
        app.settings_save(tmp.path(), &Config::parse(SRC).unwrap());
        assert_eq!(app.settings_footer_error, None, "save succeeds");

        let out = read_config_file(&tmp);
        let reparsed = Config::parse(&out).unwrap();
        // left untouched in this test -> preserved as written.
        assert_eq!(
            reparsed.ui.statusbar.left,
            Some(vec!["mode".to_string()]),
            "untouched left preserved"
        );
        // right never set -> remains absent (None).
        assert_eq!(
            reparsed.ui.statusbar.right, None,
            "untouched right stays unset"
        );
        // cleared center -> absent array per slice-3 list semantics (empty == absent).
        assert!(
            reparsed.ui.statusbar.center.unwrap_or_default().is_empty(),
            "cleared center round-trips as empty/absent"
        );
    }

    // AC5: the ordering editor only ever surfaces RFC-022 vocabulary -- both the
    // seeded selected set and the available set are subsets of STATUS_BAR_COMPONENTS,
    // and adding can only ever move a const name into selected.
    #[test]
    fn iter192_ac5_editor_offers_only_vocabulary() {
        use crate::tui::state::forms::{FieldPath, ZoneOrderingEditor, ZonePane};
        use crate::tui::views::status_bar::{STATUS_BAR_COMPONENTS, STATUS_BAR_DEFAULT_RIGHT};

        let vocab: std::collections::HashSet<&str> =
            STATUS_BAR_COMPONENTS.iter().copied().collect();
        let mut editor =
            ZoneOrderingEditor::new(FieldPath::StatusbarRight, None, STATUS_BAR_DEFAULT_RIGHT);

        for name in editor.selected.iter().chain(editor.available.iter()) {
            assert!(
                vocab.contains(name.as_str()),
                "{name} is offered but not in the RFC-022 vocabulary"
            );
        }

        // Exhaustively add everything available; selected must remain within vocab
        // and never exceed the full vocabulary.
        editor.pane = ZonePane::Available;
        while !editor.available.is_empty() {
            editor.cursor = 0;
            editor.add();
        }
        assert!(editor.available.is_empty());
        assert_eq!(editor.selected.len(), STATUS_BAR_COMPONENTS.len());
        for name in &editor.selected {
            assert!(vocab.contains(name.as_str()));
        }
    }

    fn dummy_variant_picker() -> SettingsVariantPicker {
        SettingsVariantPicker::new(FieldPath::Naming, &["sqids", "reserved"], 0)
    }

    fn dummy_zone_editor() -> ZoneOrderingEditor {
        ZoneOrderingEditor::new(FieldPath::Naming, None, &["branch"])
    }

    fn dummy_scaffold_offer() -> ScaffoldResult {
        ScaffoldResult {
            inserted: ConfigDep::Github,
            required_empty_field: None,
        }
    }

    #[test]
    fn active_key_context_defaults_to_types() {
        let app = make_test_app(1);
        assert_eq!(app.active_key_context(), KeyContext::Types);
    }

    #[test]
    fn active_key_context_view_modes() {
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Filters;
        assert_eq!(app.active_key_context(), KeyContext::Filters);
        app.view_mode = ViewMode::Graph;
        assert_eq!(app.active_key_context(), KeyContext::Graph);
        app.view_mode = ViewMode::Settings;
        assert_eq!(app.active_key_context(), KeyContext::Settings);
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn active_key_context_metrics_mode_falls_through_to_types() {
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Metrics;
        assert_eq!(app.active_key_context(), KeyContext::Types);
    }

    #[test]
    fn active_key_context_overlay_bools() {
        let mut app = make_test_app(1);
        app.show_warnings = true;
        assert_eq!(app.active_key_context(), KeyContext::Warnings);

        let mut app = make_test_app(1);
        app.create_form.active = true;
        assert_eq!(app.active_key_context(), KeyContext::CreateForm);

        let mut app = make_test_app(1);
        app.delete_confirm.active = true;
        assert_eq!(app.active_key_context(), KeyContext::DeleteConfirm);

        let mut app = make_test_app(1);
        app.override_key_prompt.active = true;
        assert_eq!(app.active_key_context(), KeyContext::OverrideKeyPrompt);

        let mut app = make_test_app(1);
        app.settings_delete_confirm.active = true;
        assert_eq!(app.active_key_context(), KeyContext::SettingsDeleteConfirm);

        let mut app = make_test_app(1);
        app.settings_impact_confirm.active = true;
        assert_eq!(app.active_key_context(), KeyContext::SettingsImpact);

        let mut app = make_test_app(1);
        app.status_picker.active = true;
        assert_eq!(app.active_key_context(), KeyContext::StatusPicker);

        let mut app = make_test_app(1);
        app.link_editor.active = true;
        assert_eq!(app.active_key_context(), KeyContext::LinkEditor);

        let mut app = make_test_app(1);
        app.provenance_editor.active = true;
        assert_eq!(app.active_key_context(), KeyContext::ProvenanceEditor);
    }

    #[test]
    fn active_key_context_gh_conflict() {
        let mut app = make_test_app(1);
        app.gh_conflict_message = Some("boom".to_string());
        assert_eq!(app.active_key_context(), KeyContext::GhConflict);
    }

    #[test]
    fn active_key_context_search_and_fullscreen() {
        let mut app = make_test_app(1);
        app.search_mode = true;
        assert_eq!(app.active_key_context(), KeyContext::Search);

        let mut app = make_test_app(1);
        app.fullscreen_doc = true;
        assert_eq!(app.active_key_context(), KeyContext::Fullscreen);
    }

    #[test]
    fn active_key_context_gh_conflict_outranks_lower_overlays() {
        // GhConflict sits at the top of the ladder, so it wins even when a lower
        // overlay is also active.
        let mut app = make_test_app(1);
        app.gh_conflict_message = Some("boom".to_string());
        app.create_form.active = true;
        app.search_mode = true;
        assert_eq!(app.active_key_context(), KeyContext::GhConflict);
    }

    #[test]
    fn active_key_context_show_help_is_transparent() {
        // Help is an overlay drawn on top of a context; it must not change which
        // context is reported (matches the skipped show_help short-circuit).
        let mut app = make_test_app(1);
        app.show_help = true;
        assert_eq!(app.active_key_context(), KeyContext::Types);

        app.view_mode = ViewMode::Settings;
        assert_eq!(app.active_key_context(), KeyContext::Settings);

        app.view_mode = ViewMode::Types;
        app.search_mode = true;
        assert_eq!(app.active_key_context(), KeyContext::Search);
    }

    #[test]
    fn active_key_context_settings_substate_precedence() {
        // Each settings sub-state in isolation.
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_quit_prompt.active = true;
        assert_eq!(app.active_key_context(), KeyContext::SettingsQuitPrompt);

        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_editing = true;
        assert_eq!(app.active_key_context(), KeyContext::SettingsEditing);

        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_zone_editor = Some(dummy_zone_editor());
        assert_eq!(app.active_key_context(), KeyContext::SettingsZoneEditor);

        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_variant_picker = Some(dummy_variant_picker());
        assert_eq!(app.active_key_context(), KeyContext::SettingsVariantPicker);

        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_scaffold_offer = Some(dummy_scaffold_offer());
        assert_eq!(app.active_key_context(), KeyContext::SettingsScaffoldOffer);
    }

    #[test]
    fn active_key_context_settings_substate_higher_precedence_wins() {
        // quit-prompt outranks editing (keys.rs checks quit_prompt first).
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_quit_prompt.active = true;
        app.settings_editing = true;
        assert_eq!(app.active_key_context(), KeyContext::SettingsQuitPrompt);

        // editing outranks the zone editor.
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_editing = true;
        app.settings_zone_editor = Some(dummy_zone_editor());
        assert_eq!(app.active_key_context(), KeyContext::SettingsEditing);

        // zone editor outranks the variant picker.
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_zone_editor = Some(dummy_zone_editor());
        app.settings_variant_picker = Some(dummy_variant_picker());
        assert_eq!(app.active_key_context(), KeyContext::SettingsZoneEditor);

        // variant picker outranks the scaffold offer.
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Settings;
        app.settings_variant_picker = Some(dummy_variant_picker());
        app.settings_scaffold_offer = Some(dummy_scaffold_offer());
        assert_eq!(app.active_key_context(), KeyContext::SettingsVariantPicker);
    }

    #[cfg(feature = "agent")]
    #[test]
    fn active_key_context_agent_dialog_vs_text_input() {
        let mut app = make_test_app(1);
        app.agent_dialog.active = true;
        assert_eq!(app.active_key_context(), KeyContext::AgentDialog);

        app.agent_dialog.text_input = Some(String::new());
        assert_eq!(app.active_key_context(), KeyContext::AgentTextInput);
    }

    #[cfg(feature = "agent")]
    #[test]
    fn active_key_context_agents_view_mode() {
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Agents;
        assert_eq!(app.active_key_context(), KeyContext::Agents);
    }

    /// Render the help overlay for `app` into a fresh TestBackend and return the
    /// whole buffer flattened to a single string.
    fn render_help_to_string(app: &mut App, w: u16, h: u16) -> String {
        use crate::tui::views::overlays::draw_help_overlay;
        use ratatui::{backend::TestBackend, Terminal};

        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw_help_overlay(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .flat_map(|y| {
                (0..buffer.area.width)
                    .map(move |x| (x, y))
                    .chain(std::iter::once((u16::MAX, y)))
            })
            .map(|(x, y)| {
                if x == u16::MAX {
                    "\n".to_string()
                } else {
                    buffer.cell((x, y)).unwrap().symbol().to_string()
                }
            })
            .collect()
    }

    #[test]
    fn help_overlay_renders_only_active_context() {
        // Default mode is Types: its keybinds (e.g. the `x` wrap toggle and the
        // help toggle) must show; a Settings-only action (Save) must not.
        let mut app = make_test_app(1);
        app.show_help = true;
        assert_eq!(app.active_key_context(), KeyContext::Types);

        // A tall viewport so all Types groups fit unscrolled and the assertions
        // test mode-awareness, not scroll position.
        let content = render_help_to_string(&mut app, 60, 50);
        assert!(
            content.contains("Toggle wrap"),
            "Types help should show the wrap toggle, got:\n{content}"
        );
        assert!(
            content.contains("Toggle help"),
            "Types help should show the help toggle, got:\n{content}"
        );
        assert!(
            content.contains("Types"),
            "title should label the Types context, got:\n{content}"
        );
        assert!(
            !content.contains("Save"),
            "Types help must not show Settings-only Save, got:\n{content}"
        );
    }

    #[test]
    fn help_overlay_is_mode_aware_for_graph() {
        // Graph mode: its title label appears, and the Types-only `x` wrap toggle
        // does not.
        let mut app = make_test_app(1);
        app.view_mode = ViewMode::Graph;
        app.show_help = true;
        assert_eq!(app.active_key_context(), KeyContext::Graph);

        let content = render_help_to_string(&mut app, 60, 30);
        let graph_label = crate::tui::views::keybinds::context_label(KeyContext::Graph);
        assert!(
            content.contains(graph_label),
            "Graph help title should appear, got:\n{content}"
        );
        assert!(
            !content.contains("Toggle wrap"),
            "Graph help must not show the Types-only wrap toggle, got:\n{content}"
        );
    }

    // ---- T4: `?` opens help in the four non-text contexts ----

    #[test]
    fn help_opens_in_graph_context() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.view_mode = ViewMode::Graph;
        app.help_scroll = 7;

        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE, &root, &config);

        assert!(app.show_help);
        assert_eq!(app.help_scroll, 0);
    }

    #[test]
    fn help_opens_in_fullscreen_context() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.fullscreen_doc = true;
        app.help_scroll = 7;

        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE, &root, &config);

        assert!(app.show_help);
        assert_eq!(app.help_scroll, 0);
    }

    #[test]
    fn help_opens_in_settings_nav_context() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.view_mode = ViewMode::Settings;
        // Plain nav: no sub-state active.
        app.help_scroll = 7;

        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE, &root, &config);

        assert!(app.show_help);
        assert_eq!(app.help_scroll, 0);
    }

    #[cfg(feature = "agent")]
    #[test]
    fn help_opens_in_agents_context() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.view_mode = ViewMode::Agents;
        app.help_scroll = 7;

        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE, &root, &config);

        assert!(app.show_help);
        assert_eq!(app.help_scroll, 0);
    }

    #[test]
    fn question_mark_does_not_open_help_in_settings_edit() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.view_mode = ViewMode::Settings;
        app.settings_editing = true;
        app.settings_edit_input.clear();

        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE, &root, &config);

        assert!(!app.show_help, "edit sub-state must not open help");
        assert!(
            app.settings_edit_input.contains('?'),
            "`?` should be a literal char in the edit buffer, got: {:?}",
            app.settings_edit_input
        );
    }

    // ---- T4: overflow-aware dismiss/scroll while help is open ----

    #[test]
    fn help_any_key_dismisses_when_content_fits() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.show_help = true;
        app.help_max_scroll = 0;

        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, &root, &config);

        assert!(!app.show_help, "j must dismiss when content fits");
    }

    #[test]
    fn help_scrolls_and_clamps_when_content_overflows() {
        let mut app = make_test_app(1);
        let root = PathBuf::from(".");
        let config = Config::default();
        app.show_help = true;
        app.help_max_scroll = 5;
        app.help_scroll = 0;

        // `j` scrolls down without dismissing.
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, &root, &config);
        assert!(app.show_help);
        assert_eq!(app.help_scroll, 1);

        // Four more `j` reach the max.
        for _ in 0..4 {
            app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, &root, &config);
        }
        assert_eq!(app.help_scroll, 5);

        // A sixth `j` clamps at max.
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.help_scroll, 5);

        // `k` scrolls back up.
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.help_scroll, 4);

        // Any other key dismisses.
        app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE, &root, &config);
        assert!(!app.show_help);
    }

    #[test]
    fn help_render_writes_overflow_for_small_viewport_only() {
        // A small viewport: the Types help content is taller than the inner
        // area, so the render must publish a positive max scroll.
        let mut app = make_test_app(1);
        app.show_help = true;
        assert_eq!(app.active_key_context(), KeyContext::Types);

        let _ = render_help_to_string(&mut app, 40, 8);
        assert!(
            app.help_max_scroll > 0,
            "small viewport should overflow, got {}",
            app.help_max_scroll
        );

        // A tall viewport: everything fits, so max scroll is 0.
        let _ = render_help_to_string(&mut app, 40, 60);
        assert_eq!(app.help_max_scroll, 0);
    }

    // --- ITERATION-208 graph pivot sidebar ---------------------------------

    /// Render the graph view into a fresh TestBackend and flatten the buffer to
    /// a single string.
    fn render_graph_to_string(app: &mut App, w: u16, h: u16, config: &Config) -> String {
        use crate::tui::views::panels::draw_graph;
        use crate::tui::views::StatusPalette;
        use ratatui::{backend::TestBackend, Terminal};

        let colors = StatusPalette::default();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| draw_graph(f, app, f.area(), config, &colors))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .flat_map(|y| {
                (0..buffer.area.width)
                    .map(move |x| (x, y))
                    .chain(std::iter::once((u16::MAX, y)))
            })
            .map(|(x, y)| {
                if x == u16::MAX {
                    "\n".to_string()
                } else {
                    buffer.cell((x, y)).unwrap().symbol().to_string()
                }
            })
            .collect()
    }

    /// An app loaded from `files` with `doc_types`/plurals populated from the
    /// default config, in Graph mode with the forest built.
    fn graph_app(files: &[(&str, &str)]) -> (tempfile::TempDir, App) {
        let (tmp, mut app) = app_with_store(files);
        app.apply_config(&Config::default());
        app.view_mode = ViewMode::Graph;
        app.rebuild_graph();
        (tmp, app)
    }

    fn type_index(app: &App, name: &str) -> usize {
        app.doc_types
            .iter()
            .position(|t| t.as_str() == name)
            .unwrap_or_else(|| panic!("type {name} not in doc_types"))
    }

    fn forest_files() -> Vec<(&'static str, String)> {
        vec![
            (
                "docs/rfcs/RFC-001-base.md",
                relations_doc_md("Base", "rfc", "[]"),
            ),
            (
                "docs/stories/STORY-001-mid.md",
                relations_doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                relations_doc_md("Leaf", "iteration", "- implements: STORY-001"),
            ),
        ]
    }

    /// AC1: graph view left column renders the type list (the pivot picker), not
    /// the empty " Graph " block.
    #[test]
    fn graph_left_column_renders_pivot_type_list() {
        let files = forest_files();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (_tmp, mut app) = graph_app(&refs);

        let rendered = render_graph_to_string(&mut app, 100, 30, &Config::default());

        assert!(
            rendered.contains("Pivot"),
            "left column should be the pivot picker, got:\n{rendered}"
        );
        // The whole-store ("All") row and the type plurals are listed.
        assert!(rendered.contains("All"), "pivot lists the All row");
        let rfc_plural = app.type_plurals.get("rfc").unwrap();
        assert!(
            rendered.contains(rfc_plural.as_str()),
            "pivot lists the rfc plural '{rfc_plural}', got:\n{rendered}"
        );
    }

    /// The graph table carries a slim ID column on the left: the header shows
    /// `ID` and a doc's id renders in that column.
    #[test]
    fn graph_table_renders_slim_id_column() {
        let files = forest_files();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (_tmp, mut app) = graph_app(&refs);

        let rendered = render_graph_to_string(&mut app, 120, 30, &Config::default());

        assert!(
            rendered.contains("ID"),
            "graph header carries an ID column, got:\n{rendered}"
        );
        assert!(
            rendered.contains("RFC-001"),
            "a doc id renders in the ID column, got:\n{rendered}"
        );
    }

    /// Tags surface as graph pivots: `available_tags` is populated on graph entry
    /// (not only in Filters), the pivot panel lists the tag, and a `Tag` anchor
    /// re-roots the forest onto the tagged doc.
    #[test]
    fn graph_tag_pivot_lists_and_reroots() {
        let tagged_rfc = "---\ntitle: \"Tagged RFC\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags:\n- alpha\nrelated: []\n---\n\nbody\n";
        let refs: Vec<(&str, &str)> = vec![
            ("docs/rfcs/RFC-001-tagged.md", tagged_rfc),
            (
                "docs/rfcs/RFC-002-plain.md",
                "---\ntitle: \"Plain\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n",
            ),
        ];
        let (_tmp, mut app) = graph_app(&refs);
        let config = Config::default();

        // available_tags is current without ever visiting Filters.
        assert_eq!(app.available_tags, vec!["alpha".to_string()]);

        let rendered = render_graph_to_string(&mut app, 120, 30, &config);
        assert!(
            rendered.contains("[alpha]"),
            "pivot lists the tag row, got:\n{rendered}"
        );

        // Walk the sidebar to the tag row (All, then each type, then tags).
        let tag_row = anchor_to_flat(GraphAnchor::Tag(0), app.doc_types.len());
        for _ in 0..tag_row {
            app.move_graph_anchor_next();
        }
        assert_eq!(app.graph_anchor, GraphAnchor::Tag(0));
        let ids: Vec<String> = app
            .graph_nodes
            .iter()
            .filter_map(|n| app.store.get(&n.path).map(|d| d.id.clone()))
            .collect();
        assert_eq!(
            ids,
            vec!["RFC-001".to_string()],
            "tag anchor re-roots onto only the tagged doc, got {ids:?}"
        );
    }

    // --- ITERATION-209 nested table + sibling sort -------------------------

    /// A config whose `story` type declares an `estimate` int attribute and whose
    /// `tui.graph` renders DOC + status + estimate columns, sorting by estimate.
    fn graph_attr_config() -> Config {
        Config::parse(
            r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[types.attributes]]
name = "estimate"
kind = "int"

[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[relationships]]
name = "related-to"
traversal = "related"

[tui.graph]
columns = ["status", "estimate"]
sort = "estimate"
"#,
        )
        .unwrap()
    }

    /// `relations_doc_md` with an extra frontmatter line (e.g. an `estimate`).
    fn doc_md_with_line(title: &str, doc_type: &str, related: &str, extra: &str) -> String {
        let related_block = if related == "[]" {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\n{extra}\n{related_block}\n---\n\n{title} body\n"
        )
    }

    /// Build a graph `App` with the store loaded under `config` (so attributes
    /// coerce to typed values) and the graph rebuilt.
    fn graph_app_with_config(files: &[(&str, &str)], config: &Config) -> (tempfile::TempDir, App) {
        let tmp = tempfile::TempDir::new().unwrap();
        for (rel_path, contents) in files {
            let full = tmp.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        let store = Store::load(tmp.path(), config).unwrap();
        let mut app = make_test_app(0);
        app.store = store;
        app.apply_config(config);
        app.graph_sort_col = config.ui.graph.sort.clone();
        app.view_mode = ViewMode::Graph;
        app.rebuild_graph();
        (tmp, app)
    }

    // AC1: the table renders a DOC header column plus each configured column,
    // and the DOC column still carries the tree connectors.
    #[test]
    fn graph_table_renders_doc_and_configured_columns_with_tree_art() {
        let files = forest_files();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (_tmp, mut app) = graph_app(&refs); // default columns: status, related

        let rendered = render_graph_to_string(&mut app, 120, 30, &Config::default());

        assert!(rendered.contains("DOC"), "DOC header column present");
        assert!(rendered.contains("STATUS"), "status column header present");
        assert!(
            rendered.contains("RELATED"),
            "related column header present"
        );
        assert!(
            rendered.contains("─▶"),
            "tree connectors survive in the DOC column, got:\n{rendered}"
        );
    }

    // AC2: a column naming an attribute undeclared/absent on a row's type renders
    // an empty cell for those rows and never panics. The RFC row has no estimate;
    // the story rows do.
    #[test]
    fn graph_table_attribute_column_blank_for_rows_without_it() {
        let config = graph_attr_config();
        let files = [
            (
                "docs/rfcs/RFC-001-base.md",
                relations_doc_md("Base", "rfc", "[]"),
            ),
            (
                "docs/stories/STORY-001-a.md",
                doc_md_with_line("A", "story", "- implements: RFC-001", "estimate: 8"),
            ),
            (
                "docs/stories/STORY-002-b.md",
                doc_md_with_line("B", "story", "- implements: RFC-001", "estimate: 3"),
            ),
        ];
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (_tmp, mut app) = graph_app_with_config(&refs, &config);

        let rendered = render_graph_to_string(&mut app, 120, 30, &config);

        assert!(
            rendered.contains("ESTIMATE"),
            "estimate column header present"
        );
        // The story estimates render; the RFC (no estimate attr) has no value
        // — its row carries a blank cell, which simply means neither 8 nor 3
        // appears beside it. We assert the present values render and nothing
        // panicked (the render completed).
        assert!(rendered.contains('8'), "story A estimate renders");
        assert!(rendered.contains('3'), "story B estimate renders");
    }

    // AC3: `o` cycles the sort column, `O` reverses; the header shows the active
    // column with a direction arrow, and siblings reorder by the active column.
    #[test]
    fn graph_o_cycles_sort_and_capital_o_reverses() {
        let config = graph_attr_config();
        let files = [
            (
                "docs/rfcs/RFC-001-base.md",
                relations_doc_md("Base", "rfc", "[]"),
            ),
            (
                "docs/stories/STORY-001-a.md",
                doc_md_with_line("A", "story", "- implements: RFC-001", "estimate: 8"),
            ),
            (
                "docs/stories/STORY-002-b.md",
                doc_md_with_line("B", "story", "- implements: RFC-001", "estimate: 3"),
            ),
        ];
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (tmp, mut app) = graph_app_with_config(&refs, &config);
        let root = tmp.path().to_path_buf();

        // Seeded sort col is `estimate` (from config). Ascending: 3 (B) before 8 (A).
        let ids = |a: &App| -> Vec<String> {
            a.graph_nodes
                .iter()
                .map(|n| a.store.get(&n.path).unwrap().id.clone())
                .collect()
        };
        assert_eq!(app.graph_sort_col, "estimate");
        assert_eq!(
            ids(&app),
            vec!["RFC-001", "STORY-002", "STORY-001"],
            "ascending estimate: 3 before 8"
        );

        // `O` reverses: 8 (A) before 3 (B).
        app.handle_key(KeyCode::Char('O'), KeyModifiers::NONE, &root, &config);
        assert!(app.graph_sort_rev, "O toggled reverse");
        assert_eq!(
            ids(&app),
            vec!["RFC-001", "STORY-001", "STORY-002"],
            "descending estimate: 8 before 3"
        );
        let rendered = render_graph_to_string(&mut app, 120, 30, &config);
        assert!(
            rendered.contains("ESTIMATE ▼"),
            "header shows active col + descending arrow, got:\n{rendered}"
        );

        // `o` cycles the column. Cycle is path, status, estimate; from estimate it
        // wraps to path.
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_sort_col, "path", "o wraps estimate -> path");
        let rendered = render_graph_to_string(&mut app, 120, 30, &config);
        assert!(
            rendered.contains("DOC ▼"),
            "DOC (path) header carries the active arrow, got:\n{rendered}"
        );

        // `o` again -> status, then estimate.
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_sort_col, "status");
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_sort_col, "estimate");
    }

    /// AC2: `h`/`l` move `graph_anchor` over the sidebar (All -> types… -> tags…)
    /// with no wraparound. The forest fixture carries no tags, so the last row is
    /// the last type.
    #[test]
    fn graph_hl_moves_anchor_over_types() {
        let files = forest_files();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (tmp, mut app) = graph_app(&refs);
        let root = tmp.path().to_path_buf();
        let config = Config::default();

        assert_eq!(
            app.graph_anchor,
            GraphAnchor::All,
            "default anchor is whole-store"
        );

        // l: All -> Type(0)
        app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_anchor, GraphAnchor::Type(0));

        // l: Type(0) -> Type(1)
        app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_anchor, GraphAnchor::Type(1));

        // h: Type(1) -> Type(0)
        app.handle_key(KeyCode::Char('h'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_anchor, GraphAnchor::Type(0));

        // h: Type(0) -> All
        app.handle_key(KeyCode::Char('h'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_anchor, GraphAnchor::All);

        // h at All: stays All (no wraparound).
        app.handle_key(KeyCode::Char('h'), KeyModifiers::NONE, &root, &config);
        assert_eq!(app.graph_anchor, GraphAnchor::All);

        // l clamps at the last type (no tags in this fixture).
        assert!(app.available_tags.is_empty(), "fixture carries no tags");
        let last = app.doc_types.len() - 1;
        for _ in 0..app.doc_types.len() + 3 {
            app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE, &root, &config);
        }
        assert_eq!(
            app.graph_anchor,
            GraphAnchor::Type(last),
            "l clamps at the last type"
        );
    }

    /// AC3: when an anchor is set, `rebuild_graph` re-roots the forest on that
    /// type -- every emitted root node is of the anchor type.
    #[test]
    fn graph_anchor_reroots_forest_on_type() {
        let files = forest_files();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (_tmp, mut app) = graph_app(&refs);

        app.graph_anchor = GraphAnchor::Type(type_index(&app, "story"));
        app.rebuild_graph();

        // The parent RFC is no longer pruned above the story anchor (STORY-247):
        // it hangs under it as a reverse-chain row, never as a root.
        let rfc_rows: Vec<&GraphNode> = app
            .graph_nodes
            .iter()
            .filter(|n| {
                app.store
                    .get(&n.path)
                    .is_some_and(|d| d.id.as_str() == "RFC-001")
            })
            .collect();
        assert!(!rfc_rows.is_empty(), "the ancestor RFC is emitted");
        assert!(
            rfc_rows.iter().all(|n| n.reverse && n.depth > 0),
            "the ancestor RFC is a marked reverse row below the anchor"
        );
        // Every depth-0 (root) node is of the anchor type 'story'.
        for node in app.graph_nodes.iter().filter(|n| n.depth == 0) {
            assert_eq!(
                node.doc_type.as_str(),
                "story",
                "every root of the anchored forest must be the anchor type"
            );
        }
        assert!(
            app.graph_nodes
                .iter()
                .any(|n| n.doc_type.as_str() == "story"),
            "anchored forest still contains the story root"
        );
    }

    /// AC4: no anchor -> whole-store forest, identical to the engine's
    /// `resolve_forest(store, None)` output (the prior behaviour).
    #[test]
    fn graph_no_anchor_is_whole_store_forest() {
        let files = forest_files();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, c)| (*p, c.as_str())).collect();
        let (_tmp, mut app) = graph_app(&refs);

        app.graph_anchor = GraphAnchor::All;
        app.rebuild_graph();

        let ids: std::collections::BTreeSet<String> = app
            .graph_nodes
            .iter()
            .filter_map(|n| app.store.get(&n.path).map(|d| d.id.clone()))
            .collect();
        assert_eq!(
            ids,
            std::collections::BTreeSet::from([
                "RFC-001".to_string(),
                "STORY-001".to_string(),
                "ITERATION-001".to_string(),
            ]),
            "whole-store forest includes every doc, ancestor RFC included"
        );
    }

    // ITERATION-306: App::plan_open is pure -- a URL target hands off to the
    // browser, a file target needs a viewer (split on whitespace), and a
    // file-only target with no viewer errors with a viewer-naming message.
    #[test]
    fn plan_open_url_target_opens_browser() {
        let req = App::plan_open(
            OpenTarget::Url("https://example.com/x".to_string()),
            Some("glow"),
            Path::new("/repo"),
        )
        .unwrap();
        assert_eq!(
            req,
            OpenRequest::Browser("https://example.com/x".to_string())
        );
    }

    #[test]
    fn plan_open_file_target_with_viewer_joins_root() {
        let rel = PathBuf::from("docs/rfcs/RFC-001-a.md");
        let req = App::plan_open(
            OpenTarget::File(rel.clone()),
            Some("glow"),
            Path::new("/repo"),
        )
        .unwrap();
        assert_eq!(
            req,
            OpenRequest::Viewer {
                command: vec!["glow".to_string()],
                path: PathBuf::from("/repo").join(&rel),
            }
        );
    }

    #[test]
    fn plan_open_splits_viewer_args_on_whitespace() {
        let req = App::plan_open(
            OpenTarget::File(PathBuf::from("docs/rfcs/RFC-001-a.md")),
            Some("code -w"),
            Path::new("/repo"),
        )
        .unwrap();
        match req {
            OpenRequest::Viewer { command, .. } => {
                assert_eq!(command, vec!["code".to_string(), "-w".to_string()]);
            }
            other => panic!("expected Viewer, got {other:?}"),
        }
    }

    #[test]
    fn resolve_editor_command_splits_args_on_whitespace() {
        assert_eq!(
            resolve_editor_command_from(Some("code --wait"), None),
            vec!["code".to_string(), "--wait".to_string()]
        );
    }

    #[test]
    fn resolve_editor_command_falls_back_to_vi_when_unset() {
        assert_eq!(
            resolve_editor_command_from(None, None),
            vec!["vi".to_string()]
        );
    }

    #[test]
    fn resolve_editor_command_falls_back_to_vi_when_whitespace_only() {
        assert_eq!(
            resolve_editor_command_from(Some("  "), Some(" ")),
            vec!["vi".to_string()]
        );
    }

    #[test]
    fn plan_open_file_target_without_viewer_errors() {
        let err = App::plan_open(
            OpenTarget::File(PathBuf::from("docs/rfcs/RFC-001-a.md")),
            None,
            Path::new("/repo"),
        )
        .unwrap_err();
        assert!(
            err.contains("viewer"),
            "error should name the viewer: {err}"
        );
    }

    // Select the first rfc doc so `selected_doc_meta()` is Some, mirroring how the
    // parity seed drills into a type's tree.
    fn select_rfc_doc(app: &mut App) {
        let rfc_idx = app
            .doc_types
            .iter()
            .position(|t| t.as_str() == "rfc")
            .expect("default config has an rfc type");
        app.selected_type = rfc_idx;
        app.build_doc_tree();
        app.selected_doc = 0;
        assert!(
            app.selected_doc_meta().is_some(),
            "an rfc doc must be selected"
        );
    }

    // ITERATION-306: pressing `o` in the Types view with a viewer configured
    // stages a Viewer open request (the tempdir is not a git repo, so the target
    // is a File).
    #[test]
    fn key_o_routes_to_open_request_with_viewer() {
        let (tmp, mut app) = bare_app();
        populate_docs(&mut app);
        select_rfc_doc(&mut app);

        let mut config = Config::default();
        config.ui.viewer = Some("glow".to_string());
        let root = tmp.path().to_path_buf();

        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE, &root, &config);

        assert!(
            matches!(app.open_request, Some(OpenRequest::Viewer { .. })),
            "pressing `o` must stage a Viewer open request"
        );
        assert!(app.open_message.is_none());
    }

    // ITERATION-306: with no viewer configured, a file-only target has nowhere to
    // open -- pressing `o` sets the transient notice instead of an open request.
    #[test]
    fn key_o_without_viewer_sets_message() {
        let (tmp, mut app) = bare_app();
        populate_docs(&mut app);
        select_rfc_doc(&mut app);

        let config = Config::default();
        let root = tmp.path().to_path_buf();

        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE, &root, &config);

        assert!(app.open_request.is_none());
        assert!(app.open_message.is_some());
    }
}
