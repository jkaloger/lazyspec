use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

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

fn default_claim_type() -> String {
    "story".to_string()
}

fn default_agent_users() -> Vec<String> {
    Vec::new()
}

fn default_branch_template() -> String {
    "agents/{{ story_id }}".to_string()
}

fn default_workspace_root() -> PathBuf {
    PathBuf::from(".lazyspec/work")
}

fn default_base_branch() -> String {
    "origin/main".to_string()
}

fn default_claude_binary() -> String {
    "claude".to_string()
}

fn default_allowed_tools() -> String {
    String::new()
}

fn default_turn_timeout_ms() -> u64 {
    600_000
}

fn default_poll_interval_ms() -> u64 {
    30_000
}

fn default_max_concurrent_agents() -> usize {
    4
}

fn default_active_statuses() -> Vec<String> {
    vec!["accepted".to_string(), "in-progress".to_string()]
}

fn default_heartbeat_interval_ms() -> u64 {
    300_000
}

fn default_metadata_push_interval_ms() -> u64 {
    30_000
}

fn default_stall_timeout_ms() -> u64 {
    300_000
}

fn default_max_turns() -> u32 {
    20
}

fn default_max_failure_attempts() -> u32 {
    5
}

fn default_max_retry_backoff_ms() -> u64 {
    300_000
}

fn default_handoff_states() -> Vec<String> {
    vec!["review".to_string()]
}

fn default_continuation_delay_ms() -> u64 {
    1_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeConfig {
    #[serde(default = "default_claude_binary")]
    pub claude_binary: String,
    #[serde(default = "default_allowed_tools")]
    pub allowed_tools: String,
    #[serde(default = "default_turn_timeout_ms")]
    pub turn_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            claude_binary: default_claude_binary(),
            allowed_tools: default_allowed_tools(),
            turn_timeout_ms: default_turn_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HookConfig {
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrchestrationHooks {
    #[serde(default)]
    pub after_create: Option<HookConfig>,
    #[serde(default)]
    pub before_run: Option<HookConfig>,
    #[serde(default)]
    pub after_run: Option<HookConfig>,
    #[serde(default)]
    pub before_remove: Option<HookConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationConfig {
    #[serde(default = "default_agent_users")]
    pub agent_users: Vec<String>,
    #[serde(default = "default_claim_type")]
    pub claim_type: String,
    #[serde(default = "default_branch_template")]
    pub branch_template: String,
    #[serde(default = "default_workspace_root")]
    pub workspace_root: PathBuf,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub hooks: OrchestrationHooks,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_max_concurrent_agents")]
    pub max_concurrent_agents: usize,
    #[serde(default = "default_active_statuses")]
    pub active_statuses: Vec<String>,
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
    #[serde(default = "default_metadata_push_interval_ms")]
    pub metadata_push_interval_ms: u64,
    #[serde(default = "default_stall_timeout_ms")]
    pub stall_timeout_ms: u64,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_max_failure_attempts")]
    pub max_failure_attempts: u32,
    #[serde(default = "default_max_retry_backoff_ms")]
    pub max_retry_backoff_ms: u64,
    #[serde(default = "default_handoff_states")]
    pub handoff_states: Vec<String>,
    #[serde(default = "default_continuation_delay_ms")]
    pub continuation_delay_ms: u64,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentConfig {
    #[serde(skip)]
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
    pub directories: Directories,
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
    #[serde(rename = "tui")]
    pub ui: UiConfig,
    #[serde(skip)]
    pub rules: Vec<ValidationRule>,
    #[serde(skip)]
    pub ref_count_ceiling: usize,
    #[serde(default)]
    pub certification: CertificationConfig,
    #[serde(skip)]
    pub coordination: Option<CoordinationConfig>,
    #[serde(skip)]
    pub orchestration: Option<OrchestrationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directories {
    pub rfcs: String,
    pub adrs: String,
    pub stories: String,
    pub iterations: String,
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

fn default_normalize() -> bool {
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

#[derive(Deserialize)]
struct RawConfig {
    types: Option<Vec<TypeDef>>,
    rules: Option<Vec<ValidationRule>>,
    directories: Option<Directories>,
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
    orchestration: Option<OrchestrationConfig>,
}

fn build_type_def(name: &str, dir: &str, prefix: &str, icon: &str) -> TypeDef {
    let plural = match name {
        "story" => "stories".to_string(),
        _ => format!("{}s", name),
    };
    TypeDef {
        name: name.to_string(),
        plural,
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: Some(icon.to_string()),
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: StoreBackend::default(),
        singleton: false,
        parent_type: None,
    }
}

fn default_types() -> Vec<TypeDef> {
    vec![
        build_type_def("rfc", "docs/rfcs", "RFC", "●"),
        build_type_def("story", "docs/stories", "STORY", "▲"),
        build_type_def("iteration", "docs/iterations", "ITERATION", "◆"),
        build_type_def("adr", "docs/adrs", "ADR", "■"),
        build_type_def("spec", "docs/specs", "SPEC", "📋"),
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
        },
    ]
}

fn default_rules() -> Vec<ValidationRule> {
    vec![
        ValidationRule::ParentChild {
            name: "stories-need-rfcs".to_string(),
            child: "story".to_string(),
            parent: "rfc".to_string(),
            link: "implements".to_string(),
            severity: Severity::Warning,
        },
        ValidationRule::ParentChild {
            name: "iterations-need-stories".to_string(),
            child: "iteration".to_string(),
            parent: "story".to_string(),
            link: "implements".to_string(),
            severity: Severity::Error,
        },
        ValidationRule::RelationExistence {
            name: "adrs-need-relations".to_string(),
            doc_type: "adr".to_string(),
            require: "any-relation".to_string(),
            severity: Severity::Error,
        },
    ]
}

fn directories_from_types(types: &[TypeDef]) -> Directories {
    let find = |name: &str| -> String {
        types
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.dir.clone())
            .unwrap_or_default()
    };
    Directories {
        rfcs: find("rfc"),
        adrs: find("adr"),
        stories: find("story"),
        iterations: find("iteration"),
    }
}

fn types_from_directories(dirs: &Directories) -> Vec<TypeDef> {
    vec![
        build_type_def("rfc", &dirs.rfcs, "RFC", "●"),
        build_type_def("story", &dirs.stories, "STORY", "▲"),
        build_type_def("iteration", &dirs.iterations, "ITERATION", "◆"),
        build_type_def("adr", &dirs.adrs, "ADR", "■"),
    ]
}

impl Default for Config {
    fn default() -> Self {
        let types = default_types();
        let directories = directories_from_types(&types);
        Config {
            documents: DocumentConfig {
                types,
                naming: Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                },
                sqids: None,
                reserved: None,
                github: None,
            },
            filesystem: FilesystemConfig {
                directories,
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            ui: UiConfig::default(),
            rules: default_rules(),
            ref_count_ceiling: 15,
            certification: CertificationConfig::default(),
            coordination: None,
            orchestration: None,
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
        let raw: RawConfig = toml::from_str(toml_str)?;

        let types = if let Some(types) = raw.types {
            types
        } else if let Some(ref dirs) = raw.directories {
            types_from_directories(dirs)
        } else {
            default_types()
        };

        let directories = if let Some(dirs) = raw.directories {
            dirs
        } else {
            directories_from_types(&types)
        };

        let rules = raw.rules.unwrap_or_else(default_rules);

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
                directories,
                templates: raw.templates.unwrap_or(Templates {
                    dir: ".lazyspec/templates".to_string(),
                }),
            },
            ui: raw.tui.unwrap_or_default(),
            rules,
            ref_count_ceiling,
            certification: raw.certification.unwrap_or_default(),
            coordination: raw.coordination,
            orchestration: raw.orchestration,
        })
    }

    pub fn load(
        project_root: &std::path::Path,
        fs: &dyn crate::engine::fs::FileSystem,
    ) -> Result<Self> {
        let path = project_root.join(".lazyspec.toml");
        if fs.exists(&path) {
            let content = fs.read_to_string(&path)?;
            return Self::parse(&content);
        }
        Ok(Self::default())
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn type_by_name(&self, name: &str) -> Option<&TypeDef> {
        self.documents.types.iter().find(|t| t.name == name)
    }
}

impl TypeDef {
    pub fn make_id(&self, suffix: impl std::fmt::Display) -> String {
        format!("{}-{}", self.prefix, suffix)
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_backend_display() {
        assert_eq!(StoreBackend::Filesystem.to_string(), "filesystem");
        assert_eq!(StoreBackend::GithubIssues.to_string(), "github-issues");
    }

    #[test]
    fn test_certification_default_when_absent() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(config.certification.normalize);
        assert!(config.certification.overrides.is_empty());
    }

    #[test]
    fn test_certification_explicit_true() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[certification]
normalize = true
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(config.certification.normalize);
    }

    #[test]
    fn test_certification_explicit_false() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[certification]
normalize = false
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(!config.certification.normalize);
    }

    #[test]
    fn test_certification_override_disables_normalize() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[certification]
normalize = true

[certification.overrides."docs/specs/SPEC-007"]
normalize = false
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(!config.certification.should_normalize("docs/specs/SPEC-007"));
    }

    #[test]
    fn test_certification_override_does_not_affect_other_specs() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[certification]
normalize = true

[certification.overrides."docs/specs/SPEC-007"]
normalize = false
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(config.certification.should_normalize("docs/specs/SPEC-008"));
    }

    #[test]
    fn test_should_normalize_falls_back_to_global() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[certification]
normalize = false
"#;
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::Filesystem);
        assert_eq!(config.documents.types[1].store, StoreBackend::GithubIssues);
    }

    #[test]
    fn test_github_config_defaults() {
        let toml_str = r#"
[github]
repo = "owner/repo"
"#;
        let config = Config::parse(toml_str).unwrap();
        let gh = config.documents.github.unwrap();
        assert_eq!(gh.repo.as_deref(), Some("owner/repo"));
        assert_eq!(gh.cache_ttl, 60);
    }

    #[test]
    fn test_github_config_custom_cache_ttl() {
        let toml_str = r#"
[github]
repo = "owner/repo"
cache_ttl = 120
"#;
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
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
        let err = Config::parse(toml_str).unwrap_err();
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
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
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
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[coordination]
remote = "upstream"
lease_duration = "30m"
grace_period = "5m"
max_push_retries = 10
"#;
        let config = Config::parse(toml_str).unwrap();
        let coord = config.coordination.unwrap();
        assert_eq!(coord.remote, "upstream");
        assert_eq!(coord.lease_duration, "30m");
        assert_eq!(coord.grace_period, "5m");
        assert_eq!(coord.max_push_retries, 10);
    }

    #[test]
    fn test_coordination_defaults_when_empty_section() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[coordination]
"#;
        let config = Config::parse(toml_str).unwrap();
        let coord = config.coordination.unwrap();
        assert_eq!(coord.remote, "origin");
        assert_eq!(coord.lease_duration, "60m");
        assert_eq!(coord.grace_period, "2m");
        assert_eq!(coord.max_push_retries, 5);
    }

    #[test]
    fn test_coordination_none_when_absent() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(config.coordination.is_none());
    }

    #[test]
    fn orchestration_defaults_when_section_absent() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;
        let config = Config::parse(toml_str).unwrap();
        assert!(config.orchestration.is_none());
    }

    #[test]
    fn orchestration_defaults_when_empty_section() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert!(orch.agent_users.is_empty());
        assert_eq!(orch.claim_type, "story");
        assert_eq!(orch.branch_template, "agents/{{ story_id }}");
        assert_eq!(orch.workspace_root, PathBuf::from(".lazyspec/work"));
        assert_eq!(orch.base_branch, "origin/main");
        assert_eq!(orch.poll_interval_ms, 30_000);
        assert_eq!(orch.max_concurrent_agents, 4);
        assert_eq!(orch.active_statuses, vec!["accepted", "in-progress"]);
        assert_eq!(orch.heartbeat_interval_ms, 300_000);
        assert_eq!(orch.metadata_push_interval_ms, 30_000);
        assert_eq!(orch.stall_timeout_ms, 300_000);
        assert_eq!(orch.max_turns, 20);
        assert_eq!(orch.max_failure_attempts, 5);
        assert_eq!(orch.max_retry_backoff_ms, 300_000);
        assert_eq!(orch.handoff_states, vec!["review"]);
        assert_eq!(orch.continuation_delay_ms, 1_000);
    }

    #[test]
    fn orchestration_reconcile_retry_explicit_values_roundtrip() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
stall_timeout_ms = 60000
max_turns = 50
max_failure_attempts = 3
max_retry_backoff_ms = 120000
handoff_states = ["in-review", "needs-human"]
continuation_delay_ms = 500
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.stall_timeout_ms, 60_000);
        assert_eq!(orch.max_turns, 50);
        assert_eq!(orch.max_failure_attempts, 3);
        assert_eq!(orch.max_retry_backoff_ms, 120_000);
        assert_eq!(orch.handoff_states, vec!["in-review", "needs-human"]);
        assert_eq!(orch.continuation_delay_ms, 500);
    }

    #[test]
    fn orchestration_tick_loop_explicit_values_roundtrip() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
poll_interval_ms = 5000
max_concurrent_agents = 8
active_statuses = ["todo", "review"]
heartbeat_interval_ms = 60000
metadata_push_interval_ms = 10000
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.poll_interval_ms, 5_000);
        assert_eq!(orch.max_concurrent_agents, 8);
        assert_eq!(orch.active_statuses, vec!["todo", "review"]);
        assert_eq!(orch.heartbeat_interval_ms, 60_000);
        assert_eq!(orch.metadata_push_interval_ms, 10_000);
    }

    #[test]
    fn orchestration_uses_explicit_values() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
agent_users = ["claude-bot", "other-bot"]
claim_type = "iteration"
branch_template = "bots/{{ iteration_id }}"
workspace_root = "/tmp/lazyspec-work"
base_branch = "origin/develop"
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.agent_users, vec!["claude-bot", "other-bot"]);
        assert_eq!(orch.claim_type, "iteration");
        assert_eq!(orch.branch_template, "bots/{{ iteration_id }}");
        assert_eq!(orch.workspace_root, PathBuf::from("/tmp/lazyspec-work"));
        assert_eq!(orch.base_branch, "origin/develop");
    }

    #[test]
    fn orchestration_partial_section_falls_back_to_default_claim_type() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
agent_users = ["claude-bot"]
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.agent_users, vec!["claude-bot"]);
        assert_eq!(orch.claim_type, "story");
    }

    #[test]
    fn runtime_defaults_when_section_absent() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.runtime.claude_binary, "claude");
        assert_eq!(orch.runtime.allowed_tools, "");
        assert_eq!(orch.runtime.turn_timeout_ms, 600_000);
    }

    #[test]
    fn runtime_uses_explicit_values() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration.runtime]
claude_binary = "/usr/local/bin/claude"
allowed_tools = "Read,Edit,Bash"
turn_timeout_ms = 120000
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.runtime.claude_binary, "/usr/local/bin/claude");
        assert_eq!(orch.runtime.allowed_tools, "Read,Edit,Bash");
        assert_eq!(orch.runtime.turn_timeout_ms, 120_000);
    }

    #[test]
    fn hooks_default_when_section_absent() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration]
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert!(orch.hooks.after_create.is_none());
        assert!(orch.hooks.before_run.is_none());
        assert!(orch.hooks.after_run.is_none());
        assert!(orch.hooks.before_remove.is_none());
    }

    #[test]
    fn hooks_partial_section_loads_one_hook() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration.hooks.after_create]
script = "scripts/after-create.sh"
"#;
        let config = Config::parse(toml_str).unwrap();
        let hooks = config.orchestration.unwrap().hooks;
        let after = hooks.after_create.unwrap();
        assert_eq!(after.script.as_deref(), Some("scripts/after-create.sh"));
        assert!(after.timeout_ms.is_none());
        assert!(hooks.before_run.is_none());
        assert!(hooks.after_run.is_none());
        assert!(hooks.before_remove.is_none());
    }

    #[test]
    fn hooks_all_four_points_load() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration.hooks.after_create]
script = "ac.sh"

[orchestration.hooks.before_run]
script = "br.sh"

[orchestration.hooks.after_run]
script = "ar.sh"

[orchestration.hooks.before_remove]
script = "brm.sh"
"#;
        let config = Config::parse(toml_str).unwrap();
        let hooks = config.orchestration.unwrap().hooks;
        assert_eq!(hooks.after_create.unwrap().script.as_deref(), Some("ac.sh"));
        assert_eq!(hooks.before_run.unwrap().script.as_deref(), Some("br.sh"));
        assert_eq!(hooks.after_run.unwrap().script.as_deref(), Some("ar.sh"));
        assert_eq!(
            hooks.before_remove.unwrap().script.as_deref(),
            Some("brm.sh")
        );
    }

    #[test]
    fn hooks_timeout_ms_loads() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration.hooks.after_create]
script = "ac.sh"
timeout_ms = 1000
"#;
        let config = Config::parse(toml_str).unwrap();
        let after = config.orchestration.unwrap().hooks.after_create.unwrap();
        assert_eq!(after.timeout_ms, Some(1000));
    }

    #[test]
    fn hooks_toml_roundtrip() {
        let hooks = OrchestrationHooks {
            after_create: Some(HookConfig {
                script: Some("ac.sh".to_string()),
                timeout_ms: Some(5000),
            }),
            before_run: Some(HookConfig {
                script: Some("br.sh".to_string()),
                timeout_ms: None,
            }),
            after_run: None,
            before_remove: Some(HookConfig {
                script: None,
                timeout_ms: Some(2000),
            }),
        };
        let serialized = toml::to_string(&hooks).unwrap();
        let parsed: OrchestrationHooks = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed, hooks);
    }

    #[test]
    fn runtime_partial_section_falls_back_to_defaults() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[orchestration.runtime]
claude_binary = "/opt/claude"
"#;
        let config = Config::parse(toml_str).unwrap();
        let orch = config.orchestration.unwrap();
        assert_eq!(orch.runtime.claude_binary, "/opt/claude");
        assert_eq!(orch.runtime.allowed_tools, "");
        assert_eq!(orch.runtime.turn_timeout_ms, 600_000);
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
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::GitRef);
    }

    #[test]
    fn test_multiline_config_defaults() {
        let cfg = MultiLineConfig::default();
        assert_eq!(cfg.max_expanded_height, 5);
    }

    #[test]
    fn test_multiline_config_parses_max_expanded_height() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[tui.multiline]
max_expanded_height = 3
"#;
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(config.ui.multiline.max_expanded_height, 3);
    }

    #[test]
    fn test_multiline_config_defaults_when_section_absent() {
        let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;
        let config = Config::parse(toml_str).unwrap();
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
        let config = Config::parse(toml_str).unwrap();
        assert_eq!(config.documents.types[0].store, StoreBackend::GitRef);
        assert_eq!(config.documents.types[1].store, StoreBackend::GithubIssues);
        assert_eq!(config.documents.types[2].store, StoreBackend::Filesystem);
    }
}
