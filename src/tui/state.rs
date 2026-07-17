mod expansion;
pub mod forms;
mod graph;
pub mod settings_guard;

mod app;

/// Per-context App seeding for the keybind-registry parity test (keybinds.rs).
#[cfg(test)]
pub(crate) use app::parity_seed;
pub use app::{
    anchor_to_flat, resolve_editor_command, resolve_editor_command_from, resolve_editor_from, App,
    AppEvent, ConfigDep, CreateResult, DocListNode, FilterField, GraphAnchor, GraphNode,
    PreviewTab, ScaffoldResult, SearchEntry, ViewMode,
};
#[cfg(feature = "agent")]
pub use forms::AgentDialog;
pub use forms::{
    CreateForm, DeleteConfirm, EditableField, FieldEditor, FieldPath, FormField, LinkEditor,
    OpenRequest, OverrideKeyPrompt, ProvenanceEditor, RelKey, RuleKey, SettingsDeleteConfirm,
    SettingsDeleteTarget, StatusPicker, TypeKey,
};
