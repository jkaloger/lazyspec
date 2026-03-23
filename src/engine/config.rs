use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub plural: String,
    pub dir: String,
    pub prefix: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub numbering: NumberingStrategy,
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

#[derive(Deserialize)]
struct RawNumbering {
    sqids: Option<SqidsConfig>,
    reserved: Option<ReservedConfig>,
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
    }
}

fn default_types() -> Vec<TypeDef> {
    vec![
        build_type_def("rfc", "docs/rfcs", "RFC", "●"),
        build_type_def("story", "docs/stories", "STORY", "▲"),
        build_type_def("iteration", "docs/iterations", "ITERATION", "◆"),
        build_type_def("adr", "docs/adrs", "ADR", "■"),
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
            },
            filesystem: FilesystemConfig {
                directories,
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            ui: UiConfig::default(),
            rules: default_rules(),
        }
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

        Ok(Config {
            documents: DocumentConfig {
                types,
                naming: raw.naming.unwrap_or(Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                }),
                sqids,
                reserved,
            },
            filesystem: FilesystemConfig {
                directories,
                templates: raw.templates.unwrap_or(Templates {
                    dir: ".lazyspec/templates".to_string(),
                }),
            },
            ui: raw.tui.unwrap_or_default(),
            rules,
        })
    }

    pub fn load(project_root: &std::path::Path) -> Result<Self> {
        let path = project_root.join(".lazyspec.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
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
