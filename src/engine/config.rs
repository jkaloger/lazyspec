use anyhow::Result;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub plural: String,
    pub dir: String,
    pub prefix: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub types: Vec<TypeDef>,
    #[serde(skip)]
    pub rules: Vec<ValidationRule>,
    pub directories: Directories,
    pub templates: Templates,
    pub naming: Naming,
    pub tui: Tui,
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
pub struct Tui {
    pub ascii_diagrams: bool,
}

impl Default for Tui {
    fn default() -> Self {
        Tui { ascii_diagrams: false }
    }
}

#[derive(Deserialize)]
struct RawConfig {
    types: Option<Vec<TypeDef>>,
    rules: Option<Vec<ValidationRule>>,
    directories: Option<Directories>,
    templates: Option<Templates>,
    naming: Option<Naming>,
    tui: Option<Tui>,
}

fn default_types() -> Vec<TypeDef> {
    vec![
        TypeDef {
            name: "rfc".to_string(),
            plural: "rfcs".to_string(),
            dir: "docs/rfcs".to_string(),
            prefix: "RFC".to_string(),
            icon: Some("●".to_string()),
        },
        TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: "docs/stories".to_string(),
            prefix: "STORY".to_string(),
            icon: Some("▲".to_string()),
        },
        TypeDef {
            name: "iteration".to_string(),
            plural: "iterations".to_string(),
            dir: "docs/iterations".to_string(),
            prefix: "ITERATION".to_string(),
            icon: Some("◆".to_string()),
        },
        TypeDef {
            name: "adr".to_string(),
            plural: "adrs".to_string(),
            dir: "docs/adrs".to_string(),
            prefix: "ADR".to_string(),
            icon: Some("■".to_string()),
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
        TypeDef {
            name: "rfc".to_string(),
            plural: "rfcs".to_string(),
            dir: dirs.rfcs.clone(),
            prefix: "RFC".to_string(),
            icon: Some("●".to_string()),
        },
        TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: dirs.stories.clone(),
            prefix: "STORY".to_string(),
            icon: Some("▲".to_string()),
        },
        TypeDef {
            name: "iteration".to_string(),
            plural: "iterations".to_string(),
            dir: dirs.iterations.clone(),
            prefix: "ITERATION".to_string(),
            icon: Some("◆".to_string()),
        },
        TypeDef {
            name: "adr".to_string(),
            plural: "adrs".to_string(),
            dir: dirs.adrs.clone(),
            prefix: "ADR".to_string(),
            icon: Some("■".to_string()),
        },
    ]
}

impl Default for Config {
    fn default() -> Self {
        let types = default_types();
        let directories = directories_from_types(&types);
        Config {
            types,
            rules: default_rules(),
            directories,
            templates: Templates {
                dir: ".lazyspec/templates".to_string(),
            },
            naming: Naming {
                pattern: "{type}-{n:03}-{title}.md".to_string(),
            },
            tui: Tui::default(),
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

        Ok(Config {
            types,
            rules,
            directories,
            templates: raw.templates.unwrap_or(Templates {
                dir: ".lazyspec/templates".to_string(),
            }),
            naming: raw.naming.unwrap_or(Naming {
                pattern: "{type}-{n:03}-{title}.md".to_string(),
            }),
            tui: raw.tui.unwrap_or_default(),
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
        self.types.iter().find(|t| t.name == name)
    }
}
