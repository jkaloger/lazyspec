use crate::engine::document::DocType;
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

/// Which buffer entry a `SettingsDeleteConfirm` targets. Vec-backed collections
/// (Document Types / Relationships / Validation Rules) carry the entry index;
/// certification overrides carry the sorted-key (spec-path) they live under.
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
    BoundedNum { min: u64, max: u64 },
    Nullable,
    Duration,
    List,
    EnumCycle { variants: &'static [&'static str] },
    ReadOnly,
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

/// A rule-key inside a drilled [[rules]] entry. `child`/`parent`/`link` are
/// ParentChild-only; `doc_type`/`require` are RelationExistence-only.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleKey {
    Name,
    Shape,
    Child,
    Parent,
    Link,
    DocType,
    Require,
    Severity,
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
    Rule {
        index: usize,
        key: RuleKey,
    },
    SqidsSalt,
    SqidsMinLength,
    ReservedRemote,
    ReservedFormat,
    ReservedMaxRetries,
    GithubRepo,
    GithubCacheTtl,
    CoordinationRemote,
    CoordinationLeaseDuration,
    CoordinationGracePeriod,
    CoordinationMaxPushRetries,
    CoordinationMaxClockSkew,
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
