use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub directories: Directories,
    pub templates: Templates,
    pub naming: Naming,
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

impl Default for Config {
    fn default() -> Self {
        Config {
            directories: Directories {
                rfcs: "docs/rfcs".to_string(),
                adrs: "docs/adrs".to_string(),
                stories: "docs/stories".to_string(),
                iterations: "docs/iterations".to_string(),
            },
            templates: Templates {
                dir: ".lazyspec/templates".to_string(),
            },
            naming: Naming {
                pattern: "{type}-{n:03}-{title}.md".to_string(),
            },
        }
    }
}

impl Config {
    pub fn parse(toml_str: &str) -> Result<Self> {
        let config: Config = toml::from_str(toml_str)?;
        Ok(config)
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
}
