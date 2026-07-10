use crate::engine::document::Status;
use anyhow::{bail, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

fn default_coordination_remote() -> String {
    "origin".to_string()
}

fn default_coordination_lease_duration() -> String {
    "60m".to_string()
}

fn default_coordination_grace_period() -> String {
    "2m".to_string()
}

fn default_coordination_max_push_retries() -> u8 {
    5
}

fn default_coordination_max_clock_skew() -> String {
    "5m".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CoordinationConfig {
    #[serde(default = "default_coordination_remote")]
    pub remote: String,
    #[serde(default = "default_coordination_lease_duration")]
    pub lease_duration: String,
    #[serde(default = "default_coordination_grace_period")]
    pub grace_period: String,
    #[serde(default = "default_coordination_max_push_retries")]
    pub max_push_retries: u8,
    #[serde(default = "default_coordination_max_clock_skew")]
    pub max_clock_skew: String,
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
    /// from `label_override`'s identity label. Schema only for now -- no
    /// resolver or matching logic reads this field yet.
    #[serde(default)]
    pub github_issue_tag: Option<String>,
    /// A GitHub native issue type naming this type as a classification
    /// signal. Schema only for now -- no resolver, discovery, or write logic
    /// reads this field yet.
    #[serde(default)]
    pub github_issue_type: Option<String>,
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
        type_def.lifecycle.states.join(", ")
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
    #[serde(skip)]
    pub coordination: Option<CoordinationConfig>,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    /// The optional `[web]` repo-coordinate overrides (RFC-052). `None` when the
    /// table is absent.
    #[serde(skip)]
    pub web: Option<WebConfig>,
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
    /// rendering, graph columns, ASCII diagrams).
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
    /// The `[coordination]` block: git-remote lease settings for distributed
    /// task coordination (remote, lease/grace durations, retries, clock skew).
    #[serde(default)]
    coordination: Option<CoordinationConfig>,
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
            coordination: None,
            agents: AgentsConfig::default(),
            skills: SkillsConfig::default(),
            web: None,
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
        let raw: RawConfig = toml::from_str(toml_str)?;

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
            coordination: raw.coordination,
            agents: raw.agents.unwrap_or_default(),
            skills: raw.skills.unwrap_or_default(),
            web: raw.web,
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

    /// True iff `status` names one of this type's declared lifecycle states.
    pub fn accepts_status(&self, status: &Status) -> bool {
        self.lifecycle.states.iter().any(|s| s == status.as_str())
    }

    /// The GitHub label identifying this type's issues: the configured
    /// `label_override` if set, else the default `lazyspec:{name}`.
    pub fn github_label(&self) -> String {
        self.label_override
            .clone()
            .unwrap_or_else(|| crate::engine::gh::type_label(&self.name))
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
    fn test_coordination_explicit_values() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[coordination]
remote = "upstream"
lease_duration = "30m"
grace_period = "5m"
max_push_retries = 10
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let coord = config.coordination.unwrap();
        assert_eq!(coord.remote, "upstream");
        assert_eq!(coord.lease_duration, "30m");
        assert_eq!(coord.grace_period, "5m");
        assert_eq!(coord.max_push_retries, 10);
    }

    #[test]
    fn test_coordination_defaults_when_empty_section() {
        let toml_str = format!(
            "{TYPES}{}",
            r#"
[coordination]
"#
        );
        let config = Config::parse(&toml_str).unwrap();
        let coord = config.coordination.unwrap();
        assert_eq!(coord.remote, "origin");
        assert_eq!(coord.lease_duration, "60m");
        assert_eq!(coord.grace_period, "2m");
        assert_eq!(coord.max_push_retries, 5);
    }

    #[test]
    fn test_coordination_none_when_absent() {
        let config = Config::parse(TYPES).unwrap();
        assert!(config.coordination.is_none());
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
