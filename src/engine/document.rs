use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DocType(String);

impl DocType {
    pub const RFC: &str = "rfc";
    pub const STORY: &str = "story";
    pub const ITERATION: &str = "iteration";
    pub const ADR: &str = "adr";

    pub fn new(s: &str) -> Self {
        DocType(s.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DocType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for DocType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(DocType(s.to_lowercase()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Review,
    Accepted,
    Rejected,
    Superseded,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Draft => write!(f, "draft"),
            Status::Review => write!(f, "review"),
            Status::Accepted => write!(f, "accepted"),
            Status::Rejected => write!(f, "rejected"),
            Status::Superseded => write!(f, "superseded"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationType {
    Implements,
    Supersedes,
    Blocks,
    RelatedTo,
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelationType::Implements => write!(f, "implements"),
            RelationType::Supersedes => write!(f, "supersedes"),
            RelationType::Blocks => write!(f, "blocks"),
            RelationType::RelatedTo => write!(f, "related to"),
        }
    }
}

impl std::str::FromStr for DocType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DocType::new(s))
    }
}

impl std::str::FromStr for Status {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Status::Draft),
            "review" => Ok(Status::Review),
            "accepted" => Ok(Status::Accepted),
            "rejected" => Ok(Status::Rejected),
            "superseded" => Ok(Status::Superseded),
            _ => Err(anyhow!("unknown status: {}", s)),
        }
    }
}

impl std::str::FromStr for RelationType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "implements" => Ok(RelationType::Implements),
            "supersedes" => Ok(RelationType::Supersedes),
            "blocks" => Ok(RelationType::Blocks),
            "related-to" | "related to" => Ok(RelationType::RelatedTo),
            _ => Err(anyhow!("unknown relation type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub rel_type: RelationType,
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct DocMeta {
    pub path: PathBuf,
    pub title: String,
    pub doc_type: DocType,
    pub status: Status,
    pub author: String,
    pub date: NaiveDate,
    pub tags: Vec<String>,
    pub related: Vec<Relation>,
    pub validate_ignore: bool,
    pub virtual_doc: bool,
    pub id: String,
}

#[derive(Deserialize)]
struct RawFrontmatter {
    title: String,
    #[serde(rename = "type")]
    doc_type: DocType,
    status: Status,
    author: String,
    date: NaiveDate,
    tags: Vec<String>,
    #[serde(default)]
    related: Vec<serde_yaml::Value>,
    #[serde(default, rename = "validate-ignore")]
    validate_ignore: bool,
}

pub fn rewrite_frontmatter<F>(path: &Path, mutate: F) -> Result<()>
where
    F: FnOnce(&mut serde_yaml::Value) -> Result<()>,
{
    let content = std::fs::read_to_string(path)?;
    let (yaml, body) = split_frontmatter(&content)?;
    let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
    mutate(&mut value)?;
    let new_yaml = serde_yaml::to_string(&value)?;
    let output = format!("---\n{}---\n{}", new_yaml, body);
    std::fs::write(path, output)?;
    Ok(())
}

pub fn split_frontmatter(content: &str) -> Result<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(anyhow!("no frontmatter found"));
    }

    let after_first = &trimmed[3..];
    let end = after_first
        .find("\n---")
        .ok_or_else(|| anyhow!("no closing frontmatter delimiter"))?;

    let frontmatter = after_first[..end].trim().to_string();
    let body = after_first[end + 4..].to_string();

    Ok((frontmatter, body))
}

fn parse_relation(value: &serde_yaml::Value) -> Result<Relation> {
    let map = value
        .as_mapping()
        .ok_or_else(|| anyhow!("relation entry must be a mapping"))?;

    let (key, val) = map
        .iter()
        .next()
        .ok_or_else(|| anyhow!("relation mapping is empty"))?;

    let key_str = key
        .as_str()
        .ok_or_else(|| anyhow!("relation key must be a string"))?;

    let rel_type: RelationType = key_str.parse()?;

    let target = val
        .as_str()
        .ok_or_else(|| anyhow!("relation target must be a string"))?
        .to_string();

    Ok(Relation { rel_type, target })
}

impl DocMeta {
    pub fn parse(content: &str) -> Result<Self> {
        let (frontmatter, _) = split_frontmatter(content)?;
        let raw: RawFrontmatter = serde_yaml::from_str(&frontmatter)?;

        let related = raw
            .related
            .iter()
            .map(parse_relation)
            .collect::<Result<Vec<_>>>()?;

        Ok(DocMeta {
            path: PathBuf::new(),
            title: raw.title,
            doc_type: raw.doc_type,
            status: raw.status,
            author: raw.author,
            date: raw.date,
            tags: raw.tags,
            related,
            validate_ignore: raw.validate_ignore,
            virtual_doc: false,
            id: String::new(),
        })
    }

    pub fn extract_body(content: &str) -> Result<String> {
        let (_, body) = split_frontmatter(content)?;
        Ok(body.trim_start_matches('\n').to_string())
    }
}
