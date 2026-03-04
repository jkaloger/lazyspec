use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocType {
    Rfc,
    Adr,
    Spec,
    Plan,
}

impl fmt::Display for DocType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocType::Rfc => write!(f, "RFC"),
            DocType::Adr => write!(f, "ADR"),
            DocType::Spec => write!(f, "SPEC"),
            DocType::Plan => write!(f, "PLAN"),
        }
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
}

fn split_frontmatter(content: &str) -> Result<(String, String)> {
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

    let rel_type = match key_str {
        "implements" => RelationType::Implements,
        "supersedes" => RelationType::Supersedes,
        "blocks" => RelationType::Blocks,
        "related_to" => RelationType::RelatedTo,
        other => return Err(anyhow!("unknown relation type: {}", other)),
    };

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
        })
    }

    pub fn extract_body(content: &str) -> Result<String> {
        let (_, body) = split_frontmatter(content)?;
        Ok(body.trim_start_matches('\n').to_string())
    }
}
