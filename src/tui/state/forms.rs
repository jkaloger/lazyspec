use crate::engine::document::DocType;
use crate::tui::state::settings_guard::TypeFieldImpact;
use std::path::PathBuf;

/// A pending external-open handover (STORY-219): set on the `App` when the user
/// presses `o`, drained by the event loop. `Browser` hands a web URL to the OS
/// opener; `Viewer` suspends the TUI and runs the configured terminal viewer on
/// the doc's file, mirroring the `$EDITOR` flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenRequest {
    Browser(String),
    Viewer {
        command: Vec<String>,
        path: std::path::PathBuf,
    },
}

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
    pub state: crate::spinners::SpinnerState,
    /// When set, the event loop auto-dismisses the overlay once this deadline
    /// passes. Used to hold the success face on screen briefly before teardown
    /// so a create that finishes instantly still renders its success frame.
    pub dismiss_at: Option<std::time::Instant>,
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
            state: crate::spinners::SpinnerState::Idle,
            dismiss_at: None,
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
        self.state = crate::spinners::SpinnerState::Idle;
        self.dismiss_at = None;
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

/// Which buffer entry a `SettingsDeleteConfirm` targets. Vec-backed collections
/// (Document Types / Relationships / Edges) carry the entry index; certification
/// overrides carry the sorted-key (spec-path) they live under.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsDeleteTarget {
    Index(usize),
    Key(String),
}

/// The confirm prompt shown before a settings collection entry is removed from
/// the in-memory buffer. Mirrors `DeleteConfirm`, but targets a buffer entry
/// (not a doc file): no disk is touched, removal is buffer-only and happens only
/// on confirm.
pub struct SettingsDeleteConfirm {
    pub active: bool,
    pub category: usize,
    pub entry_label: String,
    pub target: SettingsDeleteTarget,
}

impl Default for SettingsDeleteConfirm {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDeleteConfirm {
    pub fn new() -> Self {
        SettingsDeleteConfirm {
            active: false,
            category: 0,
            entry_label: String::new(),
            target: SettingsDeleteTarget::Index(0),
        }
    }
}

/// The spec-path key prompt shown when seeding a new certification override
/// (`n` on the Certification category). The key is entered before the override
/// is inserted into the buffer; an empty key inserts nothing.
pub struct OverrideKeyPrompt {
    pub active: bool,
    pub input: String,
}

impl Default for OverrideKeyPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl OverrideKeyPrompt {
    pub fn new() -> Self {
        OverrideKeyPrompt {
            active: false,
            input: String::new(),
        }
    }
}

/// The confirm prompt shown before a save that alters a load-bearing `[[types]]`
/// field (`dir`/`prefix`/`store`) of a type that already has documents on disk.
/// Changing such a field only rewrites config -- the settings screen never moves
/// files -- so the save pauses to report impact and require explicit confirmation
/// (RFC-023 slice 6). Holds ONLY the computed impacts for rendering; the buffer is
/// not copied here.
pub struct SettingsImpactConfirm {
    pub active: bool,
    pub impacts: Vec<TypeFieldImpact>,
}

impl Default for SettingsImpactConfirm {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsImpactConfirm {
    pub fn new() -> Self {
        SettingsImpactConfirm {
            active: false,
            impacts: Vec::new(),
        }
    }
}

/// The save/discard/cancel prompt shown when quitting Settings with unsaved
/// buffer edits (AC10). Mirrors `DeleteConfirm`: a one-flag overlay state.
pub struct SettingsQuitPrompt {
    pub active: bool,
}

impl Default for SettingsQuitPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsQuitPrompt {
    pub fn new() -> Self {
        SettingsQuitPrompt { active: false }
    }
}

pub struct StatusPicker {
    pub active: bool,
    pub selected: usize,
    pub doc_path: PathBuf,
    /// The selected document's type lifecycle states, in declared order. The
    /// picker lists and writes back from this rather than a hardcoded set.
    pub states: Vec<String>,
    pub error: Option<String>,
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
            states: Vec::new(),
            error: None,
        }
    }
}

/// The variant-picker overlay shown when an enum settings field is opened with
/// `Enter` (numbering, store, or reserved format). It lists the field's
/// variants; the chosen one is written back to the buffer at `path` (RFC-023 /
/// STORY-144). Cursor ops are pure (no terminal).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsVariantPicker {
    pub path: FieldPath,
    pub variants: &'static [&'static str],
    pub selected: usize,
}

impl SettingsVariantPicker {
    /// Seed the picker for `path` with `variants`, pre-selecting `current_index`
    /// (clamped to the variant range).
    pub fn new(path: FieldPath, variants: &'static [&'static str], current_index: usize) -> Self {
        let selected = if variants.is_empty() {
            0
        } else {
            current_index.min(variants.len() - 1)
        };
        SettingsVariantPicker {
            path,
            variants,
            selected,
        }
    }

    pub fn cursor_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn cursor_down(&mut self) {
        if !self.variants.is_empty() {
            self.selected = (self.selected + 1).min(self.variants.len() - 1);
        }
    }
}

pub struct LinkEditor {
    pub active: bool,
    pub doc_path: PathBuf,
    pub rel_type_index: usize,
    pub query: String,
    pub results: Vec<PathBuf>,
    pub selected: usize,
    pub error: Option<String>,
    /// Set when the viewed doc lives in a store that can never be the source of
    /// a relation (a `github-milestones` doc). The editor offers no relation
    /// types and the overlay shows an empty-state message instead of candidates.
    pub source_blocked: bool,
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
            error: None,
            source_blocked: false,
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

/// One entry in the agent dialog: either a resolved user-authored template (the
/// full `AgentPrompt` is retained so the dialog can render and dispatch it
/// directly) or the freeform `Custom` prompt. The built-in Expand/Create-children
/// actions are gone (RFC-046 slice 4).
#[cfg(feature = "agent")]
#[derive(Debug, Clone)]
pub enum AgentAction {
    Template(crate::engine::prompt::AgentPrompt),
    Custom,
}

#[cfg(feature = "agent")]
pub struct AgentDialog {
    pub active: bool,
    pub selected_index: usize,
    pub actions: Vec<AgentAction>,
    /// Stems named in the type's `agents` list with no matching loaded template.
    /// Captured here for the next unit's missing-template footer.
    pub missing: Vec<String>,
    pub doc_path: PathBuf,
    pub doc_title: String,
    pub text_input: Option<String>,
}

/// A pending interactive-agent handover (RFC-046 slice 5 / STORY-136). Set on the
/// `App` when a `mode: interactive` template is selected; drained by the event loop,
/// which suspends the TUI, runs `build_interactive_command`, and restores. The
/// engine builds the Command; the TUI owns the terminal state (convention 3). No
/// AgentRecord is ever written for an interactive run (AC7).
#[cfg(feature = "agent")]
#[derive(Debug, Clone)]
pub struct InteractiveRequest {
    pub cmd: String,
    pub prompt: String,
    pub doc_path: PathBuf,
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
            missing: Vec::new(),
            doc_path: PathBuf::new(),
            doc_title: String::new(),
            text_input: None,
        }
    }
}

/// How a settings field is edited. A later increment dispatches the actual edit
/// behaviour on this; this increment only renders from the model, so `ReadOnly`
/// marks fields that are display-only for now (unset optional sections, statusbar
/// component slots whose ordering is a later slice).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldEditor {
    Text,
    Toggle,
    BoundedNum {
        min: u64,
        max: u64,
    },
    Nullable,
    List,
    EnumCycle {
        variants: &'static [&'static str],
    },
    /// A two-pane (Selected/Available) reorderable editor for a status-bar zone
    /// (`statusbar.left/center/right`). The current value + per-zone defaults are
    /// derived from the `FieldPath` when the editor opens, so the variant is unit.
    ZoneOrdering,
    ReadOnly,
}

/// Which pane the status-bar zone editor cursor is on: the ordered `Selected`
/// list (the zone's chosen components) or the `Available` vocabulary list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZonePane {
    Selected,
    Available,
}

/// The two-pane reorderable editor state for one status-bar zone. `selected` is
/// the zone's components in render order; `available` is the remaining
/// `STATUS_BAR_COMPONENTS` vocabulary (const order preserved). `path` records
/// which zone the commit writes back to. Operations are pure (no terminal), so
/// they are App-testable.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneOrderingEditor {
    pub path: FieldPath,
    pub selected: Vec<String>,
    pub available: Vec<String>,
    pub cursor: usize,
    pub pane: ZonePane,
}

impl ZoneOrderingEditor {
    /// Seed the editor for `path` from the zone's current buffer value: `selected`
    /// is the current names (or `defaults` when the zone is `None`); `available`
    /// is `STATUS_BAR_COMPONENTS` minus the selected names, in const order.
    pub fn new(path: FieldPath, current: Option<&Vec<String>>, defaults: &[&str]) -> Self {
        let selected: Vec<String> = match current {
            Some(names) => names.clone(),
            None => defaults.iter().map(|s| s.to_string()).collect(),
        };
        let available = Self::available_for(&selected);
        ZoneOrderingEditor {
            path,
            selected,
            available,
            cursor: 0,
            pane: ZonePane::Selected,
        }
    }

    /// The vocabulary minus `selected`, in `STATUS_BAR_COMPONENTS` order. Only
    /// const names can ever appear here (AC5).
    fn available_for(selected: &[String]) -> Vec<String> {
        crate::tui::views::status_bar::STATUS_BAR_COMPONENTS
            .iter()
            .filter(|name| !selected.iter().any(|s| s == *name))
            .map(|s| s.to_string())
            .collect()
    }

    fn active_len(&self) -> usize {
        match self.pane {
            ZonePane::Selected => self.selected.len(),
            ZonePane::Available => self.available.len(),
        }
    }

    fn clamp_cursor(&mut self) {
        let len = self.active_len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    pub fn toggle_pane(&mut self) {
        self.pane = match self.pane {
            ZonePane::Selected => ZonePane::Available,
            ZonePane::Available => ZonePane::Selected,
        };
        self.clamp_cursor();
    }

    pub fn cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn cursor_down(&mut self) {
        let len = self.active_len();
        if len > 0 {
            self.cursor = (self.cursor + 1).min(len - 1);
        }
    }

    /// Move the focused Available name to the end of `selected`. No-op unless the
    /// Available pane is focused and non-empty. Only ever surfaces a const name.
    pub fn add(&mut self) {
        if self.pane != ZonePane::Available {
            return;
        }
        if self.cursor >= self.available.len() {
            return;
        }
        let name = self.available.remove(self.cursor);
        self.selected.push(name);
        self.clamp_cursor();
    }

    /// Move the focused Selected name back into `available`, restoring const order
    /// among the available names. No-op unless the Selected pane is focused and
    /// non-empty.
    pub fn remove(&mut self) {
        if self.pane != ZonePane::Selected {
            return;
        }
        if self.cursor >= self.selected.len() {
            return;
        }
        self.selected.remove(self.cursor);
        self.available = Self::available_for(&self.selected);
        self.clamp_cursor();
    }

    /// Swap the focused Selected name with the one above it. No-op off the Selected
    /// pane or at the top.
    pub fn move_up(&mut self) {
        if self.pane != ZonePane::Selected || self.cursor == 0 {
            return;
        }
        self.selected.swap(self.cursor, self.cursor - 1);
        self.cursor -= 1;
    }

    /// Swap the focused Selected name with the one below it. No-op off the Selected
    /// pane or at the bottom.
    pub fn move_down(&mut self) {
        if self.pane != ZonePane::Selected {
            return;
        }
        if self.cursor + 1 >= self.selected.len() {
            return;
        }
        self.selected.swap(self.cursor, self.cursor + 1);
        self.cursor += 1;
    }
}

/// A type-key inside a drilled [[types]] entry. The buffer target a later
/// increment writes to is `(index, key)`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeKey {
    Name,
    Plural,
    Dir,
    Prefix,
    Icon,
    Numbering,
    Subdirectory,
    Store,
    Singleton,
    ParentType,
    Agents,
}

/// A relationship-key inside a drilled [[relationships]] entry.
#[derive(Debug, Clone, PartialEq)]
pub enum RelKey {
    Name,
    Inverse,
}

/// An edge-key inside a drilled [[edges]] entry -- one key per `EdgeDef` field,
/// so the drilled view is the row (RFC-067). Every key is editable: `name` and
/// the three selector positions through text entry, the two optional qualifiers
/// through the enum cycler's unset-leading variant list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeKey {
    Name,
    From,
    To,
    Via,
    Required,
    Traversal,
}

/// Uniquely identifies the buffer target for one editable settings field, so a
/// later increment can read/write `App.settings_buffer` (a `Config`) for it via
/// an exhaustive `match`. Collection variants carry the entry index; the
/// certification override variant carries the override map key. This increment
/// only attaches paths to the model -- nothing dispatches on them yet.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldPath {
    Naming,
    RefCountCeiling,
    TemplatesDir,
    Type {
        index: usize,
        key: TypeKey,
    },
    Rel {
        index: usize,
        key: RelKey,
    },
    Edge {
        index: usize,
        key: EdgeKey,
    },
    SqidsSalt,
    SqidsMinLength,
    ReservedRemote,
    ReservedFormat,
    ReservedMaxRetries,
    GithubRepo,
    GithubCacheTtl,
    CertNormalize,
    CertOverride {
        key: String,
    },
    AgentsInteractive,
    UiAsciiDiagrams,
    StatusbarEnabled,
    StatusbarLeft,
    StatusbarCenter,
    StatusbarRight,
    MultilineMaxExpandedHeight,
    /// An unset optional-section placeholder line (rendered ReadOnly). It targets
    /// no concrete config field; a later increment surfaces "create section"
    /// rather than an in-place edit for these.
    Unset,
}

/// One row in a settings FIELD-view: the rendered key label, the current display
/// value, the editor kind, and the buffer path. The render derives its lines from
/// `format!("{}: {}", label, value)` so the view and the (future) editors share
/// one source of truth.
#[derive(Debug, Clone, PartialEq)]
pub struct EditableField {
    pub label: String,
    pub value: String,
    pub editor: FieldEditor,
    pub path: FieldPath,
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC-023 STORY-144 enum variant picker (Task 5). The variant set used here
    // mirrors the numbering variants the settings render carries.
    const NUMBERING: &[&str] = &["incremental", "sqids", "reserved"];

    // AC4: the picker pre-selects the current variant's index.
    #[test]
    fn ac4_variant_picker_new_selects_current_index() {
        let picker = SettingsVariantPicker::new(FieldPath::Unset, NUMBERING, 1);
        assert_eq!(picker.selected, 1);
    }

    // AC4: cursor_down stops at the last variant and does not run past it.
    #[test]
    fn ac4_variant_picker_cursor_down_clamps_at_last() {
        let mut picker = SettingsVariantPicker::new(FieldPath::Unset, NUMBERING, 2);
        picker.cursor_down();
        assert_eq!(
            picker.selected,
            NUMBERING.len() - 1,
            "cursor_down clamps at the final variant"
        );
    }

    // AC4: cursor_up saturates at the first variant.
    #[test]
    fn ac4_variant_picker_cursor_up_saturates_at_zero() {
        let mut picker = SettingsVariantPicker::new(FieldPath::Unset, NUMBERING, 0);
        picker.cursor_up();
        assert_eq!(picker.selected, 0, "cursor_up saturates at 0");
    }

    // STORY-230 AC3: a successful create holds the success face -- state is
    // Success, animation stops, and a dismiss deadline is armed rather than the
    // overlay being torn down immediately.
    #[test]
    fn ac3_success_arms_dismiss_deadline_without_reset() {
        let mut form = CreateForm::new();
        form.active = true;
        form.loading = true;
        form.state = crate::spinners::SpinnerState::Loading;

        // Mirror the event loop's CreateComplete Ok branch.
        form.loading = false;
        form.state = crate::spinners::SpinnerState::Success;
        form.dismiss_at = Some(std::time::Instant::now() + std::time::Duration::from_millis(600));

        assert_eq!(form.state, crate::spinners::SpinnerState::Success);
        assert!(!form.loading, "success is a held frame, not animating");
        assert!(
            form.dismiss_at.is_some(),
            "overlay not dismissed immediately"
        );
        assert!(form.active, "overlay stays active during the hold");
    }

    // reset() (via close_create_form) must clear the dismiss deadline so a stale
    // Instant can never re-trigger a dismiss on the next form.
    #[test]
    fn reset_clears_dismiss_at() {
        let mut form = CreateForm::new();
        form.dismiss_at = Some(std::time::Instant::now());
        form.reset();
        assert!(form.dismiss_at.is_none());
    }
}
