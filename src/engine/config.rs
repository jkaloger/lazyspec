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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum StoreBackend {
    #[default]
    #[serde(rename = "filesystem")]
    Filesystem,
    #[serde(rename = "github-issues")]
    GithubIssues,
}

impl fmt::Display for StoreBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreBackend::Filesystem => write!(f, "filesystem"),
            StoreBackend::GithubIssues => write!(f, "github-issues"),
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    pub ascii_diagrams: bool,
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
}
