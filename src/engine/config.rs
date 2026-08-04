use crate::engine::document::Status;
use anyhow::{bail, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// How a relationship participates in context traversal. `Chain` relationships
/// form the parent-child DAG walked by `resolve_chain`/`resolve_forest`;
/// `Related` relationships form the symmetric depth-bounded neighbourhood.
/// Absence (`None` on `RelationshipDef`) means the relationship participates in
/// neither walk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Traversal {
    Chain,
    Related,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(tag = "shape")]
pub enum ValidationRule {
    #[serde(rename = "parent-child")]
    ParentChild {
        name: String,
        child: String,
        parent: String,
        severity: Severity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        require_parent_status: Option<String>,
    },
    #[serde(rename = "relation-existence")]
    RelationExistence {
        name: String,
        #[serde(rename = "type")]
        doc_type: String,
        require: String,
        severity: Severity,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NumberingStrategy {
    #[default]
    Incremental,
    Sqids,
    Reserved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SqidsConfig {
    pub salt: String,
    #[serde(default = "default_sqids_min_length")]
    pub min_length: u8,
}

fn default_sqids_min_length() -> u8 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReservedFormat {
    Incremental,
    Sqids,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ReservedConfig {
    #[serde(default = "default_reserved_remote")]
    pub remote: String,
    pub format: ReservedFormat,
    #[serde(default = "default_reserved_max_retries")]
    pub max_retries: u8,
}

fn default_reserved_remote() -> String {
    "origin".to_string()
}

fn default_reserved_max_retries() -> u8 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default, JsonSchema)]
pub enum StoreBackend {
    #[default]
    #[serde(rename = "filesystem")]
    Filesystem,
    #[serde(rename = "github-issues")]
    GithubIssues,
    #[serde(rename = "github-milestones")]
    GithubMilestones,
    #[serde(rename = "github-projects")]
    GithubProjects,
    #[serde(rename = "git-ref")]
    GitRef,
    #[serde(rename = "clickup-tasks")]
    ClickupTasks,
}

impl fmt::Display for StoreBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreBackend::Filesystem => write!(f, "filesystem"),
            StoreBackend::GithubIssues => write!(f, "github-issues"),
            StoreBackend::GithubMilestones => write!(f, "github-milestones"),
            StoreBackend::GithubProjects => write!(f, "github-projects"),
            StoreBackend::GitRef => write!(f, "git-ref"),
            StoreBackend::ClickupTasks => write!(f, "clickup-tasks"),
        }
    }
}

impl StoreBackend {
    /// The lifecycle a remote store dictates for a type that declares none. GitHub
    /// issues and milestones are `open`/`closed`; the empty edge set leaves the
    /// pair unconstrained, so the DAG is bidirectional (reopen is a valid move) and
    /// `first_active_status`/`terminal_status` resolve to `open`/`closed`. Stores
    /// whose status is authored locally (`filesystem`, `git-ref`) or captured from
    /// the remote per-list at sync time (`clickup-tasks` persists its own derived
    /// lifecycle) return `None`.
    pub fn canonical_lifecycle(&self) -> Option<Lifecycle> {
        match self {
            StoreBackend::GithubIssues | StoreBackend::GithubMilestones => Some(Lifecycle {
                states: vec!["open".to_string(), "closed".to_string()],
                edges: Vec::new(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Authorship {
    Human,
    #[default]
    Assisted,
    Generated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
pub struct Lifecycle {
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

impl Lifecycle {
    /// The status a freshly-created document is born into: the first declared
    /// lifecycle state, or `draft` when the lifecycle declares none. Every
    /// store's create path (fs, git-ref, github) seeds from this single source
    /// so a doc is always born inside its own lifecycle.
    pub fn seed_status(&self) -> &str {
        self.states.first().map(String::as_str).unwrap_or("draft")
    }

    /// The lifecycle state a remote-`open` GitHub issue/milestone maps to on the
    /// sync read path: the type's first active state, which is the birth/seed
    /// state (`states[0]`). Shares its derivation with [`seed_status`] so a doc's
    /// open state and its birth state always agree. Falls back to `draft` when
    /// the lifecycle declares no states. The single source both this iteration's
    /// read mapping and ITERATION-319's write-through close derive from.
    pub fn first_active_status(&self) -> &str {
        self.seed_status()
    }

    /// The terminal lifecycle state a remote-`closed` GitHub issue/milestone maps
    /// to on the sync read path: the end of the lifecycle's *main forward path*.
    /// Starting at the first declared state we follow the first declared
    /// non-wildcard outgoing edge from each state until reaching a state with no
    /// such edge -- that state is terminal. For the default lifecycle
    /// (draft->review->accepted->in-progress->complete, plus `review->rejected`
    /// and `* -> superseded`) this is `complete`, never the branch terminals
    /// `rejected`/`superseded`. The universal `* -> X` wildcard edge is never
    /// followed, so a terminal reachable only by wildcard (e.g. `superseded`) is
    /// never selected. With no declared edges the lifecycle is unconstrained and
    /// the last declared state is terminal; falls back to `complete` when the
    /// lifecycle declares no states. Reused unchanged by ITERATION-319's
    /// write-through close.
    pub fn terminal_status(&self) -> &str {
        if self.states.is_empty() {
            return "complete";
        }
        if self.edges.is_empty() {
            return self.states.last().map(String::as_str).unwrap_or("complete");
        }
        let mut current = self.states[0].as_str();
        let mut visited = vec![current];
        while let Some(next) = self
            .edges
            .iter()
            .find(|e| e.from == current && e.to != current)
            .map(|e| e.to.as_str())
        {
            if visited.contains(&next) {
                break;
            }
            current = next;
            visited.push(next);
        }
        current
    }

    /// True iff a `from -> to` transition is permitted. With no declared edges
    /// the lifecycle is unconstrained: any move between declared states is
    /// allowed. Otherwise the transition must match a declared edge; a `*` edge
    /// source matches any `from`, so `* -> rejected` permits the move from any
    /// state.
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        if self.edges.is_empty() {
            return self.states.iter().any(|s| s == to);
        }
        self.edges
            .iter()
            .any(|e| (e.from == from || e.from == "*") && e.to == to)
    }

    /// The set of states reachable from `from`. With no declared edges every
    /// other declared state is reachable. Otherwise it is the declared edge
    /// targets (including wildcard targets). Used to report the allowed moves
    /// when a transition is rejected.
    pub fn targets_from(&self, from: &str) -> Vec<&str> {
        if self.edges.is_empty() {
            return self
                .states
                .iter()
                .map(String::as_str)
                .filter(|s| *s != from)
                .collect();
        }
        self.edges
            .iter()
            .filter(|e| e.from == from || e.from == "*")
            .map(|e| e.to.as_str())
            .collect()
    }
}

/// The declared kind of a custom frontmatter attribute. `Str` serializes as
/// `"string"`; the rest map to their lowercase name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AttrKind {
    Int,
    Float,
    #[serde(rename = "string")]
    Str,
    Enum,
    Date,
    Bool,
}

/// One declared custom attribute on a document type: its frontmatter key
/// (`name`), its `kind`, whether it is `required`, and for `enum` kinds the
/// permitted `values`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct AttrDef {
    pub name: String,
    pub kind: AttrKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TypeDef {
    /// The type's canonical singular name, used as its identifier in commands,
    /// relationships, and rules (e.g. `rfc`, `story`).
    pub name: String,
    /// The plural label shown in the TUI and used for grouping (e.g. `rfcs`).
    pub plural: String,
    /// The directory this type's documents live in, relative to the project
    /// root (e.g. `docs/rfcs`). Directories derive entirely from each type's
    /// own `dir`; there is no separate `[directories]` table.
    pub dir: String,
    /// The uppercase ID prefix for this type's document IDs (e.g. `RFC` yields
    /// `RFC-001`).
    pub prefix: String,
    /// An optional glyph shown beside this type in the TUI (e.g. `●`).
    pub icon: Option<String>,
    /// How new document numbers are assigned for this type: `incremental`
    /// (default), `sqids`, or `reserved`. See the `[numbering]` sections for
    /// the `sqids`/`reserved` configuration each strategy requires.
    #[serde(default)]
    pub numbering: NumberingStrategy,
    /// When true, documents of this type are authored as `PREFIX-n-slug/index.md`
    /// inside their own subdirectory rather than as a flat `.md` file.
    #[serde(default)]
    pub subdirectory: bool,
    /// The storage backend for this type's documents: `filesystem` (default),
    /// `github-issues`, `github-milestones`, `github-projects`, `git-ref`, or
    /// `clickup-tasks`.
    #[serde(default)]
    pub store: StoreBackend,
    /// When true, this type holds at most one document (e.g. a project
    /// convention), so `create` and numbering treat it as a singleton.
    #[serde(default)]
    pub singleton: bool,
    /// The name of another declared type that documents of this type belong
    /// under, if any. A child must share its parent's store backend.
    #[serde(default)]
    pub parent_type: Option<String>,
    /// The ordered list of agent action skill names offered for this type. An
    /// absent or empty list turns agent mode off for the type.
    #[serde(default)]
    pub agents: Vec<String>,
    /// A short statement of what documents of this type are for, surfaced to
    /// authors and agents as authoring guidance.
    #[serde(default)]
    pub intent: Option<String>,
    /// The authorship ceiling for this type -- how much of a document's body an
    /// AI may write: `human`, `assisted` (default), or `generated`.
    #[serde(default)]
    pub authorship: Authorship,
    /// The valid statuses (`states`) and permitted transitions (`edges`) for
    /// this type. `update --status` is gated by these edges; a lifecycle with
    /// states but no edges is unconstrained.
    #[serde(default)]
    pub lifecycle: Lifecycle,
    /// Declared custom frontmatter attributes for this type, each with a name,
    /// kind, requiredness, and (for `enum` kinds) permitted values.
    #[serde(default)]
    pub attributes: Vec<AttrDef>,
    /// Overrides the default `lazyspec:{name}` GitHub label used to identify
    /// this type's issues, for `github-issues`-backed types. Unused by other
    /// stores.
    #[serde(default, rename = "github_label")]
    pub label_override: Option<String>,
    /// A GitHub label naming this type as a classification signal, distinct
    /// from `label_override`'s identity label. Drives discovery/matching and,
    /// when set, replaces the `lazyspec:{name}` identity label as the sole
    /// label attached on create (see `github_create_labels`).
    #[serde(default)]
    pub github_issue_tag: Option<String>,
    /// A GitHub native issue type naming this type as a classification signal.
    /// Drives discovery and classification, and is pushed on create; when set,
    /// it replaces the `lazyspec:{name}` identity label (see
    /// `github_create_labels`).
    #[serde(default)]
    pub github_issue_type: Option<String>,
    /// The id of a `github-projects`-backed document (e.g. `PROJECT-7`) whose
    /// board's `Status` single-select field is the authority for this type's
    /// lifecycle. That field's options become the type's `lifecycle` states,
    /// persisted at fetch. Only the nominated board is authoritative: any other
    /// board a document belongs to still contributes plain `PROJECT-n.<field>`
    /// attributes and does not affect lifecycle. Unused by other stores.
    #[serde(default)]
    pub status_authority: Option<String>,
    /// The ClickUp List id this type binds to, for `clickup-tasks`-backed types.
    /// Each such type materializes exactly one bound List's tasks. Unused by
    /// other stores.
    #[serde(default)]
    pub clickup_list_id: Option<String>,
    /// The ClickUp custom task type (`custom_item_id`) stamped on this type's
    /// tasks, for `clickup-tasks`-backed types. A numeric id only -- name->id
    /// resolution is deferred. Setting it on any other store is a config error.
    /// Unused by other stores.
    #[serde(default)]
    pub clickup_task_type: Option<i64>,
    /// Maps a lazyspec name to the ClickUp custom-field uuid that holds it, for
    /// anything with no native ClickUp field (RFC-056 §Field mapping). The
    /// reserved key [`CLICKUP_RELATIONS_FIELD`] names the *text* field carrying
    /// the serialized relations block; every other key names a non-native
    /// attribute. Read via [`TypeDef::clickup_field_id`] (name -> uuid, the write
    /// direction) and [`TypeDef::clickup_field_name`] (uuid -> name, the decode
    /// direction). Unused by other stores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clickup_custom_field_map: Option<HashMap<String, String>>,
}

/// The reserved [`TypeDef::clickup_custom_field_map`] key naming the ClickUp
/// *text* custom field that holds a task's serialized lazyspec relations block
/// (the `issue_body.rs` YAML `- implements: RFC-056` shape). Relations round-trip
/// through one text field, not a relationship-type field (RFC-056 §Relations), so
/// one reserved key names it; any other map key names a non-native attribute.
pub const CLICKUP_RELATIONS_FIELD: &str = "relations";

/// One entry in the `[[relationships]]` block: a relationship name and its
/// optional inverse keyword. A relationship with no `inverse` is symmetric
/// (e.g. `related-to`); a directional one declares its inverse (e.g.
/// `implements` / `implemented-by`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct RelationshipDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
    /// Names a GitHub-native edge this relationship maps onto (e.g.
    /// `"milestone"`, `"sub-issue"`, `"membership"`), so linking writes the
    /// native association instead of (or alongside) the frontmatter relation.
    /// Absent for ordinary relationships.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_native: Option<String>,
    /// How this relationship participates in context traversal. `None` (the
    /// default, absent from TOML) means it drives neither the chain walk nor the
    /// related neighbourhood.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traversal: Option<Traversal>,
}

/// The canonical starter relationship vocabulary, mirroring the closed enum that
/// preceded the config registry. Used by `init`'s `starter_config`, the
/// `to_toml` writer, and the test-only `Config::default()`. The load path
/// carries none (ADR-011): a real config must declare `[[relationships]]`.
pub fn starter_relationships() -> Vec<RelationshipDef> {
    let directional = |name: &str, inverse: &str, traversal: Option<Traversal>| RelationshipDef {
        name: name.to_string(),
        inverse: Some(inverse.to_string()),
        github_native: None,
        traversal,
    };
    vec![
        directional("implements", "implemented-by", Some(Traversal::Chain)),
        directional("supersedes", "superseded-by", None),
        directional("blocks", "blocked-by", None),
        RelationshipDef {
            name: "related-to".to_string(),
            inverse: None,
            github_native: None,
            traversal: Some(Traversal::Related),
        },
    ]
}

pub(crate) fn default_lifecycle() -> Lifecycle {
    let edge = |from: &str, to: &str| Edge {
        from: from.into(),
        to: to.into(),
    };
    Lifecycle {
        states: [
            "draft",
            "review",
            "accepted",
            "in-progress",
            "complete",
            "rejected",
            "superseded",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        edges: vec![
            edge("draft", "review"),
            edge("review", "accepted"),
            edge("review", "rejected"),
            edge("accepted", "in-progress"),
            edge("in-progress", "complete"),
            edge("*", "superseded"),
        ],
    }
}

/// True iff `status` names one of `type_def`'s declared lifecycle states.
pub fn validate_status(type_def: &TypeDef, status: &Status) -> Result<()> {
    if type_def.accepts_status(status) {
        return Ok(());
    }
    bail!(
        "status \"{}\" is not a valid state for type \"{}\" (allowed: {})",
        status,
        type_def.name,
        type_def.effective_lifecycle().states.join(", ")
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentConfig {
    // Serialized so `init` writes `[[types]]` into the config it scaffolds, but
    // deserialized via `RawConfig` in `Config::parse`, not the derive.
    #[serde(skip_deserializing)]
    pub types: Vec<TypeDef>,
    pub naming: Naming,
    #[serde(skip)]
    pub sqids: Option<SqidsConfig>,
    #[serde(skip)]
    pub reserved: Option<ReservedConfig>,
    #[serde(skip)]
    pub github: Option<GithubConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    pub templates: Templates,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct StatusBarConfig {
    #[serde(default = "default_statusbar_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub left: Option<Vec<String>>,
    #[serde(default)]
    pub center: Option<Vec<String>>,
    #[serde(default)]
    pub right: Option<Vec<String>>,
}

fn default_statusbar_enabled() -> bool {
    true
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            left: None,
            center: None,
            right: None,
        }
    }
}

fn default_multiline_max_expanded_height() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MultiLineConfig {
    #[serde(default = "default_multiline_max_expanded_height")]
    pub max_expanded_height: usize,
}

impl Default for MultiLineConfig {
    fn default() -> Self {
        Self {
            max_expanded_height: default_multiline_max_expanded_height(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct UiConfig {
    #[serde(default)]
    pub ascii_diagrams: bool,
    #[serde(default)]
    pub statusbar: StatusBarConfig,
    #[serde(default)]
    pub multiline: MultiLineConfig,
    #[serde(default)]
    pub graph: GraphConfig,
    /// The `[tui.table]` block: the doc-table columns for the types view. Column
    /// ids are the built-ins `status` / `tags` / `provenance` / `related` plus any
    /// declared attribute name. Carries a serde default so a config without the
    /// block still loads with today's visual column set.
    #[serde(default)]
    pub table: TableConfig,
    /// The `[tui.status_colors]` table: per-status colour overrides mapping a
    /// status name to a raw colour string (named ANSI colour or `#rrggbb` hex).
    /// Parsing into a concrete colour happens TUI-side, not in the engine.
    #[serde(default)]
    pub status_colors: BTreeMap<String, String>,
    /// The `[tui] viewer` command spawned by `show --open` (and the TUI open
    /// keybind) to view a document that has no web URL -- git-ref/clickup docs
    /// and filesystem docs whose repo coords don't resolve. Absent -> opening
    /// such a document errors rather than guessing a viewer.
    #[serde(default)]
    pub viewer: Option<String>,
}

fn default_graph_columns() -> Vec<String> {
    vec!["status".to_string(), "related".to_string()]
}

fn default_graph_sort() -> String {
    "path".to_string()
}

/// The `[tui.graph]` block: the nested-table columns and the default sibling
/// sort column. Column ids are the built-ins `status` / `related` plus any
/// declared attribute name; `sort` is `path` (the topo tiebreak) or any column
/// id. Both carry serde defaults so a config without the block still loads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct GraphConfig {
    #[serde(default = "default_graph_columns")]
    pub columns: Vec<String>,
    #[serde(default = "default_graph_sort")]
    pub sort: String,
}

impl Default for GraphConfig {
    fn default() -> Self {
        GraphConfig {
            columns: default_graph_columns(),
            sort: default_graph_sort(),
        }
    }
}

pub fn default_table_columns() -> Vec<String> {
    vec![
        "status".to_string(),
        "tags".to_string(),
        "assignee".to_string(),
        "provenance".to_string(),
    ]
}

/// The `[tui.table]` block: the doc-table columns for the types view. Column ids
/// are the built-ins `status` / `tags` / `provenance` / `related` plus any
/// declared attribute name. Carries a serde default so a config without the block
/// still loads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct TableConfig {
    #[serde(default = "default_table_columns")]
    pub columns: Vec<String>,
}

impl Default for TableConfig {
    fn default() -> Self {
        TableConfig {
            columns: default_table_columns(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub documents: DocumentConfig,
    #[serde(flatten)]
    pub filesystem: FilesystemConfig,
    // Serialized so `to_toml` writes `[[relationships]]` into the config it emits,
    // but deserialized via `RawConfig` in `Config::parse`, not the derive.
    #[serde(skip_deserializing)]
    pub relationships: Vec<RelationshipDef>,
    #[serde(rename = "tui")]
    pub ui: UiConfig,
    // Serialized so `to_toml` writes `[[rules]]` into the config it emits, but
    // deserialized via `RawConfig` in `Config::parse`, not the derive.
    #[serde(skip_deserializing)]
    pub rules: Vec<ValidationRule>,
    #[serde(skip)]
    pub ref_count_ceiling: usize,
    #[serde(default)]
    pub certification: CertificationConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    /// The optional `[web]` repo-coordinate overrides (RFC-052). `None` when the
    /// table is absent.
    #[serde(skip)]
    pub web: Option<WebConfig>,
    /// The `[git-ref]` table: the remote the git-ref store fetches from (and,
    /// later, pushes to). Defaults to `origin`. Serialized into `config --json`
    /// but parsed via `RawConfig`.
    #[serde(rename = "git-ref", default, skip_deserializing)]
    pub git_ref: GitRefConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Templates {
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Naming {
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CertificationConfig {
    #[serde(default = "default_normalize")]
    pub normalize: bool,
    #[serde(default)]
    pub overrides: HashMap<String, CertificationOverride>,
}

impl Default for CertificationConfig {
    fn default() -> Self {
        CertificationConfig {
            normalize: default_normalize(),
            overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CertificationOverride {
    pub normalize: bool,
}

pub fn default_normalize() -> bool {
    true
}

impl CertificationConfig {
    pub fn should_normalize(&self, spec_path: &str) -> bool {
        if let Some(override_cfg) = self.overrides.get(spec_path) {
            return override_cfg.normalize;
        }
        self.normalize
    }
}

#[derive(Deserialize, JsonSchema)]
struct RawNumbering {
    sqids: Option<SqidsConfig>,
    reserved: Option<ReservedConfig>,
}

fn default_cache_ttl() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct GithubConfig {
    pub repo: Option<String>,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: u64,
}

/// The optional `[web]` table (RFC-052): overrides for the GitHub repo
/// coordinates the read-only web view deep-links against. Each field overrides
/// the value otherwise inferred from the `origin` remote (owner/repo) or the
/// current branch. All optional and independently overriding -- absence of the
/// whole table is fine and falls back to `origin`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
pub struct WebConfig {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
}

/// The `[git-ref]` table: settings for the `git-ref` document store. `remote`
/// names the git remote that git-ref fetch (and, later, push) targets and is the
/// single source of truth for it (STORY-218 AC1); it defaults to `origin`.
/// Absence of the whole table falls back to that default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct GitRefConfig {
    #[serde(default = "default_git_ref_remote")]
    pub remote: String,
}

pub fn default_git_ref_remote() -> String {
    "origin".to_string()
}

impl Default for GitRefConfig {
    fn default() -> Self {
        GitRefConfig {
            remote: default_git_ref_remote(),
        }
    }
}

/// The global `[agents]` block. `interactive` is the optional `bash -lc` shell
/// command for terminal handover (e.g. `claude "$LAZYSPEC_PROMPT"`). Zero-defaults
/// (ADR-015): absent -> None -> interactive run mode is unavailable and not offered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct AgentsConfig {
    #[serde(default)]
    pub interactive: Option<String>,
}

pub fn default_skills_entry() -> String {
    "lazy".to_string()
}

/// The global `[skills]` block. `entry` names the router skill that `skills
/// install` renames the embedded router directory to. Zero-defaults (ADR-015):
/// absent -> `entry = "lazy"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_entry")]
    pub entry: String,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        SkillsConfig {
            entry: default_skills_entry(),
        }
    }
}

#[derive(Deserialize, JsonSchema)]
struct RawConfig {
    /// The document types this project declares, one per `[[types]]` block. At
    /// least one type is required.
    types: Option<Vec<TypeDef>>,
    /// The relationship vocabulary, one per `[[relationships]]` block. Each
    /// declares a name and optional inverse; the block is required.
    relationships: Option<Vec<RelationshipDef>>,
    /// Structural validation rules between types, one per `[[rules]]` block
    /// (`parent-child` or `relation-existence` shapes), checked by `validate`.
    rules: Option<Vec<ValidationRule>>,
    /// The `[templates]` block: where Markdown document templates live.
    templates: Option<Templates>,
    /// The `[naming]` block: the filename pattern new documents are created
    /// under (e.g. `{type}-{n:03}-{title}.md`).
    naming: Option<Naming>,
    /// The `[tui]` block: terminal UI preferences (status bar, multiline
    /// rendering, graph columns, ASCII diagrams, status colours).
    tui: Option<UiConfig>,
    /// The `[numbering]` block: `sqids` and `reserved` sub-tables backing the
    /// `sqids`/`reserved` numbering strategies used by types.
    numbering: Option<RawNumbering>,
    /// The maximum number of distinct `@ref` code-reference targets a document
    /// may embed before `validate` warns that it should be split. Defaults to 15.
    #[serde(default)]
    ref_count_ceiling: Option<usize>,
    /// The `[certification]` block: whether document bodies are normalized on
    /// write, with optional per-document overrides.
    #[serde(default)]
    certification: Option<CertificationConfig>,
    /// The `[github]` block: repo coordinates and cache TTL. Required when any
    /// type uses a GitHub-backed store.
    github: Option<GithubConfig>,
    /// The `[agents]` block: the interactive agent run-mode shell command.
    #[serde(default)]
    agents: Option<AgentsConfig>,
    /// The `[skills]` block: the router skill entry name that `skills install`
    /// uses (default `lazy`).
    #[serde(default)]
    skills: Option<SkillsConfig>,
    /// The `[web]` block: overrides for the GitHub repo coordinates (owner,
    /// repo, branch) the read-only web view deep-links against.
    #[serde(default)]
    web: Option<WebConfig>,
    /// The `[git-ref]` block: the remote the git-ref store targets (default
    /// `origin`).
    #[serde(rename = "git-ref", default)]
    git_ref: Option<GitRefConfig>,
}

/// The JSON Schema for `.lazyspec.toml`, derived from the private `RawConfig`
/// deserialize path (the input grammar), not the assembled `Config` output
/// shape. Serves as an LLM-readable config reference and backs editor
/// autocomplete.
pub fn config_schema() -> schemars::Schema {
    schemars::schema_for!(RawConfig)
}

/// The canonical starter document types. The engine carries no built-in types in
/// its load path (see ADR-011); this is the set `init` writes into a fresh config
/// and the set the `#[cfg(test)]` `Config::default()` fixture uses.
pub fn starter_types() -> Vec<TypeDef> {
    let simple = |name: &str, plural: &str, dir: &str, prefix: &str, icon: &str| TypeDef {
        name: name.to_string(),
        plural: plural.to_string(),
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: Some(icon.to_string()),
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: StoreBackend::default(),
        singleton: false,
        parent_type: None,
        agents: Vec::new(),
        intent: None,
        authorship: Authorship::default(),
        lifecycle: default_lifecycle(),
        attributes: Vec::new(),
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        status_authority: None,
        clickup_list_id: None,
        clickup_task_type: None,
        clickup_custom_field_map: None,
    };
    vec![
        simple("rfc", "rfcs", "docs/rfcs", "RFC", "●"),
        simple("story", "stories", "docs/stories", "STORY", "▲"),
        simple(
            "iteration",
            "iterations",
            "docs/iterations",
            "ITERATION",
            "◆",
        ),
        simple("adr", "adrs", "docs/adrs", "ADR", "■"),
        simple("spec", "specs", "docs/specs", "SPEC", "📋"),
        TypeDef {
            name: "convention".to_string(),
            plural: "convention".to_string(),
            dir: "docs/convention".to_string(),
            prefix: "CONVENTION".to_string(),
            icon: Some("📜".to_string()),
            numbering: NumberingStrategy::default(),
            subdirectory: true,
            store: StoreBackend::default(),
            singleton: true,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Authorship::default(),
            lifecycle: default_lifecycle(),
            attributes: Vec::new(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        },
        TypeDef {
            name: "dictum".to_string(),
            plural: "dicta".to_string(),
            dir: "docs/convention".to_string(),
            prefix: "DICTUM".to_string(),
            icon: Some("⚖".to_string()),
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store: StoreBackend::default(),
            singleton: false,
            parent_type: Some("convention".to_string()),
            agents: Vec::new(),
            intent: None,
            authorship: Authorship::default(),
            lifecycle: default_lifecycle(),
            attributes: Vec::new(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        },
    ]
}

/// The canonical starter validation rules. Not injected by the load path; only
/// the config `init` writes and the test-only `Config::default()` use these.
pub fn default_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule::ParentChild {
            name: "stories-need-rfcs".to_string(),
            child: "story".to_string(),
            parent: "rfc".to_string(),
            severity: Severity::Warning,
            require_parent_status: None,
        },
        ValidationRule::ParentChild {
            name: "iterations-need-stories".to_string(),
            child: "iteration".to_string(),
            parent: "story".to_string(),
            severity: Severity::Error,
            require_parent_status: None,
        },
        ValidationRule::RelationExistence {
            name: "adrs-need-relations".to_string(),
            doc_type: "adr".to_string(),
            require: "any-relation".to_string(),
            severity: Severity::Error,
        },
    ]
}

#[cfg(any(test, feature = "test-support"))]
impl Default for Config {
    fn default() -> Self {
        Config {
            documents: DocumentConfig {
                types: starter_types(),
                naming: Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                },
                sqids: None,
                reserved: None,
                github: None,
            },
            filesystem: FilesystemConfig {
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            relationships: starter_relationships(),
            ui: UiConfig::default(),
            rules: default_rules(),
            ref_count_ceiling: 15,
            certification: CertificationConfig::default(),
            agents: AgentsConfig::default(),
            skills: SkillsConfig::default(),
            web: None,
            git_ref: GitRefConfig::default(),
        }
    }
}

impl DocumentConfig {
    pub fn github_issues_types(&self) -> Vec<&str> {
        self.types
            .iter()
            .filter(|t| t.store == StoreBackend::GithubIssues)
            .map(|t| t.name.as_str())
            .collect()
    }

    pub fn has_github_issues_types(&self) -> bool {
        self.types
            .iter()
            .any(|t| t.store == StoreBackend::GithubIssues)
    }
}

// A TOML-level failure aborts before serde (and thus before any of parse_inner's
// own diagnostics) can run, so the raw message is all the user gets. The one
// shape worth enriching is a duplicate `attributes` key on a type -- an inline
// `attributes = []` colliding with `[[types.attributes]]` blocks (the hand-edit
// that broke this very repo's config, AUDIT-018 F1). TOML rejects it at the
// grammar level, so the type is named here by scanning the source above the
// error span; the original error (with its line/column snippet) is kept below
// the hint. Anything else passes through untouched.
fn enrich_parse_error(toml_str: &str, err: toml::de::Error) -> anyhow::Error {
    if !err.message().contains("duplicate key `attributes`") {
        return err.into();
    }
    let offset = err.span().map(|s| s.start).unwrap_or(0);
    let Some(name) = enclosing_type_name(toml_str, offset) else {
        return err.into();
    };
    anyhow::anyhow!(
        "type \"{name}\" in .lazyspec.toml declares `attributes` twice: an inline \
         `attributes = []` key and [[types.attributes]] blocks. Delete the inline \
         `attributes = [...]` line and keep the [[types.attributes]] blocks.\n\n{err}"
    )
}

// The `name` of the [[types]] block enclosing byte `offset`, per the source
// text alone (the document is unparseable at this point). Sub-table headers
// like [[types.attributes]] stay inside the block; any other header ends it.
fn enclosing_type_name(toml_str: &str, offset: usize) -> Option<String> {
    let head = &toml_str[..offset.min(toml_str.len())];
    let mut name = None;
    let mut in_type = false;
    for line in head.lines() {
        let line = line.trim();
        if line.starts_with("[[types]]") {
            in_type = true;
            name = None;
        } else if line.starts_with('[') && !line.starts_with("[[types.") {
            in_type = false;
            name = None;
        } else if in_type && name.is_none() {
            if let Some(value) = line
                .strip_prefix("name")
                .map(str::trim_start)
                .and_then(|rest| rest.strip_prefix('='))
            {
                name = Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    name
}

impl Config {
    pub fn parse(toml_str: &str) -> Result<Self> {
        Self::parse_inner(toml_str, false)
    }

    /// Lenient parse used only by `fix --config`: tolerates a missing
    /// `[[relationships]]` block (treating it as empty) so the migration can
    /// read an upgraded legacy config that strict load would reject. All other
    /// validation (types, numbering, github, etc.) is preserved.
    pub fn parse_lenient(toml_str: &str) -> Result<Self> {
        Self::parse_inner(toml_str, true)
    }

    fn parse_inner(toml_str: &str, lenient: bool) -> Result<Self> {
        let raw: RawConfig =
            toml::from_str(toml_str).map_err(|e| enrich_parse_error(toml_str, e))?;

        let types = match raw.types {
            Some(types) if !types.is_empty() => types,
            _ => bail!(
                ".lazyspec.toml is missing required [[types]]. Run `lazyspec init` to scaffold a config."
            ),
        };

        let relationships = match raw.relationships {
            Some(relationships) => relationships,
            None if lenient => Vec::new(),
            None => bail!(
                "[[relationships]] section is required; run `lazyspec fix --config` to add the standard set"
            ),
        };

        let sub_issue_rels: Vec<&str> = relationships
            .iter()
            .filter(|r| r.github_native.as_deref() == Some("sub-issue"))
            .map(|r| r.name.as_str())
            .collect();
        if sub_issue_rels.len() > 1 {
            bail!(
                "at most one relationship may declare github_native = \"sub-issue\" \
                 (GitHub sub-issues are single-parent); found {}",
                sub_issue_rels.join(", ")
            );
        }

        let rules = raw.rules.unwrap_or_default();

        let any_sqids = types
            .iter()
            .any(|t| t.numbering == NumberingStrategy::Sqids);
        let (sqids, reserved) = match raw.numbering {
            Some(n) => (n.sqids, n.reserved),
            None => (None, None),
        };

        if any_sqids {
            let Some(ref sqids_cfg) = sqids else {
                bail!("numbering = \"sqids\" requires a [numbering.sqids] section with a non-empty salt");
            };
            if sqids_cfg.salt.is_empty() {
                bail!("numbering.sqids.salt must not be empty");
            }
            if sqids_cfg.min_length < 1 || sqids_cfg.min_length > 10 {
                bail!(
                    "numbering.sqids.min_length must be between 1 and 10, got {}",
                    sqids_cfg.min_length
                );
            }
        }

        let any_reserved = types
            .iter()
            .any(|t| t.numbering == NumberingStrategy::Reserved);
        if any_reserved {
            let Some(ref reserved_cfg) = reserved else {
                bail!("numbering = \"reserved\" requires a [numbering.reserved] section");
            };
            if reserved_cfg.remote.is_empty() {
                bail!("numbering.reserved.remote must not be empty");
            }
            if reserved_cfg.format == ReservedFormat::Sqids {
                let Some(ref sqids_cfg) = sqids else {
                    bail!("numbering.reserved.format = \"sqids\" requires a [numbering.sqids] section with a non-empty salt");
                };
                if sqids_cfg.salt.is_empty() {
                    bail!("numbering.reserved.format = \"sqids\" requires a non-empty numbering.sqids.salt");
                }
                if sqids_cfg.min_length < 1 || sqids_cfg.min_length > 10 {
                    bail!(
                        "numbering.sqids.min_length must be between 1 and 10, got {}",
                        sqids_cfg.min_length
                    );
                }
            }
        }

        let any_github = types.iter().any(|t| {
            matches!(
                t.store,
                StoreBackend::GithubIssues
                    | StoreBackend::GithubMilestones
                    | StoreBackend::GithubProjects
            )
        });
        if any_github && raw.github.is_none() {
            bail!("store = \"github-issues\", \"github-milestones\", or \"github-projects\" requires a [github] section");
        }

        if let Some(t) = types
            .iter()
            .find(|t| t.clickup_task_type.is_some() && t.store != StoreBackend::ClickupTasks)
        {
            bail!(
                "type \"{}\" sets clickup_task_type but store = \"{}\"; clickup_task_type is only valid on store = \"clickup-tasks\"",
                t.name,
                t.store
            );
        }

        let ref_count_ceiling = raw.ref_count_ceiling.unwrap_or(15);

        Ok(Config {
            documents: DocumentConfig {
                types,
                naming: raw.naming.unwrap_or(Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                }),
                sqids,
                reserved,
                github: raw.github,
            },
            filesystem: FilesystemConfig {
                templates: raw.templates.unwrap_or(Templates {
                    dir: ".lazyspec/templates".to_string(),
                }),
            },
            relationships,
            ui: raw.tui.unwrap_or_default(),
            rules,
            ref_count_ceiling,
            certification: raw.certification.unwrap_or_default(),
            agents: raw.agents.unwrap_or_default(),
            skills: raw.skills.unwrap_or_default(),
            web: raw.web,
            git_ref: raw.git_ref.unwrap_or_default(),
        })
    }

    pub fn load(
        project_root: &std::path::Path,
        fs: &dyn crate::engine::fs::FileSystem,
    ) -> Result<Self> {
        let path = project_root.join(".lazyspec.toml");
        if !fs.exists(&path) {
            bail!(
                "no .lazyspec.toml found in {}. Run `lazyspec init` to scaffold a config.",
                project_root.display()
            );
        }
        let content = fs.read_to_string(&path)?;
        Self::parse(&content)
    }

    /// Lenient counterpart of [`Config::load`], used only by `fix --config`.
    /// Reads `.lazyspec.toml` without enforcing the strict `[[relationships]]`
    /// requirement, so the migration can repair the very config strict load
    /// would reject.
    pub fn load_lenient(
        project_root: &std::path::Path,
        fs: &dyn crate::engine::fs::FileSystem,
    ) -> Result<Self> {
        let path = project_root.join(".lazyspec.toml");
        if !fs.exists(&path) {
            bail!(
                "no .lazyspec.toml found in {}. Run `lazyspec init` to scaffold a config.",
                project_root.display()
            );
        }
        let content = fs.read_to_string(&path)?;
        Self::parse_lenient(&content)
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn type_by_name(&self, name: &str) -> Option<&TypeDef> {
        self.documents.types.iter().find(|t| t.name == name)
    }

    /// The relationship declared under the canonical `name`, if any.
    pub fn relationship_by_name(&self, name: &str) -> Option<&RelationshipDef> {
        self.relationships.iter().find(|r| r.name == name)
    }

    /// The relationship whose `github_native` edge equals `native` (e.g.
    /// `"milestone"`), if one is declared. Lets fetch resolve the rel name to
    /// surface a native association under rather than hardcoding `targets`.
    pub fn relationship_by_github_native(&self, native: &str) -> Option<&RelationshipDef> {
        self.relationships
            .iter()
            .find(|r| r.github_native.as_deref() == Some(native))
    }

    /// The declared inverse keyword for a canonical relationship `name`, if it
    /// is directional. Symmetric relationships return `None`.
    pub fn inverse_of(&self, name: &str) -> Option<&str> {
        self.relationship_by_name(name)
            .and_then(|r| r.inverse.as_deref())
    }

    /// All link/unlink keywords the config declares: each relationship `name`
    /// followed by each declared `inverse`. The source of truth for both shell
    /// completion and the TUI link-editor cycler.
    pub fn relationship_keywords(&self) -> Vec<String> {
        let mut keywords: Vec<String> = self.relationships.iter().map(|r| r.name.clone()).collect();
        keywords.extend(self.relationships.iter().filter_map(|r| r.inverse.clone()));
        keywords
    }

    /// Resolve a link/unlink keyword against the registry. A keyword matching a
    /// declared `name` resolves to `(name, false)` (not flipped); a keyword
    /// matching some declared `inverse` resolves to that name `(name, true)`
    /// (direction flipped). An unknown keyword is an error naming it. The
    /// config registry is the sole source of relationship names and inverses.
    pub fn resolve_relationship(&self, keyword: &str) -> Result<(String, bool)> {
        let keyword = keyword.to_lowercase();
        if let Some(rel) = self.relationship_by_name(&keyword) {
            return Ok((rel.name.clone(), false));
        }
        if let Some(rel) = self
            .relationships
            .iter()
            .find(|r| r.inverse.as_deref() == Some(keyword.as_str()))
        {
            return Ok((rel.name.clone(), true));
        }
        bail!(
            "unknown relationship \"{}\" (not declared in [[relationships]])",
            keyword
        )
    }
}

impl TypeDef {
    pub fn make_id(&self, suffix: impl std::fmt::Display) -> String {
        format!("{}-{}", self.prefix, suffix)
    }

    /// True iff `status` names one of this type's effective lifecycle states
    /// (declared, or the store's canonical lifecycle when none is declared).
    pub fn accepts_status(&self, status: &Status) -> bool {
        self.effective_lifecycle()
            .states
            .iter()
            .any(|s| s == status.as_str())
    }

    /// The lifecycle to use for status derivation, transition gating, and the DAG:
    /// the declared `lifecycle` when it names any states, else the store backend's
    /// canonical lifecycle (github `open`/`closed`). A declared lifecycle always
    /// wins; a store with no canonical lifecycle leaves the (possibly empty)
    /// declared one untouched. Consult this rather than `self.lifecycle` directly
    /// wherever a github-backed type's states must reflect the remote.
    pub fn effective_lifecycle(&self) -> std::borrow::Cow<'_, Lifecycle> {
        use std::borrow::Cow;
        if !self.lifecycle.states.is_empty() {
            return Cow::Borrowed(&self.lifecycle);
        }
        match self.store.canonical_lifecycle() {
            Some(lc) => Cow::Owned(lc),
            None => Cow::Borrowed(&self.lifecycle),
        }
    }

    /// The GitHub label identifying this type's issues: the configured
    /// `label_override` if set, else the default `lazyspec:{name}`.
    pub fn github_label(&self) -> String {
        self.label_override
            .clone()
            .unwrap_or_else(|| crate::engine::gh::type_label(&self.name))
    }

    /// The GitHub labels to attach when creating this type's issues. Setting
    /// either classification signal (`github_issue_tag`, `github_issue_type`)
    /// opts the type out of the `lazyspec:{name}` identity label, mirroring the
    /// read side: [`crate::engine::issue_body::extract_type_and_tags`] skips the
    /// identity label once either is configured, so attaching it on create would
    /// stamp issues with a label nothing matches on. Such types get only their
    /// `github_issue_tag`, if one is set. Types with neither signal keep the
    /// identity label (`github_label`).
    pub fn github_create_labels(&self) -> Vec<String> {
        if self.github_issue_tag.is_some() || self.github_issue_type.is_some() {
            self.github_issue_tag.clone().into_iter().collect()
        } else {
            vec![self.github_label()]
        }
    }

    /// Resolve a lazyspec name to the ClickUp custom-field uuid holding it (the
    /// *write* direction). `None` when no `clickup_custom_field_map` is
    /// configured or the name is unmapped.
    pub fn clickup_field_id(&self, name: &str) -> Option<&str> {
        self.clickup_custom_field_map
            .as_ref()?
            .get(name)
            .map(String::as_str)
    }

    /// Resolve a ClickUp custom-field uuid back to the lazyspec name it holds
    /// (the *decode* direction). `None` when no `clickup_custom_field_map` is
    /// configured or no mapping points at that uuid.
    pub fn clickup_field_name(&self, field_id: &str) -> Option<&str> {
        self.clickup_custom_field_map
            .as_ref()?
            .iter()
            .find(|(_, uuid)| uuid.as_str() == field_id)
            .map(|(name, _)| name.as_str())
    }
}

#[cfg(test)]
impl TypeDef {
    pub fn test_fixture(name: &str, store: StoreBackend) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}", name),
            prefix: name.to_uppercase(),
            icon: None,
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Authorship::default(),
            lifecycle: Lifecycle::default(),
            attributes: Vec::new(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid `[[types]]` + `[[relationships]]` preamble. Strict load
    /// requires at least one type and a `[[relationships]]` block, so tests that
    /// only exercise other sections prepend this.
    const TYPES: &str = r#"
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

    /// A standalone `[[relationships]]` block for tests that build their own
    /// `[[types]]` inline (which would otherwise trip the strict-load error).
    const RELATIONSHIPS: &str = r#"
[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"
"#;

    #[test]
    fn config_schema_serializes_and_encodes_input_grammar() {
        let schema = config_schema();
        let json = serde_json::to_value(&schema).expect("schema serializes to JSON");

        assert!(
            json["properties"]["types"].is_object(),
            "schema must expose the top-level `types` property"
        );

        // The internally `shape`-tagged ValidationRule enum lands as a `oneOf`
        // of two subschemas, each pinning `shape` to a const kebab-case tag.
        let variants = json["$defs"]["ValidationRule"]["oneOf"]
            .as_array()
            .expect("ValidationRule must be a oneOf of variant subschemas");
        assert_eq!(variants.len(), 2, "two rule shapes");

        let shape_consts: Vec<&str> = variants
            .iter()
            .filter_map(|v| v["properties"]["shape"]["const"].as_str())
            .collect();
        assert!(
            shape_consts.contains(&"parent-child"),
            "expected a parent-child shape const, got {shape_consts:?}"
        );
        assert!(
            shape_consts.contains(&"relation-existence"),
            "expected a relation-existence shape const, got {shape_consts:?}"
        );
    }

    #[test]
    fn test_store_backend_display() {
        assert_eq!(StoreBackend::Filesystem.to_string(), "filesystem");
        assert_eq!(StoreBackend::GithubIssues.to_string(), "github-issues");
    }

    #[test]
    fn test_certification_default_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.certification.normalize);
        assert!(config.certification.overrides.is_empty());
    }

    #[test]
    fn test_certification_explicit_true() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[certification]
normalize = true
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert!(config.certification.normalize);
    }

    #[test]
    fn test_certification_explicit_false() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[certification]
normalize = false
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert!(!config.certification.normalize);
    }

    #[test]
    fn test_certification_override_disables_normalize() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[certification]
normalize = true

[certification.overrides."docs/specs/SPEC-007"]
normalize = false
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert!(!config.certification.should_normalize("docs/specs/SPEC-007"));
    }

    #[test]
    fn test_certification_override_does_not_affect_other_specs() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[certification]
normalize = true

[certification.overrides."docs/specs/SPEC-007"]
normalize = false
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert!(config.certification.should_normalize("docs/specs/SPEC-008"));
    }

    #[test]
    fn test_should_normalize_falls_back_to_global() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[certification]
normalize = false
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert!(!config.certification.should_normalize("docs/specs/SPEC-001"));
    }

    #[test]
    fn test_store_backend_defaults_to_filesystem() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::Filesystem);
    }

    #[test]
    fn test_store_backend_parses_github_issues() {
        let toml_str = r#"
[github]
repo = "owner/repo"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "github-issues"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::GithubIssues);
    }

    #[test]
    fn test_store_backend_parses_filesystem_explicit() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "filesystem"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::Filesystem);
    }

    #[test]
    fn test_store_backend_mixed_types() {
        let toml_str = r#"
[github]
repo = "owner/repo"

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
store = "github-issues"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::Filesystem);
        assert_eq!(config.documents.types[1].store, StoreBackend::GithubIssues);
    }

    #[test]
    fn test_github_config_defaults() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[github]
repo = "owner/repo"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let gh = config.documents.github.unwrap();
        assert_eq!(gh.repo.as_deref(), Some("owner/repo"));
        assert_eq!(gh.cache_ttl, 60);
    }

    #[test]
    fn test_github_config_custom_cache_ttl() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[github]
repo = "owner/repo"
cache_ttl = 120
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let gh = config.documents.github.unwrap();
        assert_eq!(gh.cache_ttl, 120);
    }

    #[test]
    fn test_github_config_absent_when_not_needed() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "filesystem"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert!(config.documents.github.is_none());
    }

    #[test]
    fn test_github_issues_without_github_section_fails() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "github-issues"
"#;
        let err = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap_err();
        assert!(
            err.to_string().contains("[github] section"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn github_issues_types_filters_by_store_backend() {
        let toml_str = r#"
[github]
repo = "owner/repo"

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
store = "github-issues"

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"
store = "github-issues"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.github_issues_types(), vec!["story", "adr"]);
    }

    #[test]
    fn github_issues_types_empty_when_all_filesystem() {
        let config = Config::default();
        assert!(config.documents.github_issues_types().is_empty());
    }

    #[test]
    fn has_github_issues_types_true_when_present() {
        let toml_str = r#"
[github]
repo = "owner/repo"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
store = "github-issues"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert!(config.documents.has_github_issues_types());
    }

    #[test]
    fn has_github_issues_types_false_when_filesystem_only() {
        let config = Config::default();
        assert!(!config.documents.has_github_issues_types());
    }

    #[test]
    fn test_make_id_basic() {
        let td = TypeDef::test_fixture("story", StoreBackend::Filesystem);
        assert_eq!(td.make_id(42), "STORY-42");
    }

    #[test]
    fn test_make_id_with_zero_padded_suffix() {
        let td = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        assert_eq!(td.make_id(format_args!("{:03}", 7)), "RFC-007");
    }

    #[test]
    fn test_make_id_with_string_suffix() {
        let td = TypeDef::test_fixture("adr", StoreBackend::Filesystem);
        assert_eq!(td.make_id("abc"), "ADR-abc");
    }

    #[test]
    fn test_github_issues_without_repo_parses() {
        let toml_str = r#"
[github]
cache_ttl = 30

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "github-issues"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        let gh = config.documents.github.unwrap();
        assert!(gh.repo.is_none());
    }

    #[test]
    fn test_store_backend_display_git_ref() {
        assert_eq!(StoreBackend::GitRef.to_string(), "git-ref");
    }

    #[test]
    fn test_store_backend_display_github_milestones() {
        assert_eq!(
            StoreBackend::GithubMilestones.to_string(),
            "github-milestones"
        );
    }

    #[test]
    fn test_store_backend_parses_github_milestones() {
        let toml_str = r#"
[github]
repo = "owner/repo"

[[types]]
name = "milestone"
plural = "milestones"
dir = "docs/milestones"
prefix = "MILESTONE"
store = "github-milestones"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(
            config.documents.types[0].store,
            StoreBackend::GithubMilestones
        );
    }

    #[test]
    fn test_store_backend_display_github_projects() {
        assert_eq!(StoreBackend::GithubProjects.to_string(), "github-projects");
    }

    #[test]
    fn test_store_backend_parses_github_projects() {
        let toml_str = r#"
[github]
repo = "owner/repo"

[[types]]
name = "project"
plural = "projects"
dir = "docs/projects"
prefix = "PROJECT"
store = "github-projects"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(
            config.documents.types[0].store,
            StoreBackend::GithubProjects
        );
    }

    #[test]
    fn test_github_projects_without_github_section_fails() {
        let toml_str = r#"
[[types]]
name = "project"
plural = "projects"
dir = "docs/projects"
prefix = "PROJECT"
store = "github-projects"
"#;
        let err = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap_err();
        assert!(err.to_string().contains("[github] section"), "got: {err}");
    }

    #[test]
    fn relationship_github_native_membership_round_trips() {
        let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[relationships]]
name = "member-of"
inverse = "has-member"
github_native = "membership"

[[relationships]]
name = "related-to"
"#;
        let config = Config::parse(toml_str).unwrap();
        let rel = config.relationship_by_name("member-of").unwrap();
        assert_eq!(rel.github_native.as_deref(), Some("membership"));
        let emitted = config.to_toml().unwrap();
        assert!(
            emitted.contains("github_native = \"membership\""),
            "{emitted}"
        );
    }

    // STORY-245 AC7: at most one relationship may declare
    // github_native = "sub-issue" -- GitHub sub-issues are single-parent, so two
    // competing relations would fight over the edge. Config load must reject it
    // with a message naming both offending relationships.
    #[test]
    fn two_sub_issue_native_relationships_rejected() {
        let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[relationships]]
name = "implements"
inverse = "implemented-by"
github_native = "sub-issue"

[[relationships]]
name = "parent-of"
inverse = "child-of"
github_native = "sub-issue"
"#;
        let err = Config::parse(toml_str).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("sub-issue"),
            "error should mention sub-issue, got: {msg}"
        );
        assert!(
            msg.contains("implements") && msg.contains("parent-of"),
            "error should name both offending relationships, got: {msg}"
        );
    }

    // A single sub-issue-native relationship is fine.
    #[test]
    fn one_sub_issue_native_relationship_ok() {
        let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[relationships]]
name = "implements"
inverse = "implemented-by"
github_native = "sub-issue"
"#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(
            config
                .relationship_by_name("implements")
                .unwrap()
                .github_native
                .as_deref(),
            Some("sub-issue")
        );
    }

    #[test]
    fn test_github_milestones_without_github_section_fails() {
        let toml_str = r#"
[[types]]
name = "milestone"
plural = "milestones"
dir = "docs/milestones"
prefix = "MILESTONE"
store = "github-milestones"
"#;
        let err = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap_err();
        assert!(err.to_string().contains("[github] section"), "got: {err}");
    }

    #[test]
    fn relationship_github_native_round_trips() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "targets"
inverse = "targeted-by"
github_native = "milestone"

[[relationships]]
name = "related-to"
"#;
        let config = Config::parse(toml_str).unwrap();
        let rel = config.relationship_by_name("targets").unwrap();
        assert_eq!(rel.github_native.as_deref(), Some("milestone"));
        // A relationship without the key carries None.
        assert!(config
            .relationship_by_name("related-to")
            .unwrap()
            .github_native
            .is_none());
        // The field survives the to_toml writer.
        let emitted = config.to_toml().unwrap();
        assert!(
            emitted.contains("github_native = \"milestone\""),
            "{emitted}"
        );
    }

    #[test]
    fn test_store_backend_parses_git_ref() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "git-ref"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::GitRef);
    }

    // STORY-218 AC1: with no [git-ref] table the remote defaults to `origin`.
    #[test]
    fn git_ref_remote_defaults_to_origin() {
        let config = Config::parse(TYPES).unwrap();
        assert_eq!(config.git_ref.remote, "origin");
    }

    // STORY-218 AC1: a [git-ref] remote override is parsed and wins over the default.
    #[test]
    fn git_ref_remote_override_is_honoured() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[git-ref]
remote = "upstream"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert_eq!(config.git_ref.remote, "upstream");
    }

    #[test]
    fn test_multiline_config_defaults() {
        let cfg = MultiLineConfig::default();
        assert_eq!(cfg.max_expanded_height, 5);
    }

    #[test]
    fn test_multiline_config_parses_max_expanded_height() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[tui.multiline]
max_expanded_height = 3
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert_eq!(config.ui.multiline.max_expanded_height, 3);
    }

    #[test]
    fn test_multiline_config_defaults_when_section_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert_eq!(config.ui.multiline.max_expanded_height, 5);
    }

    #[test]
    fn test_git_ref_does_not_affect_other_backends() {
        let toml_str = r#"
[github]
repo = "owner/repo"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
store = "git-ref"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
store = "github-issues"

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::GitRef);
        assert_eq!(config.documents.types[1].store, StoreBackend::GithubIssues);
        assert_eq!(config.documents.types[2].store, StoreBackend::Filesystem);
    }

    // AC6: missing [[relationships]] block is a hard load error.
    #[test]
    fn parse_without_relationships_block_is_hard_error() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
"#;
        let err = Config::parse(toml_str).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("[[relationships]]"),
            "error should name the missing section, got: {msg}"
        );
        assert!(
            msg.contains("lazyspec fix"),
            "error should point at the fix remedy, got: {msg}"
        );
    }

    // STORY-213 AC4: the c83bb99 outage shape -- a type carrying BOTH an inline
    // `attributes = []` key AND [[types.attributes]] blocks -- is a TOML
    // duplicate-key error before serde ever runs, so the enriched parse error
    // must name the offending type and say how to fix it, on both the strict
    // and lenient (`fix --config`) paths.
    #[test]
    fn duplicate_attributes_key_error_names_the_type() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "bug"
plural = "bugs"
dir = "docs/bugs"
prefix = "BUG"
attributes = []

[[types.attributes]]
name = "severity"
kind = "enum"
values = ["low", "high"]

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;
        for parse in [Config::parse, Config::parse_lenient] {
            let err = parse(toml_str).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("\"bug\""),
                "error should name the offending type, got: {msg}"
            );
            assert!(
                msg.contains("attributes = []") && msg.contains("[[types.attributes]]"),
                "error should describe both conflicting forms, got: {msg}"
            );
            assert!(
                msg.contains("line"),
                "error should keep the TOML line context, got: {msg}"
            );
        }
    }

    // A duplicate key unrelated to the attributes shape keeps the plain TOML
    // error (line context included), with no bogus type-naming hint.
    #[test]
    fn unrelated_duplicate_key_keeps_plain_toml_error() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
dir = "docs/other"
prefix = "RFC"
"#;
        let err = Config::parse(toml_str).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("duplicate key"), "got: {msg}");
        assert!(!msg.contains("[[types.attributes]]"), "got: {msg}");
    }

    #[test]
    fn parse_with_relationships_block_succeeds() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.relationship_by_name("implements").is_some());
        assert!(config.relationship_by_name("related-to").is_some());
    }

    #[test]
    fn parse_skills_entry_round_trips() {
        let src = format!("{TYPES}\n[skills]\nentry = \"go\"\n");
        let config = Config::parse(&src).unwrap();
        assert_eq!(config.skills.entry, "go");
    }

    #[test]
    fn parse_without_skills_section_defaults_to_lazy() {
        let config = Config::parse(TYPES).unwrap();
        assert_eq!(config.skills.entry, "lazy");
    }

    // AC1: a [[types]] entry with no `agents` key loads and resolves to an empty
    // action set (agent mode off; not an error).
    #[test]
    fn type_without_agents_key_parses_and_is_empty() {
        let config = Config::parse(TYPES).unwrap();
        let rfc = config.type_by_name("rfc").unwrap();
        assert!(rfc.agents.is_empty());
    }

    // AC2: an explicit empty `agents = []` loads and resolves to an empty action
    // set (off), exactly as an absent key does.
    #[test]
    fn type_with_empty_agents_list_parses_empty() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
agents = []
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert!(config.type_by_name("rfc").unwrap().agents.is_empty());
    }

    // AC2-shape: a malformed `agents` (not a list of strings) is a strict-load
    // error -- the shape is validated even though empty/absent is allowed.
    #[test]
    fn type_with_malformed_agents_shape_is_error() {
        let as_string = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
agents = "expand"
"#;
        assert!(Config::parse(&format!("{as_string}{RELATIONSHIPS}")).is_err());

        let as_ints = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
agents = [1, 2]
"#;
        assert!(Config::parse(&format!("{as_ints}{RELATIONSHIPS}")).is_err());
    }

    // AC3: a type that declares `agents` with templates assumed loaded carries the
    // declared list through parse in declared order.
    #[test]
    fn type_with_agents_list_parses_in_declared_order() {
        let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
agents = ["expand", "create-children"]
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(
            config.type_by_name("story").unwrap().agents,
            vec!["expand".to_string(), "create-children".to_string()]
        );
    }

    // AC1: an [agents] block with `interactive` parses into config.
    #[test]
    fn agents_config_parses_interactive() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[agents]
interactive = 'claude "$LAZYSPEC_PROMPT"'
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert_eq!(
            config.agents.interactive,
            Some("claude \"$LAZYSPEC_PROMPT\"".to_string())
        );
    }

    // AC1: absent [agents] block -> interactive is None (zero-defaults: unavailable).
    #[test]
    fn agents_config_none_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.agents.interactive.is_none());
    }

    // AC4: edge lookup honours declared edges and the `*` wildcard source.
    #[test]
    fn lifecycle_has_edge_honours_declared_edges_and_wildcard() {
        let lc = default_lifecycle();
        assert!(lc.has_edge("draft", "review"));
        assert!(lc.has_edge("review", "accepted"));
        assert!(!lc.has_edge("draft", "accepted"));
        // `* -> superseded` matches any source.
        assert!(lc.has_edge("draft", "superseded"));
        assert!(lc.has_edge("complete", "superseded"));
        // No reverse edge unless declared.
        assert!(!lc.has_edge("review", "draft"));
    }

    #[test]
    fn lifecycle_targets_from_includes_wildcard() {
        let lc = default_lifecycle();
        let from_draft = lc.targets_from("draft");
        assert!(from_draft.contains(&"review"));
        assert!(from_draft.contains(&"superseded"));
        assert!(!from_draft.contains(&"accepted"));
    }

    // STORY-223 AC1: the default lifecycle's remote-open state is its first
    // active state (`draft`) and its remote-closed state is the main-forward-path
    // terminal (`complete`), never the branch terminals `rejected`/`superseded`.
    #[test]
    fn lifecycle_first_active_and_terminal_default() {
        let lc = default_lifecycle();
        assert_eq!(lc.first_active_status(), "draft");
        assert_eq!(lc.terminal_status(), "complete");
    }

    // STORY-223 AC1: a custom lifecycle derives its own first-active and terminal
    // states from its own states/edges.
    #[test]
    fn lifecycle_first_active_and_terminal_custom() {
        let edge = |from: &str, to: &str| Edge {
            from: from.into(),
            to: to.into(),
        };
        let lc = Lifecycle {
            states: vec![
                "backlog".into(),
                "doing".into(),
                "shipped".into(),
                "abandoned".into(),
            ],
            edges: vec![
                edge("backlog", "doing"),
                edge("doing", "shipped"),
                edge("backlog", "abandoned"),
            ],
        };
        assert_eq!(lc.first_active_status(), "backlog");
        // `shipped` is the main-forward-path terminal (backlog->doing->shipped);
        // `abandoned` is a branch terminal and is not selected.
        assert_eq!(lc.terminal_status(), "shipped");
    }

    // With no declared edges the lifecycle is unconstrained: terminal falls back
    // to the last declared state, first-active to the first.
    #[test]
    fn lifecycle_terminal_no_edges_uses_last_state() {
        let lc = Lifecycle {
            states: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![],
        };
        assert_eq!(lc.first_active_status(), "a");
        assert_eq!(lc.terminal_status(), "c");
    }

    // A lifecycle with no states at all falls back to the draft/complete defaults.
    #[test]
    fn lifecycle_first_active_and_terminal_empty_defaults() {
        let lc = Lifecycle::default();
        assert_eq!(lc.first_active_status(), "draft");
        assert_eq!(lc.terminal_status(), "complete");
    }

    // STORY-224 AC1/AC2: a github-backed type with no declared lifecycle resolves
    // to the store's canonical open/closed lifecycle -- bidirectional (empty edges)
    // with open first-active and closed terminal.
    #[test]
    fn effective_lifecycle_github_undeclared_is_open_closed() {
        for store in [StoreBackend::GithubMilestones, StoreBackend::GithubIssues] {
            let td = TypeDef::test_fixture("m", store);
            let lc = td.effective_lifecycle();
            assert_eq!(lc.states, vec!["open".to_string(), "closed".to_string()]);
            assert_eq!(lc.first_active_status(), "open");
            assert_eq!(lc.terminal_status(), "closed");
            // bidirectional DAG: open<->closed both reachable.
            assert!(lc.targets_from("open").contains(&"closed"));
            assert!(lc.targets_from("closed").contains(&"open"));
        }
    }

    // STORY-224 AC3: a declared non-empty lifecycle wins over the store canonical.
    #[test]
    fn effective_lifecycle_declared_wins_over_canonical() {
        let mut td = TypeDef::test_fixture("m", StoreBackend::GithubMilestones);
        td.lifecycle = Lifecycle {
            states: vec!["backlog".into(), "shipped".into()],
            edges: vec![],
        };
        let lc = td.effective_lifecycle();
        assert_eq!(
            lc.states,
            vec!["backlog".to_string(), "shipped".to_string()]
        );
    }

    // STORY-224 AC6: filesystem/git-ref have no canonical lifecycle, so an
    // undeclared lifecycle stays empty (behaviour unchanged).
    #[test]
    fn effective_lifecycle_local_stores_stay_empty() {
        for store in [StoreBackend::Filesystem, StoreBackend::GitRef] {
            let td = TypeDef::test_fixture("s", store);
            assert!(td.effective_lifecycle().states.is_empty());
        }
    }

    // STORY-224 AC1: status membership for a github type accepts open/closed even
    // with no declared lifecycle.
    #[test]
    fn accepts_status_github_undeclared_accepts_open_closed() {
        let td = TypeDef::test_fixture("m", StoreBackend::GithubMilestones);
        assert!(td.accepts_status(&Status::new("open")));
        assert!(td.accepts_status(&Status::new("closed")));
        assert!(!td.accepts_status(&Status::new("draft")));
    }

    // Empty edges = unconstrained lifecycle: any move between declared states.
    #[test]
    fn lifecycle_with_no_edges_allows_any_transition() {
        let lc = Lifecycle {
            states: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![],
        };
        assert!(lc.has_edge("a", "c"));
        assert!(lc.has_edge("c", "a"));
        // a target outside the declared states is still rejected.
        assert!(!lc.has_edge("a", "bogus"));
        let from_a = lc.targets_from("a");
        assert_eq!(from_a, vec!["b", "c"]);
        assert!(!from_a.contains(&"a"));
    }

    // AC5 (data): a parent-child rule with `require_parent_status` parses and the
    // field is readable.
    #[test]
    fn parent_child_rule_parses_require_parent_status() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[rules]]
name = "stories-need-accepted-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "error"
require_parent_status = "accepted"
"#;
        let config = Config::parse(toml_str).unwrap();
        match &config.rules[0] {
            ValidationRule::ParentChild {
                require_parent_status,
                ..
            } => assert_eq!(require_parent_status.as_deref(), Some("accepted")),
            other => panic!("unexpected rule: {other:?}"),
        }
    }

    // AC5 (data): a parent-child rule WITHOUT the key parses with the field None.
    #[test]
    fn parent_child_rule_without_require_parent_status_is_none() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning"
"#;
        let config = Config::parse(toml_str).unwrap();
        match &config.rules[0] {
            ValidationRule::ParentChild {
                require_parent_status,
                ..
            } => assert!(require_parent_status.is_none()),
            other => panic!("unexpected rule: {other:?}"),
        }
    }

    // AC1: a [[types.attributes]] block deserializes into TypeDef.attributes,
    // and all six kinds parse (note `string` -> AttrKind::Str).
    #[test]
    fn type_attributes_deserialize_all_kinds() {
        let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[types.attributes]]
name = "estimate"
kind = "int"

[[types.attributes]]
name = "weight"
kind = "float"

[[types.attributes]]
name = "owner"
kind = "string"
required = true

[[types.attributes]]
name = "priority"
kind = "enum"
values = ["low", "high"]

[[types.attributes]]
name = "due"
kind = "date"

[[types.attributes]]
name = "blocked"
kind = "bool"
"#;
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        let attrs = &config.type_by_name("story").unwrap().attributes;
        assert_eq!(attrs.len(), 6);
        assert_eq!(attrs[0].name, "estimate");
        assert_eq!(attrs[0].kind, AttrKind::Int);
        assert_eq!(attrs[1].kind, AttrKind::Float);
        assert_eq!(attrs[2].kind, AttrKind::Str);
        assert!(attrs[2].required);
        assert_eq!(attrs[3].kind, AttrKind::Enum);
        assert_eq!(attrs[3].values, vec!["low".to_string(), "high".to_string()]);
        assert_eq!(attrs[4].kind, AttrKind::Date);
        assert_eq!(attrs[5].kind, AttrKind::Bool);
    }

    #[test]
    fn graph_config_defaults_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert_eq!(config.ui.graph.columns, vec!["status", "related"]);
        assert_eq!(config.ui.graph.sort, "path");
    }

    #[test]
    fn graph_config_parses_columns_and_sort() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[tui.graph]
columns = ["status", "estimate", "related"]
sort = "estimate"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert_eq!(
            config.ui.graph.columns,
            vec!["status", "estimate", "related"]
        );
        assert_eq!(config.ui.graph.sort, "estimate");
    }

    #[test]
    fn table_config_defaults_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert_eq!(
            config.ui.table.columns,
            vec!["status", "tags", "assignee", "provenance"]
        );
    }

    #[test]
    fn table_config_parses_columns() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[tui.table]
columns = ["status", "priority"]
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        assert_eq!(config.ui.table.columns, vec!["status", "priority"]);
    }

    #[test]
    fn graph_config_partial_block_keeps_other_default() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[tui.graph]
sort = "owner"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        // `columns` falls back to its default, `sort` is taken from the block.
        assert_eq!(config.ui.graph.columns, vec!["status", "related"]);
        assert_eq!(config.ui.graph.sort, "owner");
    }

    #[test]
    fn status_colors_parse_named_and_hex_values() {
        let toml_str = format!(
            "{TYPES}{}",
            r##"
[tui.status_colors]
draft = "magenta"
pending = "#336699"
"##
        );
        let config = Config::parse(&toml_str).unwrap();
        assert_eq!(
            config.ui.status_colors.get("draft").map(String::as_str),
            Some("magenta")
        );
        assert_eq!(
            config.ui.status_colors.get("pending").map(String::as_str),
            Some("#336699")
        );
    }

    #[test]
    fn status_colors_default_empty_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.ui.status_colors.is_empty());
    }

    #[test]
    fn type_without_attributes_defaults_empty() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.type_by_name("rfc").unwrap().attributes.is_empty());
    }

    #[test]
    fn resolve_relationship_canonical_inverse_symmetric_and_unknown() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "tracks"
inverse = "tracked-by"

[[relationships]]
name = "related-to"
"#;
        let config = Config::parse(toml_str).unwrap();

        // Canonical name resolves, not flipped.
        assert_eq!(
            config.resolve_relationship("tracks").unwrap(),
            ("tracks".to_string(), false)
        );
        // Inverse keyword resolves to the canonical name, flipped.
        assert_eq!(
            config.resolve_relationship("tracked-by").unwrap(),
            ("tracks".to_string(), true)
        );
        // Symmetric relationship resolves, not flipped, and has no inverse.
        assert_eq!(
            config.resolve_relationship("related-to").unwrap(),
            ("related-to".to_string(), false)
        );
        assert_eq!(config.inverse_of("related-to"), None);
        assert_eq!(config.inverse_of("tracks"), Some("tracked-by"));
        // Unknown keyword errors, naming it.
        let err = config.resolve_relationship("frobs").unwrap_err();
        assert!(err.to_string().contains("frobs"));
    }

    #[test]
    fn web_config_parses_owner_repo_branch() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[web]
owner = "acme"
repo = "widgets"
branch = "main"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let web = config.web.unwrap();
        assert_eq!(web.owner.as_deref(), Some("acme"));
        assert_eq!(web.repo.as_deref(), Some("widgets"));
        assert_eq!(web.branch.as_deref(), Some("main"));
    }

    #[test]
    fn web_config_partial_table_leaves_others_none() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[web]
branch = "release"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let web = config.web.unwrap();
        assert!(web.owner.is_none());
        assert!(web.repo.is_none());
        assert_eq!(web.branch.as_deref(), Some("release"));
    }

    #[test]
    fn web_config_absent_is_none() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.web.is_none());
    }

    #[test]
    fn relationship_traversal_round_trips() {
        assert_eq!(
            serde_json::to_string(&Traversal::Chain).unwrap(),
            "\"chain\""
        );
        assert_eq!(
            serde_json::to_string(&Traversal::Related).unwrap(),
            "\"related\""
        );

        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[relationships]]
name = "related-to"
traversal = "related"

[[relationships]]
name = "mentions"
"#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(
            config.relationship_by_name("implements").unwrap().traversal,
            Some(Traversal::Chain)
        );
        assert_eq!(
            config.relationship_by_name("related-to").unwrap().traversal,
            Some(Traversal::Related)
        );
        assert_eq!(
            config.relationship_by_name("mentions").unwrap().traversal,
            None
        );

        let emitted = config.to_toml().unwrap();
        assert!(emitted.contains("traversal = \"chain\""), "{emitted}");
        assert!(emitted.contains("traversal = \"related\""), "{emitted}");
        // Only the two marked relationships emit the key; `mentions` (None) is
        // skipped, so `skip_serializing_if` genuinely omits absent traversal.
        assert_eq!(
            emitted.matches("traversal =").count(),
            2,
            "skip_serializing_if must omit absent traversal: {emitted}"
        );
    }

    #[test]
    fn github_label_returns_override_verbatim_when_set() {
        let mut td = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        td.label_override = Some("Ticket".to_string());

        assert_eq!(td.github_label(), "Ticket");
    }

    #[test]
    fn github_label_falls_back_to_type_label_when_unset() {
        let td = TypeDef::test_fixture("story", StoreBackend::GithubIssues);

        assert_eq!(td.github_label(), "lazyspec:story");
    }

    #[test]
    fn github_create_labels_native_type_without_tag_is_empty() {
        let mut td = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        td.github_issue_type = Some("Bug".to_string());
        td.github_issue_tag = None;

        assert!(td.github_create_labels().is_empty());
    }

    #[test]
    fn github_create_labels_native_type_with_tag_attaches_only_tag() {
        let mut td = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        td.github_issue_type = Some("Bug".to_string());
        td.github_issue_tag = Some("bug".to_string());

        assert_eq!(td.github_create_labels(), vec!["bug".to_string()]);
    }

    // A tag alone is a classification signal: fetch filters and matches on it
    // instead of the identity label, so create must attach it instead too.
    #[test]
    fn github_create_labels_tag_without_native_type_attaches_only_tag() {
        let mut td = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        td.github_issue_type = None;
        td.github_issue_tag = Some("bug".to_string());

        assert_eq!(td.github_create_labels(), vec!["bug".to_string()]);
    }

    #[test]
    fn github_create_labels_without_any_signal_uses_identity_label() {
        let mut td = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        td.github_issue_type = None;
        td.github_issue_tag = None;

        assert_eq!(
            td.github_create_labels(),
            vec!["lazyspec:story".to_string()]
        );
    }

    #[test]
    fn toml_github_label_key_parses_into_label_override() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
github_label = "Ticket"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let td = config.type_by_name("ticket").unwrap();
        assert_eq!(td.label_override, Some("Ticket".to_string()));
    }

    #[test]
    fn toml_without_github_label_key_leaves_label_override_none() {
        let config = Config::parse(TYPES).unwrap();
        let td = config.type_by_name("rfc").unwrap();
        assert_eq!(td.label_override, None);
    }

    #[test]
    fn toml_clickup_custom_field_map_parses_and_round_trips() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[[types]]
name = "task"
plural = "tasks"
dir = "docs/tasks"
prefix = "TASK"
store = "clickup-tasks"
clickup_list_id = "list123"

[types.clickup_custom_field_map]
relations = "uuid-rel"
owner = "uuid-owner"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let td = config.type_by_name("task").unwrap();
        assert_eq!(td.clickup_field_id("relations"), Some("uuid-rel"));
        assert_eq!(td.clickup_field_id("owner"), Some("uuid-owner"));
        assert_eq!(td.clickup_field_name("uuid-rel"), Some("relations"));

        // The map survives the config_write round-trip (to_toml -> parse).
        let emitted = config.to_toml().unwrap();
        let reparsed = Config::parse(&emitted).unwrap();
        let td = reparsed.type_by_name("task").unwrap();
        assert_eq!(td.clickup_field_id("relations"), Some("uuid-rel"));
        assert_eq!(td.clickup_field_id("owner"), Some("uuid-owner"));
    }

    #[test]
    fn toml_without_clickup_custom_field_map_leaves_it_none() {
        let config = Config::parse(TYPES).unwrap();
        let td = config.type_by_name("rfc").unwrap();
        assert!(td.clickup_custom_field_map.is_none());
    }

    #[test]
    fn toml_clickup_task_type_parses_and_round_trips() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[[types]]
name = "task"
plural = "tasks"
dir = "docs/tasks"
prefix = "TASK"
store = "clickup-tasks"
clickup_list_id = "list123"
clickup_task_type = 1001
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let td = config.type_by_name("task").unwrap();
        assert_eq!(td.clickup_task_type, Some(1001));

        // Survives the config_write round-trip (to_toml -> parse) and surfaces
        // in `config --json`.
        let emitted = config.to_toml().unwrap();
        let reparsed = Config::parse(&emitted).unwrap();
        let td = reparsed.type_by_name("task").unwrap();
        assert_eq!(td.clickup_task_type, Some(1001));
        let json = serde_json::to_value(td).unwrap();
        assert_eq!(json["clickup_task_type"], serde_json::json!(1001));
    }

    #[test]
    fn toml_without_clickup_task_type_leaves_it_none() {
        let config = Config::parse(TYPES).unwrap();
        let td = config.type_by_name("rfc").unwrap();
        assert_eq!(td.clickup_task_type, None);
        let json = serde_json::to_value(td).unwrap();
        assert_eq!(json["clickup_task_type"], serde_json::Value::Null);
    }

    #[test]
    fn toml_status_authority_parses_and_round_trips() {
        let toml_str = format!(
            "{}{RELATIONSHIPS}",
            r#"
[github]
repo = "owner/repo"

[[types]]
name = "bug"
plural = "bugs"
dir = "docs/bugs"
prefix = "BUG"
store = "github-issues"
status_authority = "PROJECT-7"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let td = config.type_by_name("bug").unwrap();
        assert_eq!(td.status_authority.as_deref(), Some("PROJECT-7"));

        // `to_toml` does not emit `[github]` (the field is `serde(skip)`), so the
        // section is restored for the strict reparse.
        let emitted = config.to_toml().unwrap();
        let reparsed =
            Config::parse(&format!("{emitted}\n[github]\nrepo = \"owner/repo\"\n")).unwrap();
        let td = reparsed.type_by_name("bug").unwrap();
        assert_eq!(td.status_authority.as_deref(), Some("PROJECT-7"));
        let json = serde_json::to_value(td).unwrap();
        assert_eq!(json["status_authority"], serde_json::json!("PROJECT-7"));
    }

    #[test]
    fn toml_without_status_authority_leaves_it_none() {
        let config = Config::parse(TYPES).unwrap();
        let td = config.type_by_name("rfc").unwrap();
        assert_eq!(td.status_authority, None);
        let json = serde_json::to_value(td).unwrap();
        assert_eq!(json["status_authority"], serde_json::Value::Null);
    }

    #[test]
    fn clickup_task_type_on_non_clickup_store_is_rejected() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[[types]]
name = "task"
plural = "tasks"
dir = "docs/tasks"
prefix = "TASK"
clickup_task_type = 1001
"#
        );
        let err = Config::parse(&toml_str).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("clickup_task_type"), "got: {msg}");
        assert!(msg.contains("clickup-tasks"), "got: {msg}");
    }

    #[test]
    fn toml_github_issue_tag_and_type_keys_parse_into_type_def() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
github_issue_tag = "Bug"
github_issue_type = "Bug"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let td = config.type_by_name("ticket").unwrap();
        assert_eq!(td.github_issue_tag, Some("Bug".to_string()));
        assert_eq!(td.github_issue_type, Some("Bug".to_string()));
    }

    #[test]
    fn toml_without_github_issue_tag_and_type_keys_leaves_both_none() {
        let config = Config::parse(TYPES).unwrap();
        let td = config.type_by_name("rfc").unwrap();
        assert_eq!(td.github_issue_tag, None);
        assert_eq!(td.github_issue_type, None);
    }

    #[test]
    fn type_def_json_surfaces_github_issue_tag_and_type_as_null_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        let td = config.type_by_name("rfc").unwrap();
        let json = serde_json::to_value(td).unwrap();
        assert_eq!(json["github_issue_tag"], serde_json::Value::Null);
        assert_eq!(json["github_issue_type"], serde_json::Value::Null);
    }

    #[test]
    fn type_def_json_surfaces_github_issue_tag_and_type_when_set() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
github_issue_tag = "Bug"
github_issue_type = "Bug"
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let td = config.type_by_name("ticket").unwrap();
        let json = serde_json::to_value(td).unwrap();
        assert_eq!(
            json["github_issue_tag"],
            serde_json::Value::String("Bug".to_string())
        );
        assert_eq!(
            json["github_issue_type"],
            serde_json::Value::String("Bug".to_string())
        );
    }
}
