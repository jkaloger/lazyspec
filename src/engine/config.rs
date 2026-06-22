use crate::engine::document::Status;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "shape")]
pub enum ValidationRule {
    #[serde(rename = "parent-child")]
    ParentChild {
        name: String,
        child: String,
        parent: String,
        link: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NumberingStrategy {
    #[default]
    Incremental,
    Sqids,
    Reserved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SqidsConfig {
    pub salt: String,
    #[serde(default = "default_sqids_min_length")]
    pub min_length: u8,
}

fn default_sqids_min_length() -> u8 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReservedFormat {
    Incremental,
    Sqids,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum StoreBackend {
    #[default]
    #[serde(rename = "filesystem")]
    Filesystem,
    #[serde(rename = "github-issues")]
    GithubIssues,
    #[serde(rename = "git-ref")]
    GitRef,
}

impl fmt::Display for StoreBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreBackend::Filesystem => write!(f, "filesystem"),
            StoreBackend::GithubIssues => write!(f, "github-issues"),
            StoreBackend::GitRef => write!(f, "git-ref"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Authorship {
    Human,
    #[default]
    Assisted,
    Generated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Lifecycle {
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

impl Lifecycle {
    /// True iff a `from -> to` transition is declared. A `*` edge source matches
    /// any `from`, so `* -> rejected` permits the move from any state.
    pub fn has_edge(&self, from: &str, to: &str) -> bool {
        self.edges
            .iter()
            .any(|e| (e.from == from || e.from == "*") && e.to == to)
    }

    /// The set of states reachable from `from` in a single declared edge,
    /// including wildcard targets. Used to report the allowed moves when a
    /// transition is rejected.
    pub fn targets_from(&self, from: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.from == from || e.from == "*")
            .map(|e| e.to.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub plural: String,
    pub dir: String,
    pub prefix: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub numbering: NumberingStrategy,
    #[serde(default)]
    pub subdirectory: bool,
    #[serde(default)]
    pub store: StoreBackend,
    #[serde(default)]
    pub singleton: bool,
    #[serde(default)]
    pub parent_type: Option<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub authorship: Authorship,
    #[serde(default)]
    pub lifecycle: Lifecycle,
}

/// One entry in the `[[relationships]]` block: a relationship name and its
/// optional inverse keyword. A relationship with no `inverse` is symmetric
/// (e.g. `related-to`); a directional one declares its inverse (e.g.
/// `implements` / `implemented-by`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationshipDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
}

/// The canonical starter relationship vocabulary, mirroring the closed enum that
/// preceded the config registry. Used by `init`'s `starter_config`, the
/// `to_toml` writer, and the test-only `Config::default()`. The load path
/// carries none (ADR-011): a real config must declare `[[relationships]]`.
pub fn starter_relationships() -> Vec<RelationshipDef> {
    let directional = |name: &str, inverse: &str| RelationshipDef {
        name: name.to_string(),
        inverse: Some(inverse.to_string()),
    };
    vec![
        directional("implements", "implemented-by"),
        directional("supersedes", "superseded-by"),
        directional("blocks", "blocked-by"),
        RelationshipDef {
            name: "related-to".to_string(),
            inverse: None,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub ascii_diagrams: bool,
    #[serde(default)]
    pub statusbar: StatusBarConfig,
    #[serde(default)]
    pub multiline: MultiLineConfig,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Templates {
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Naming {
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Deserialize)]
struct RawNumbering {
    sqids: Option<SqidsConfig>,
    reserved: Option<ReservedConfig>,
}

fn default_cache_ttl() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GithubConfig {
    pub repo: Option<String>,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: u64,
}

/// The global `[agents]` block. `interactive` is the optional `bash -lc` shell
/// command for terminal handover (e.g. `claude "$LAZYSPEC_PROMPT"`). Zero-defaults
/// (ADR-015): absent -> None -> interactive run mode is unavailable and not offered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentsConfig {
    #[serde(default)]
    pub interactive: Option<String>,
}

#[derive(Deserialize)]
struct RawConfig {
    types: Option<Vec<TypeDef>>,
    relationships: Option<Vec<RelationshipDef>>,
    rules: Option<Vec<ValidationRule>>,
    templates: Option<Templates>,
    naming: Option<Naming>,
    tui: Option<UiConfig>,
    numbering: Option<RawNumbering>,
    #[serde(default)]
    ref_count_ceiling: Option<usize>,
    #[serde(default)]
    certification: Option<CertificationConfig>,
    github: Option<GithubConfig>,
    #[serde(default)]
    coordination: Option<CoordinationConfig>,
    #[serde(default)]
    agents: Option<AgentsConfig>,
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
            link: "implements".to_string(),
            severity: Severity::Warning,
            require_parent_status: None,
        },
        ValidationRule::ParentChild {
            name: "iterations-need-stories".to_string(),
            child: "iteration".to_string(),
            parent: "story".to_string(),
            link: "implements".to_string(),
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

        let any_github_issues = types.iter().any(|t| t.store == StoreBackend::GithubIssues);
        if any_github_issues && raw.github.is_none() {
            bail!("store = \"github-issues\" requires a [github] section");
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
link = "implements"
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
link = "implements"
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
}
