use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use regex::Regex;

use crate::engine::document::{self, DocMeta, DocType, Relation, Status};
use std::path::PathBuf;

/// Fields that come from GitHub Issue primitives rather than the issue body.
pub struct IssueContext {
    pub title: String,
    pub labels: Vec<String>,
    pub is_open: bool,
    pub known_types: Vec<String>,
}

const COMMENT_START: &str = "<!-- lazyspec\n";
const COMMENT_END: &str = "\n-->";

/// Serialize a `DocMeta` and markdown body into a GitHub Issue body string.
///
/// Fields with GitHub-native equivalents (title, tags/labels, type, lifecycle
/// status) are omitted from the HTML comment; only `author`, `date`, `related`,
/// and non-lifecycle `status` are embedded.
pub fn serialize(doc: &DocMeta, body: &str) -> String {
    let mut yaml_lines: Vec<String> = Vec::new();

    yaml_lines.push(format!("author: {}", doc.author));
    yaml_lines.push(format!("date: {}", doc.date));

    if needs_frontmatter_status(&doc.status) {
        yaml_lines.push(format!("status: {}", doc.status));
    }

    if !doc.related.is_empty() {
        yaml_lines.push("related:".to_string());
        for rel in &doc.related {
            yaml_lines.push(format!("- {}: {}", rel.rel_type, rel.target));
        }
    }

    let yaml_block = yaml_lines.join("\n");

    let comment = format!("{COMMENT_START}---\n{yaml_block}\n---{COMMENT_END}");

    let body_trimmed = body.trim();
    if body_trimmed.is_empty() {
        comment
    } else {
        format!("{comment}\n\n{body_trimmed}")
    }
}

/// Deserialize a GitHub Issue body back into a `DocMeta` and markdown body.
///
/// The `IssueContext` supplies title, labels (mapped to tags and doc_type), and
/// open/closed state (mapped to lifecycle status). Fields inside the HTML
/// comment supply author, date, related, and optionally a non-lifecycle status.
pub fn deserialize(issue_body: &str, ctx: &IssueContext) -> Result<(DocMeta, String)> {
    let (frontmatter, body) = extract_comment(issue_body)?;
    let parsed: CommentFrontmatter = serde_yaml::from_str(&frontmatter)
        .map_err(|e| anyhow!("failed to parse lazyspec comment frontmatter: {e}"))?;

    let related = parsed
        .related
        .unwrap_or_default()
        .iter()
        .map(parse_relation)
        .collect::<Result<Vec<_>>>()?;

    let known_type_refs: Vec<&str> = ctx.known_types.iter().map(|s| s.as_str()).collect();
    let (doc_type, tags) = extract_type_and_tags(&ctx.labels, &known_type_refs);

    let status = reconstruct_status(ctx.is_open, parsed.status.as_deref());

    let meta = DocMeta {
        path: PathBuf::new(),
        title: ctx.title.clone(),
        doc_type,
        status,
        author: parsed.author,
        date: parsed.date,
        tags,
        related,
        validate_ignore: false,
        virtual_doc: false,
        id: String::new(),
    };

    Ok((meta, body))
}

fn needs_frontmatter_status(status: &Status) -> bool {
    !matches!(status, Status::Draft | Status::Complete)
}

/// Reconstruct status from GitHub open/closed state and optional frontmatter
/// override. Non-lifecycle statuses (rejected, superseded) are stored in
/// frontmatter because they can't be derived from open/closed alone.
fn reconstruct_status(is_open: bool, frontmatter_status: Option<&str>) -> Status {
    if let Some(s) = frontmatter_status {
        if let Ok(status) = s.parse::<Status>() {
            return status;
        }
    }

    if is_open {
        Status::Draft
    } else {
        Status::Complete
    }
}

/// Extract doc_type and tags from GitHub labels.
///
/// The first label matching a known doc type is used as the type; all remaining
/// labels become tags.
fn extract_type_and_tags(labels: &[String], known_types: &[&str]) -> (DocType, Vec<String>) {
    let mut doc_type: Option<DocType> = None;
    let mut tags = Vec::new();

    for label in labels {
        let lower = label.to_lowercase();
        if let Some(suffix) = lower.strip_prefix("lazyspec:") {
            if doc_type.is_none() && known_types.iter().any(|t| t.to_lowercase() == suffix) {
                doc_type = Some(DocType::new(suffix));
            }
            // lazyspec:-prefixed labels are never added to tags
        } else {
            tags.push(label.clone());
        }
    }

    (doc_type.unwrap_or_else(|| DocType::new("spec")), tags)
}

#[derive(serde::Deserialize)]
struct CommentFrontmatter {
    author: String,
    date: NaiveDate,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    related: Option<Vec<serde_yaml::Value>>,
}

fn parse_relation(value: &serde_yaml::Value) -> Result<Relation> {
    document::parse_relation(value)
}

/// Extract the YAML frontmatter from the `<!-- lazyspec ... -->` comment and
/// return it alongside the remaining body text.
fn extract_comment(issue_body: &str) -> Result<(String, String)> {
    let re = Regex::new(r"(?s)<!--\s*lazyspec\s*\n---\n(.*?)\n---\s*\n-->").unwrap();

    let caps = re
        .captures(issue_body)
        .ok_or_else(|| anyhow!("no lazyspec HTML comment found in issue body"))?;

    let yaml = caps.get(1).unwrap().as_str().to_string();
    let full_match = caps.get(0).unwrap();
    let rest = issue_body[full_match.end()..].trim().to_string();

    Ok((yaml, rest))
}

/// Extract a doc ID (e.g. "STORY-042") from a title string.
///
/// Checks whether the title starts with the pattern, then falls back to
/// scanning whitespace-separated words.
pub fn extract_doc_id_from_title(title: &str, type_name: &str) -> Option<String> {
    let prefix = type_name.to_uppercase();
    let tag = format!("{}-", prefix);

    if let Some(rest) = title.strip_prefix(&tag) {
        let id_part: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
        if !id_part.is_empty() {
            return Some(format!("{}-{}", prefix, id_part));
        }
    }

    for word in title.split_whitespace() {
        if let Some(rest) = word.strip_prefix(&tag) {
            let id_part: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
            if !id_part.is_empty() {
                return Some(format!("{}-{}", prefix, id_part));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::RelationType;
    use chrono::NaiveDate;

    fn sample_doc() -> DocMeta {
        DocMeta {
            path: PathBuf::new(),
            title: "Add caching layer".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::Draft,
            author: "agent-7".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
            tags: vec!["performance".to_string()],
            related: vec![Relation {
                rel_type: RelationType::Implements,
                target: "STORY-075".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            id: "RFC-042".to_string(),
        }
    }

    fn default_known_types() -> Vec<String> {
        vec![
            "rfc".to_string(),
            "story".to_string(),
            "iteration".to_string(),
            "adr".to_string(),
            "spec".to_string(),
        ]
    }

    fn sample_context() -> IssueContext {
        IssueContext {
            title: "Add caching layer".to_string(),
            labels: vec!["lazyspec:rfc".to_string(), "performance".to_string()],
            is_open: true,
            known_types: default_known_types(),
        }
    }

    #[test]
    fn serialize_produces_comment_block() {
        let doc = sample_doc();
        let result = serialize(&doc, "Some body text.");

        assert!(result.starts_with("<!-- lazyspec\n---\n"));
        assert!(result.contains("author: agent-7"));
        assert!(result.contains("date: 2026-03-27"));
        assert!(result.contains("- implements: STORY-075"));
        assert!(result.ends_with("Some body text."));
    }

    #[test]
    fn serialize_omits_lifecycle_status() {
        let doc = sample_doc();
        let result = serialize(&doc, "");
        assert!(!result.contains("status:"));
    }

    #[test]
    fn serialize_includes_non_lifecycle_status() {
        let mut doc = sample_doc();
        doc.status = Status::Rejected;
        let result = serialize(&doc, "");
        assert!(result.contains("status: rejected"));
    }

    #[test]
    fn serialize_empty_body() {
        let mut doc = sample_doc();
        doc.related = vec![];
        let result = serialize(&doc, "");
        assert!(!result.contains("\n\n"));
        assert!(result.ends_with("-->"));
    }

    #[test]
    fn deserialize_round_trip() {
        let doc = sample_doc();
        let body = "Some body text.";
        let serialized = serialize(&doc, body);
        let ctx = sample_context();

        let (meta, parsed_body) = deserialize(&serialized, &ctx).unwrap();

        assert_eq!(meta.title, "Add caching layer");
        assert_eq!(meta.author, "agent-7");
        assert_eq!(meta.date, NaiveDate::from_ymd_opt(2026, 3, 27).unwrap());
        assert_eq!(meta.doc_type.as_str(), "rfc");
        assert_eq!(meta.tags, vec!["performance"]);
        assert_eq!(meta.related.len(), 1);
        assert_eq!(meta.related[0].rel_type, RelationType::Implements);
        assert_eq!(meta.related[0].target, "STORY-075");
        assert_eq!(parsed_body, "Some body text.");
    }

    #[test]
    fn deserialize_missing_comment_returns_error() {
        let ctx = sample_context();
        let result = deserialize("just some markdown", &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no lazyspec HTML comment found"));
    }

    #[test]
    fn deserialize_malformed_yaml_returns_error() {
        let bad = "<!-- lazyspec\n---\n[invalid yaml\n---\n-->\n\nbody";
        let ctx = sample_context();
        let result = deserialize(bad, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn status_from_open_issue_without_frontmatter() {
        assert_eq!(reconstruct_status(true, None), Status::Draft);
    }

    #[test]
    fn status_from_closed_issue_without_frontmatter() {
        assert_eq!(reconstruct_status(false, None), Status::Complete);
    }

    #[test]
    fn status_from_frontmatter_overrides_open_closed() {
        assert_eq!(
            reconstruct_status(false, Some("rejected")),
            Status::Rejected
        );
        assert_eq!(
            reconstruct_status(false, Some("superseded")),
            Status::Superseded
        );
    }

    #[test]
    fn extract_type_and_tags_finds_type() {
        let labels = vec!["lazyspec:rfc".to_string(), "cache".to_string()];
        let types = default_known_types();
        let known: Vec<&str> = types.iter().map(|s| s.as_str()).collect();
        let (dt, tags) = extract_type_and_tags(&labels, &known);
        assert_eq!(dt.as_str(), "rfc");
        assert_eq!(tags, vec!["cache"]);
    }

    #[test]
    fn extract_type_and_tags_defaults_to_spec() {
        let labels = vec!["random-label".to_string()];
        let types = default_known_types();
        let known: Vec<&str> = types.iter().map(|s| s.as_str()).collect();
        let (dt, tags) = extract_type_and_tags(&labels, &known);
        assert_eq!(dt.as_str(), "spec");
        assert_eq!(tags, vec!["random-label"]);
    }

    #[test]
    fn round_trip_with_non_lifecycle_status() {
        let mut doc = sample_doc();
        doc.status = Status::Superseded;
        let serialized = serialize(&doc, "body");

        let ctx = IssueContext {
            title: doc.title.clone(),
            labels: vec!["lazyspec:rfc".to_string(), "performance".to_string()],
            is_open: false,
            known_types: default_known_types(),
        };

        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::Superseded);
    }

    #[test]
    fn round_trip_with_multiple_relations() {
        let mut doc = sample_doc();
        doc.related = vec![
            Relation {
                rel_type: RelationType::Implements,
                target: "STORY-075".to_string(),
            },
            Relation {
                rel_type: RelationType::Blocks,
                target: "RFC-010".to_string(),
            },
        ];

        let serialized = serialize(&doc, "");
        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();

        assert_eq!(meta.related.len(), 2);
        assert_eq!(meta.related[1].rel_type, RelationType::Blocks);
        assert_eq!(meta.related[1].target, "RFC-010");
    }

    #[test]
    fn round_trip_with_no_relations() {
        let mut doc = sample_doc();
        doc.related = vec![];
        let serialized = serialize(&doc, "body here");
        let ctx = sample_context();
        let (meta, body) = deserialize(&serialized, &ctx).unwrap();
        assert!(meta.related.is_empty());
        assert_eq!(body, "body here");
    }

    #[test]
    fn round_trip_review_status() {
        let mut doc = sample_doc();
        doc.status = Status::Review;
        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("status: review"));

        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::Review);
    }

    #[test]
    fn round_trip_accepted_status() {
        let mut doc = sample_doc();
        doc.status = Status::Accepted;
        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("status: accepted"));

        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::Accepted);
    }

    #[test]
    fn round_trip_in_progress_status() {
        let mut doc = sample_doc();
        doc.status = Status::InProgress;
        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("status: in-progress"));

        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::InProgress);
    }

    #[test]
    fn serialize_omits_complete_status() {
        let mut doc = sample_doc();
        doc.status = Status::Complete;
        let result = serialize(&doc, "");
        assert!(!result.contains("status:"));
    }

    #[test]
    fn extract_type_and_tags_filters_lazyspec_labels() {
        let labels = vec![
            "lazyspec:iteration".to_string(),
            "lazyspec:unknown".to_string(),
            "team-alpha".to_string(),
        ];
        let types = default_known_types();
        let known: Vec<&str> = types.iter().map(|s| s.as_str()).collect();
        let (dt, tags) = extract_type_and_tags(&labels, &known);
        assert_eq!(dt.as_str(), "iteration");
        assert_eq!(tags, vec!["team-alpha"]);
    }

    // --- Round-trip fidelity tests ---

    #[test]
    fn round_trip_body_with_html_comments() {
        let doc = sample_doc();
        let body = "Some text\n\n<!-- this is a regular HTML comment -->\n\nMore text";
        let serialized = serialize(&doc, body);
        let ctx = sample_context();
        let (_, parsed_body) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(parsed_body, body);
    }

    #[test]
    fn round_trip_body_with_triple_dash_lines() {
        let doc = sample_doc();
        let body = "Section one\n\n---\n\nSection two\n\n---\n\nSection three";
        let serialized = serialize(&doc, body);
        let ctx = sample_context();
        let (_, parsed_body) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(parsed_body, body);
    }

    // --- Edge case and error tests ---

    #[test]
    fn unclosed_lazyspec_comment_returns_error() {
        let bad = "<!-- lazyspec\n---\nauthor: someone\ndate: 2026-01-01\n---\nno closing arrow";
        let ctx = sample_context();
        let result = deserialize(bad, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no lazyspec HTML comment found"));
    }

    #[test]
    fn empty_yaml_block_returns_error() {
        let bad = "<!-- lazyspec\n---\n\n---\n-->\n\nbody text";
        let ctx = sample_context();
        let result = deserialize(bad, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to parse lazyspec comment frontmatter"));
    }

    #[test]
    fn unknown_yaml_fields_are_ignored() {
        let input = "<!-- lazyspec\n---\nauthor: agent-7\ndate: 2026-03-27\nfuture_field: some_value\nanother_unknown: 42\n---\n-->\n\nbody";
        let ctx = sample_context();
        let (meta, body) = deserialize(input, &ctx).unwrap();
        assert_eq!(meta.author, "agent-7");
        assert_eq!(body, "body");
    }

    #[test]
    fn extra_whitespace_around_comment_block_tolerated() {
        let input = "<!--   lazyspec   \n---\nauthor: agent-7\ndate: 2026-03-27\n---\n-->\n\nbody";
        let ctx = sample_context();
        let (meta, body) = deserialize(input, &ctx).unwrap();
        assert_eq!(meta.author, "agent-7");
        assert_eq!(body, "body");
    }

    #[test]
    fn multiple_lazyspec_blocks_first_wins() {
        let input = "<!-- lazyspec\n---\nauthor: first-author\ndate: 2026-01-01\n---\n-->\n\nsome body\n\n<!-- lazyspec\n---\nauthor: second-author\ndate: 2026-12-31\n---\n-->";
        let ctx = sample_context();
        let (meta, body) = deserialize(input, &ctx).unwrap();
        assert_eq!(meta.author, "first-author");
        assert_eq!(meta.date, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        // The second block is treated as part of the body
        assert!(body.contains("<!-- lazyspec"));
        assert!(body.contains("second-author"));
    }

    #[test]
    fn extract_doc_id_prefix() {
        assert_eq!(
            extract_doc_id_from_title("STORY-042 Implement feature", "story"),
            Some("STORY-042".to_string())
        );
    }

    #[test]
    fn extract_doc_id_mid_title() {
        assert_eq!(
            extract_doc_id_from_title("Some prefix STORY-007 suffix", "story"),
            Some("STORY-007".to_string())
        );
    }

    #[test]
    fn extract_doc_id_none_when_missing() {
        assert_eq!(extract_doc_id_from_title("Just a random title", "story"), None);
    }

    #[test]
    fn extract_doc_id_different_type() {
        assert_eq!(
            extract_doc_id_from_title("RFC-001 Some RFC", "rfc"),
            Some("RFC-001".to_string())
        );
    }

    #[test]
    fn custom_type_recognized_when_in_known_types() {
        let labels = vec!["lazyspec:task".to_string(), "team-beta".to_string()];
        let known = vec!["task", "rfc", "story"];
        let (dt, tags) = extract_type_and_tags(&labels, &known);
        assert_eq!(dt.as_str(), "task");
        assert_eq!(tags, vec!["team-beta"]);
    }

    #[test]
    fn custom_type_defaults_to_spec_when_not_in_known_types() {
        let labels = vec!["lazyspec:task".to_string(), "team-beta".to_string()];
        let known = vec!["rfc", "story"];
        let (dt, tags) = extract_type_and_tags(&labels, &known);
        assert_eq!(dt.as_str(), "spec");
        assert_eq!(tags, vec!["team-beta"]);
    }
}
