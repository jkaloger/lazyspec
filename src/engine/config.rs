use crate::engine::document::Status;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
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
    #[serde(default)]
    pub requires_priority: Option<bool>,
    #[serde(default)]
    pub terminal_statuses: Option<Vec<Status>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriorityDef {
    pub weight: u32,
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
    #[serde(skip)]
    pub priorities: BTreeMap<String, u32>,
}

fn default_priorities() -> BTreeMap<String, u32> {
    let mut m = BTreeMap::new();
    m.insert("must".to_string(), 4);
    m.insert("should".to_string(), 3);
    m.insert("could".to_string(), 2);
    m.insert("wont".to_string(), 1);
    m
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub ascii_diagrams: bool,
    #[serde(default)]
    pub statusbar: StatusBarConfig,
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
    priorities: Option<HashMap<String, PriorityDef>>,
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
        requires_priority: None,
        terminal_statuses: None,
    }
}

fn default_types() -> Vec<TypeDef> {
    vec![
        build_type_def("rfc", "docs/rfcs", "RFC", "●"),
        build_type_def("story", "docs/stories", "STORY", "▲"),
        build_type_def("iteration", "docs/iterations", "ITERATION", "◆"),
        build_type_def("adr", "docs/adrs", "ADR", "■"),
        build_type_def("spec", "docs/specs", "SPEC", "📋"),
        build_type_def("audit", "docs/audits", "AUDIT", "✓"),
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
            requires_priority: None,
            terminal_statuses: None,
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
            requires_priority: None,
            terminal_statuses: None,
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
                priorities: default_priorities(),
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

        let priorities = raw
            .priorities
            .map(|m| m.into_iter().map(|(k, v)| (k, v.weight)).collect())
            .unwrap_or_else(default_priorities);

        Ok(Config {
            documents: DocumentConfig {
                types,
                naming: raw.naming.unwrap_or(Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                }),
                sqids,
                reserved,
                github: raw.github,
                priorities,
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

    pub fn priority_weights(&self) -> &BTreeMap<String, u32> {
        &self.documents.priorities
    }
}

fn default_requires_priority(type_name: &str) -> bool {
    matches!(type_name, "story" | "iteration")
}

fn default_terminal_statuses(type_name: &str) -> Vec<Status> {
    match type_name {
        "rfc" | "story" => vec![Status::Complete, Status::Superseded, Status::Rejected],
        "iteration" | "audit" => vec![Status::Complete],
        "adr" | "convention" | "dictum" => vec![Status::Accepted, Status::Superseded],
        _ => vec![],
    }
}

impl TypeDef {
    pub fn make_id(&self, suffix: impl std::fmt::Display) -> String {
        format!("{}-{}", self.prefix, suffix)
    }

    pub fn resolved_requires_priority(&self) -> bool {
        self.requires_priority
            .unwrap_or_else(|| default_requires_priority(&self.name))
    }

    pub fn resolved_terminal_statuses(&self) -> Vec<Status> {
        self.terminal_statuses
            .clone()
            .unwrap_or_else(|| default_terminal_statuses(&self.name))
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
            requires_priority: None,
            terminal_statuses: None,
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
    fn test_priority_weights_custom_replaces_default() {
        let toml_str = r#"
[priorities.high]
weight = 10

[priorities.low]
weight = 1
"#;
        let config = Config::parse(toml_str).unwrap();
        let weights = config.priority_weights();
        let keys: std::collections::BTreeSet<&str> =
            weights.keys().map(|s| s.as_str()).collect();
        let expected_keys: std::collections::BTreeSet<&str> =
            ["high", "low"].into_iter().collect();
        assert_eq!(keys, expected_keys);
        assert!(!weights.contains_key("must"));
        assert!(!weights.contains_key("should"));
        assert!(!weights.contains_key("could"));
        assert!(!weights.contains_key("wont"));
    }

    #[test]
    fn test_priority_weights_returns_parsed_map() {
        let toml_str = r#"
[priorities.critical]
weight = 100

[priorities.normal]
weight = 50

[priorities.minor]
weight = 10
"#;
        let config = Config::parse(toml_str).unwrap();
        let weights = config.priority_weights();
        let expected: BTreeMap<String, u32> = [
            ("critical".to_string(), 100),
            ("normal".to_string(), 50),
            ("minor".to_string(), 10),
        ]
        .into_iter()
        .collect();
        assert_eq!(weights, &expected);
    }

    #[test]
    fn test_priority_weights_default_when_absent() {
        let config = Config::parse("").unwrap();
        let weights = config.priority_weights();
        let expected: BTreeMap<String, u32> = [
            ("must".to_string(), 4),
            ("should".to_string(), 3),
            ("could".to_string(), 2),
            ("wont".to_string(), 1),
        ]
        .into_iter()
        .collect();
        assert_eq!(weights, &expected);
    }

    fn sorted_statuses(v: Vec<crate::engine::document::Status>) -> Vec<String> {
        let mut s: Vec<String> = v.into_iter().map(|x| x.to_string()).collect();
        s.sort();
        s
    }

    fn sorted_strs(v: &[&str]) -> Vec<String> {
        let mut s: Vec<String> = v.iter().map(|x| x.to_string()).collect();
        s.sort();
        s
    }

    #[test]
    fn ac7_default_terminal_statuses_rfc() {
        let cfg = Config::default();
        let td = cfg.type_by_name("rfc").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["complete", "superseded", "rejected"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_story() {
        let cfg = Config::default();
        let td = cfg.type_by_name("story").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["complete", "superseded", "rejected"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_iteration() {
        let cfg = Config::default();
        let td = cfg.type_by_name("iteration").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["complete"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_audit() {
        let cfg = Config::default();
        let td = cfg.type_by_name("audit").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["complete"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_adr() {
        let cfg = Config::default();
        let td = cfg.type_by_name("adr").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["accepted", "superseded"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_convention() {
        let cfg = Config::default();
        let td = cfg.type_by_name("convention").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["accepted", "superseded"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_dictum() {
        let cfg = Config::default();
        let td = cfg.type_by_name("dictum").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["accepted", "superseded"])
        );
    }

    #[test]
    fn ac7_default_terminal_statuses_unknown_type_empty() {
        let td = build_type_def("note", "docs/notes", "NOTE", "📝");
        assert!(td.resolved_terminal_statuses().is_empty());
    }

    #[test]
    fn ac8_terminal_statuses_override_replaces_default_no_merge() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
terminal_statuses = ["accepted"]
"#;
        let cfg = Config::parse(toml_str).unwrap();
        let td = cfg.type_by_name("rfc").unwrap();
        assert_eq!(
            sorted_statuses(td.resolved_terminal_statuses()),
            sorted_strs(&["accepted"])
        );
    }

    #[test]
    fn ac8b_partial_override_other_types_keep_defaults() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
terminal_statuses = ["accepted"]

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
"#;
        let cfg = Config::parse(toml_str).unwrap();
        let story = cfg.type_by_name("story").unwrap();
        assert_eq!(
            sorted_statuses(story.resolved_terminal_statuses()),
            sorted_strs(&["complete", "superseded", "rejected"])
        );
    }

    #[test]
    fn requires_priority_defaults_story_true() {
        let cfg = Config::default();
        assert!(cfg.type_by_name("story").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_iteration_true() {
        let cfg = Config::default();
        assert!(cfg.type_by_name("iteration").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_rfc_false() {
        let cfg = Config::default();
        assert!(!cfg.type_by_name("rfc").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_adr_false() {
        let cfg = Config::default();
        assert!(!cfg.type_by_name("adr").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_audit_false() {
        let cfg = Config::default();
        assert!(!cfg.type_by_name("audit").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_convention_false() {
        let cfg = Config::default();
        assert!(!cfg.type_by_name("convention").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_dictum_false() {
        let cfg = Config::default();
        assert!(!cfg.type_by_name("dictum").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_defaults_spec_false() {
        let cfg = Config::default();
        assert!(!cfg.type_by_name("spec").unwrap().resolved_requires_priority());
    }

    #[test]
    fn requires_priority_override_rfc_true() {
        let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
requires_priority = true
"#;
        let cfg = Config::parse(toml_str).unwrap();
        assert!(cfg.type_by_name("rfc").unwrap().resolved_requires_priority());
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
