use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use regex::Regex;

use crate::engine::config::{AttrDef, TypeDef};
use crate::engine::document::{
    self, coerce_attr, deserialize_naive_date, AttrValue, DocMeta, DocType, Relation, Status,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// One document type's resolved classification rule against a GitHub issue.
///
/// - `name` is the lazyspec type name (the resulting `DocType` on a match).
/// - `label` is the type's `github_label()` (its `label_override` or the default
///   `lazyspec:{name}`), checked when neither `tag` nor `issue_type` is set.
/// - `tag` is an arbitrary GitHub label naming this type; when set, the `label`
///   check is skipped and this label is checked instead.
/// - `issue_type` is a native GitHub issue type naming this type.
///
/// When both `tag` and `issue_type` are set they are AND-combined. See
/// [`extract_type_and_tags`] for the full match semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMatchRule {
    pub name: String,
    pub label: String,
    pub tag: Option<String>,
    pub issue_type: Option<String>,
}

impl From<&TypeDef> for TypeMatchRule {
    fn from(type_def: &TypeDef) -> Self {
        TypeMatchRule {
            name: type_def.name.clone(),
            label: type_def.github_label(),
            tag: type_def.github_issue_tag.clone(),
            issue_type: type_def.github_issue_type.clone(),
        }
    }
}

/// Fields that come from GitHub Issue primitives rather than the issue body.
pub struct IssueContext {
    pub title: String,
    pub labels: Vec<String>,
    pub is_open: bool,
    /// Known document types as resolved [`TypeMatchRule`]s. Each type's rule is
    /// evaluated independently against the issue's labels and native issue type
    /// to decide whether the issue belongs to that type.
    pub known_types: Vec<TypeMatchRule>,
    /// The issue's own native GitHub issue type, distinct from a classification
    /// rule's `issue_type`. Used to evaluate rules configured with a
    /// `github_issue_type`.
    pub issue_type: Option<String>,
    pub default_type: String,
    /// Declared attribute schema for the document's type, used to coerce the
    /// `attributes:` block in the HTML comment back into typed [`AttrValue`]s.
    pub attr_defs: Vec<AttrDef>,
    /// The lifecycle status a remote-`open` issue maps to (the type's first
    /// active state) when the body carries no explicit lifecycle status.
    /// Derived by the caller from the type's `lifecycle.first_active_status()`.
    pub open_status: String,
    /// The lifecycle status a remote-`closed` issue maps to (the type's terminal
    /// state) when the body carries no explicit lifecycle status. Derived by the
    /// caller from the type's `lifecycle.terminal_status()`.
    pub closed_status: String,
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

    yaml_lines.push(format!("date: {}", doc.date));

    if needs_frontmatter_status(&doc.status) {
        yaml_lines.push(format!("status: {}", doc.status));
    }

    if !doc.provenance.is_empty() {
        yaml_lines.push("provenance:".to_string());
        for entry in &doc.provenance {
            let yaml_value = serde_yaml::to_string(entry).unwrap_or_else(|_| entry.clone());
            yaml_lines.push(format!("- {}", yaml_value.trim_end()));
        }
    }

    if !doc.related.is_empty() {
        yaml_lines.push("related:".to_string());
        for rel in &doc.related {
            yaml_lines.push(format!("- {}: {}", rel.rel_type, rel.target));
        }
    }

    if !doc.attributes.is_empty() {
        yaml_lines.push("attributes:".to_string());
        for (key, value) in &doc.attributes {
            let scalar = serde_yaml::to_string(value)
                .map(|s| s.trim_end().to_string())
                .unwrap_or_default();
            yaml_lines.push(format!("  {}: {}", key, scalar));
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

    let (doc_type, tags) = extract_type_and_tags(
        &ctx.labels,
        &ctx.known_types,
        ctx.issue_type.as_deref(),
        &ctx.default_type,
    );

    let status = reconstruct_status(
        ctx.is_open,
        parsed.status.as_deref(),
        &ctx.open_status,
        &ctx.closed_status,
    );

    let attributes = parse_attributes(parsed.attributes.as_ref(), &ctx.attr_defs);

    let meta = DocMeta {
        path: PathBuf::new(),
        title: ctx.title.clone(),
        doc_type,
        status,
        author: parsed.author.unwrap_or_else(|| "unknown".to_string()),
        date: parsed.date,
        tags,
        provenance: parsed.provenance.unwrap_or_default(),
        related,
        validate_ignore: false,
        virtual_doc: false,
        assignee: None,
        attributes,
        id: String::new(),
    };

    Ok((meta, body))
}

/// Coerce the raw `attributes:` mapping from the HTML comment against the type's
/// declared [`AttrDef`]s. Declared keys are coerced to their kind; undeclared
/// keys are preserved as [`AttrValue::Raw`], mirroring `parse_with_schema`.
fn parse_attributes(
    mapping: Option<&serde_yaml::Mapping>,
    attr_defs: &[AttrDef],
) -> BTreeMap<String, AttrValue> {
    let Some(mapping) = mapping else {
        return BTreeMap::new();
    };
    mapping
        .iter()
        .filter_map(|(k, v)| {
            let key = k.as_str()?.to_string();
            let coerced = match attr_defs.iter().find(|d| d.name == key) {
                Some(def) => coerce_attr(v, def).unwrap_or_else(|| AttrValue::Raw(v.clone())),
                None => AttrValue::Raw(v.clone()),
            };
            Some((key, coerced))
        })
        .collect()
}

fn needs_frontmatter_status(status: &Status) -> bool {
    !matches!(status.as_str(), "draft" | "complete")
}

/// Whether a lifecycle `status` corresponds to a GitHub *open* issue, given the
/// type's `terminal_status` (from [`Lifecycle::terminal_status`], the single
/// ITERATION-318 derivation). A status is closed-equivalent iff it is that
/// terminal state OR one of the canonical closed-exceptional states
/// (`complete`/`rejected`/`superseded`); every other status is open-equivalent.
/// Passing the terminal state in makes the classification lifecycle-aware so a
/// CUSTOM terminal (e.g. `shipped`) closes while a custom intermediate state
/// stays open, without a second terminal definition. For the default lifecycle
/// the terminal is `complete`, already in the canonical closed set, so behaviour
/// is unchanged. The single classification both the sync read reconcile
/// ([`reconstruct_status`]) and the github-backed write-through
/// (`GithubIssuesStore::update`'s `should_be_open`, `GithubMilestonesStore::update`)
/// share, so read and write agree on which statuses mean open and never disagree
/// with GitHub's own bit.
pub fn status_maps_to_open(status: &str, terminal_status: &str) -> bool {
    status != terminal_status && !matches!(status, "complete" | "rejected" | "superseded")
}

/// Reconstruct status from GitHub open/closed state and optional frontmatter
/// override, reconciling the two so the remote open/closed bit is always the
/// source of truth (STORY-223 AC3).
///
/// An explicit frontmatter status wins ONLY when it agrees with the open/closed
/// bit (per [`status_maps_to_open`]): an intermediate open state like
/// `review`/`in-progress` round-trips while the issue is open, and an
/// exceptional closed state like `rejected`/`superseded` round-trips while the
/// issue is closed. When the stored status contradicts the bit -- e.g. a
/// `status: in-progress` body on an issue closed directly on GitHub -- it is a
/// stale local value the remote overrides: the bit maps to the type's
/// first-active (`open_status`) / terminal (`closed_status`) lifecycle state.
/// With no frontmatter status the bit maps the same way (STORY-223 AC1).
fn reconstruct_status(
    is_open: bool,
    frontmatter_status: Option<&str>,
    open_status: &str,
    closed_status: &str,
) -> Status {
    if let Some(s) = frontmatter_status {
        if let Ok(status) = s.parse::<Status>() {
            if status_maps_to_open(status.as_str(), closed_status) == is_open {
                return status;
            }
        }
    }

    if is_open {
        Status::new(open_status)
    } else {
        Status::new(closed_status)
    }
}

/// Extract doc_type and tags by evaluating each known type's [`TypeMatchRule`]
/// against the issue's labels and native issue type.
///
/// Every rule is evaluated independently -- there is no first-hit short circuit.
/// A rule is satisfied when:
/// - neither `tag` nor `issue_type` is set: a label case-insensitively equals
///   `rule.label`;
/// - only `tag` is set: a label case-insensitively equals `rule.tag` (the
///   `label` check is skipped);
/// - only `issue_type` is set: `issue_native_type` equals `rule.issue_type`;
/// - both are set: both the tag-label match and the issue-type match hold (AND).
///
/// The first satisfied rule (in `known_types` order) wins the returned
/// `DocType`; with no satisfied rule, `default_type` is used. A label that any
/// rule uses to classify (its `label` when unqualified, or its `tag` when set)
/// is never carried as a tag, regardless of whether that rule's overall match
/// succeeded. Every other label becomes a tag.
pub(crate) fn extract_type_and_tags(
    labels: &[String],
    known_types: &[TypeMatchRule],
    issue_native_type: Option<&str>,
    default_type: &str,
) -> (DocType, Vec<String>) {
    let has_label = |value: &str| {
        let lower = value.to_lowercase();
        labels.iter().any(|l| l.to_lowercase() == lower)
    };

    // Single pass over the rules couples doc_type resolution with tag exclusion:
    // both are derived here so a short-circuit that stopped once doc_type is found
    // would also stop excluding later rules' classifying labels.
    let mut doc_type: Option<DocType> = None;
    let mut classifying_labels: Vec<String> = Vec::new();
    for rule in known_types {
        // The label value a rule classifies on: its `tag` when set, else its
        // `label` (an `issue_type`-only rule classifies on no label). Such a
        // label is never carried as a tag.
        let classifying = match (&rule.tag, &rule.issue_type) {
            (Some(tag), _) => Some(tag.to_lowercase()),
            (None, None) => Some(rule.label.to_lowercase()),
            (None, Some(_)) => None,
        };
        if let Some(ref classifying) = classifying {
            classifying_labels.push(classifying.clone());
        }

        let satisfied = match (&rule.tag, &rule.issue_type) {
            (None, None) => has_label(&rule.label),
            (Some(tag), None) => has_label(tag),
            (None, Some(it)) => issue_native_type == Some(it.as_str()),
            (Some(tag), Some(it)) => has_label(tag) && issue_native_type == Some(it.as_str()),
        };
        if satisfied && doc_type.is_none() {
            doc_type = Some(DocType::new(&rule.name));
        }
    }

    let tags = labels
        .iter()
        .filter(|label| {
            let lower = label.to_lowercase();
            !classifying_labels.iter().any(|c| c == &lower)
        })
        .cloned()
        .collect();

    (doc_type.unwrap_or_else(|| DocType::new(default_type)), tags)
}

#[derive(serde::Deserialize)]
struct CommentFrontmatter {
    #[serde(default)]
    author: Option<String>,
    #[serde(deserialize_with = "deserialize_naive_date")]
    date: NaiveDate,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    provenance: Option<Vec<String>>,
    #[serde(default)]
    related: Option<Vec<serde_yaml::Value>>,
    #[serde(default)]
    attributes: Option<serde_yaml::Mapping>,
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
            status: Status::new("draft"),
            author: "agent-7".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
            tags: vec!["performance".to_string()],
            provenance: vec![],
            related: vec![Relation {
                rel_type: RelationType::new("implements"),
                target: "STORY-075".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "RFC-042".to_string(),
        }
    }

    /// A label-only match rule using the default `lazyspec:{name}` label.
    fn label_rule(name: &str) -> TypeMatchRule {
        TypeMatchRule {
            name: name.to_string(),
            label: format!("lazyspec:{name}"),
            tag: None,
            issue_type: None,
        }
    }

    /// The default known types as label-only rules, each using the default
    /// `lazyspec:{name}` label (no override).
    fn default_known_types() -> Vec<TypeMatchRule> {
        ["rfc", "story", "iteration", "adr", "spec"]
            .iter()
            .map(|name| label_rule(name))
            .collect()
    }

    /// Build label-only rules from bare names, each using the default
    /// `lazyspec:{name}` label.
    fn known_pairs(names: &[&str]) -> Vec<TypeMatchRule> {
        names.iter().map(|name| label_rule(name)).collect()
    }

    fn sample_context() -> IssueContext {
        IssueContext {
            title: "Add caching layer".to_string(),
            labels: vec!["lazyspec:rfc".to_string(), "performance".to_string()],
            is_open: true,
            known_types: default_known_types(),
            issue_type: None,
            default_type: "spec".to_string(),
            attr_defs: vec![],
            open_status: "draft".to_string(),
            closed_status: "complete".to_string(),
        }
    }

    #[test]
    fn serialize_produces_comment_block() {
        let doc = sample_doc();
        let result = serialize(&doc, "Some body text.");

        assert!(result.starts_with("<!-- lazyspec\n---\n"));
        assert!(
            !result.contains("author:"),
            "serialize should not emit author"
        );
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
        doc.status = Status::new("rejected");
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
        // author no longer round-trips through serialize; deserialize returns placeholder
        assert_eq!(meta.author, "unknown");
        assert_eq!(meta.date, NaiveDate::from_ymd_opt(2026, 3, 27).unwrap());
        assert_eq!(meta.doc_type.as_str(), "rfc");
        assert_eq!(meta.tags, vec!["performance"]);
        assert_eq!(meta.related.len(), 1);
        assert_eq!(meta.related[0].rel_type, RelationType::new("implements"));
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
        assert_eq!(
            reconstruct_status(true, None, "draft", "complete"),
            Status::new("draft")
        );
    }

    #[test]
    fn status_from_closed_issue_without_frontmatter() {
        assert_eq!(
            reconstruct_status(false, None, "draft", "complete"),
            Status::new("complete")
        );
    }

    // STORY-223 AC1: with no frontmatter status, open/closed maps to the type's
    // own first-active/terminal states (a custom lifecycle here), not the
    // hardcoded draft/complete.
    #[test]
    fn status_maps_custom_lifecycle_open_closed() {
        assert_eq!(
            reconstruct_status(true, None, "backlog", "shipped"),
            Status::new("backlog")
        );
        assert_eq!(
            reconstruct_status(false, None, "backlog", "shipped"),
            Status::new("shipped")
        );
    }

    // STORY-223 AC3: a closed-equivalent frontmatter status (`rejected`/
    // `superseded`) agrees with a closed issue, so it still round-trips -- the
    // reconcile only overrides a status that CONTRADICTS the open/closed bit.
    #[test]
    fn closed_equivalent_frontmatter_survives_close() {
        assert_eq!(
            reconstruct_status(false, Some("rejected"), "draft", "complete"),
            Status::new("rejected")
        );
        assert_eq!(
            reconstruct_status(false, Some("superseded"), "draft", "complete"),
            Status::new("superseded")
        );
    }

    // STORY-223 AC3 regression: an issue advanced to `in-progress` (open, status
    // persisted in the body) then CLOSED directly on GitHub. The stale
    // open-equivalent status must NOT mask the close -- the remote bit wins and
    // the doc remaps to the terminal state. This is the review blocker.
    #[test]
    fn remote_close_overrides_stale_open_status() {
        assert_eq!(
            reconstruct_status(false, Some("in-progress"), "draft", "complete"),
            Status::new("complete")
        );
        assert_eq!(
            reconstruct_status(false, Some("review"), "draft", "complete"),
            Status::new("complete")
        );
        // The stale open state remaps to the type's own terminal, custom or not.
        assert_eq!(
            reconstruct_status(false, Some("in-progress"), "backlog", "shipped"),
            Status::new("shipped")
        );
    }

    // Symmetric: an issue REOPENED on GitHub while the body still carries a
    // closed-equivalent status (`rejected`/`superseded`). The open bit wins and
    // the doc remaps to the first-active state.
    #[test]
    fn remote_reopen_overrides_stale_closed_status() {
        assert_eq!(
            reconstruct_status(true, Some("rejected"), "draft", "complete"),
            Status::new("draft")
        );
        assert_eq!(
            reconstruct_status(true, Some("superseded"), "draft", "complete"),
            Status::new("draft")
        );
    }

    // An open-equivalent intermediate status agrees with an open issue and still
    // round-trips (open/closed alone cannot express `review`/`in-progress`).
    #[test]
    fn open_equivalent_intermediate_status_round_trips() {
        assert_eq!(
            reconstruct_status(true, Some("review"), "draft", "complete"),
            Status::new("review")
        );
        assert_eq!(
            reconstruct_status(true, Some("in-progress"), "draft", "complete"),
            Status::new("in-progress")
        );
    }

    #[test]
    fn status_maps_to_open_classifies_open_and_closed_sets() {
        // Default lifecycle terminal is `complete`.
        for open in ["draft", "review", "accepted", "in-progress"] {
            assert!(
                status_maps_to_open(open, "complete"),
                "{open} should map to open"
            );
        }
        for closed in ["complete", "rejected", "superseded"] {
            assert!(
                !status_maps_to_open(closed, "complete"),
                "{closed} should map to closed"
            );
        }
    }

    // A custom terminal state (not `complete`) is closed-equivalent, while a
    // custom intermediate state stays open -- the lifecycle-aware classification
    // ITERATION-319's write-through relies on.
    #[test]
    fn status_maps_to_open_is_lifecycle_aware_for_custom_terminal() {
        assert!(
            !status_maps_to_open("shipped", "shipped"),
            "custom terminal `shipped` should map to closed"
        );
        assert!(
            status_maps_to_open("doing", "shipped"),
            "custom intermediate `doing` should map to open"
        );
        assert!(
            status_maps_to_open("backlog", "shipped"),
            "custom first-active `backlog` should map to open"
        );
    }

    #[test]
    fn extract_type_and_tags_finds_type() {
        let labels = vec!["lazyspec:rfc".to_string(), "cache".to_string()];
        let (dt, tags) = extract_type_and_tags(&labels, &default_known_types(), None, "spec");
        assert_eq!(dt.as_str(), "rfc");
        assert_eq!(tags, vec!["cache"]);
    }

    #[test]
    fn extract_type_and_tags_defaults_to_configured_type() {
        let labels = vec!["random-label".to_string()];
        let (dt, tags) = extract_type_and_tags(&labels, &default_known_types(), None, "testgh");
        assert_eq!(dt.as_str(), "testgh");
        assert_eq!(tags, vec!["random-label"]);
    }

    // A type whose resolved label is a custom override (no `lazyspec:` prefix) is
    // recognized on read by exact-matching that label.
    #[test]
    fn extract_type_and_tags_recognizes_custom_label() {
        let labels = vec!["Ticket".to_string(), "cache".to_string()];
        let known = vec![TypeMatchRule {
            name: "ticket".to_string(),
            label: "Ticket".to_string(),
            tag: None,
            issue_type: None,
        }];
        let (dt, tags) = extract_type_and_tags(&labels, &known, None, "spec");
        assert_eq!(dt.as_str(), "ticket");
        assert_eq!(tags, vec!["cache"]);
    }

    // AC3: typed attributes survive serialize -> deserialize through the HTML comment.
    #[test]
    fn round_trip_preserves_typed_attributes() {
        use crate::engine::config::{AttrDef, AttrKind};

        let mut doc = sample_doc();
        doc.attributes
            .insert("owner".to_string(), AttrValue::Str("jkaloger".to_string()));
        doc.attributes
            .insert("estimate".to_string(), AttrValue::Int(3));

        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("attributes:"), "got: {serialized}");

        let ctx = IssueContext {
            attr_defs: vec![
                AttrDef {
                    name: "owner".to_string(),
                    kind: AttrKind::Str,
                    required: false,
                    values: vec![],
                },
                AttrDef {
                    name: "estimate".to_string(),
                    kind: AttrKind::Int,
                    required: false,
                    values: vec![],
                },
            ],
            ..sample_context()
        };

        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(
            meta.attributes["owner"],
            AttrValue::Str("jkaloger".to_string())
        );
        assert_eq!(meta.attributes["estimate"], AttrValue::Int(3));
    }

    #[test]
    fn round_trip_with_non_lifecycle_status() {
        let mut doc = sample_doc();
        doc.status = Status::new("superseded");
        let serialized = serialize(&doc, "body");

        let ctx = IssueContext {
            title: doc.title.clone(),
            labels: vec!["lazyspec:rfc".to_string(), "performance".to_string()],
            is_open: false,
            known_types: default_known_types(),
            issue_type: None,
            default_type: "spec".to_string(),
            attr_defs: vec![],
            open_status: "draft".to_string(),
            closed_status: "complete".to_string(),
        };

        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::new("superseded"));
    }

    #[test]
    fn round_trip_with_multiple_relations() {
        let mut doc = sample_doc();
        doc.related = vec![
            Relation {
                rel_type: RelationType::new("implements"),
                target: "STORY-075".to_string(),
            },
            Relation {
                rel_type: RelationType::new("blocks"),
                target: "RFC-010".to_string(),
            },
        ];

        let serialized = serialize(&doc, "");
        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();

        assert_eq!(meta.related.len(), 2);
        assert_eq!(meta.related[1].rel_type, RelationType::new("blocks"));
        assert_eq!(meta.related[1].target, "RFC-010");
    }

    // AC2 (round-trip): deserializing a remote body, pushing a new relation, then
    // re-serializing keeps the original prose and carries both relations -- the
    // shape `merge_relation_to_remote` relies on.
    #[test]
    fn round_trip_relation_add_preserves_prose() {
        let doc = sample_doc();
        let serialized = serialize(&doc, "REMOTE PROSE LINE");
        let ctx = sample_context();

        let (mut meta, prose) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(prose, "REMOTE PROSE LINE");

        meta.related.push(Relation {
            rel_type: RelationType::new("blocks"),
            target: "RFC-010".to_string(),
        });

        let re_serialized = serialize(&meta, &prose);
        assert!(
            re_serialized.contains("REMOTE PROSE LINE"),
            "got: {re_serialized}"
        );
        assert!(
            re_serialized.contains("- implements: STORY-075"),
            "got: {re_serialized}"
        );
        assert!(
            re_serialized.contains("- blocks: RFC-010"),
            "got: {re_serialized}"
        );
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
        doc.status = Status::new("review");
        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("status: review"));

        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::new("review"));
    }

    #[test]
    fn round_trip_accepted_status() {
        let mut doc = sample_doc();
        doc.status = Status::new("accepted");
        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("status: accepted"));

        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::new("accepted"));
    }

    #[test]
    fn round_trip_in_progress_status() {
        let mut doc = sample_doc();
        doc.status = Status::new("in-progress");
        let serialized = serialize(&doc, "body");
        assert!(serialized.contains("status: in-progress"));

        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.status, Status::new("in-progress"));
    }

    #[test]
    fn serialize_omits_complete_status() {
        let mut doc = sample_doc();
        doc.status = Status::new("complete");
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
        let (dt, tags) = extract_type_and_tags(&labels, &default_known_types(), None, "spec");
        assert_eq!(dt.as_str(), "iteration");
        // Matching is now exact against each known type's resolved label, not a
        // `lazyspec:` prefix strip. `lazyspec:unknown` matches no known type, so
        // it becomes an ordinary tag (previously it was silently dropped).
        assert_eq!(tags, vec!["lazyspec:unknown", "team-alpha"]);
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
    fn custom_type_recognized_when_in_known_types() {
        let labels = vec!["lazyspec:task".to_string(), "team-beta".to_string()];
        let known = known_pairs(&["task", "rfc", "story"]);
        let (dt, tags) = extract_type_and_tags(&labels, &known, None, "spec");
        assert_eq!(dt.as_str(), "task");
        assert_eq!(tags, vec!["team-beta"]);
    }

    #[test]
    fn deserialize_tolerates_missing_author() {
        let input = "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody";
        let ctx = sample_context();
        let (meta, body) = deserialize(input, &ctx).unwrap();
        assert_eq!(meta.author, "unknown");
        assert_eq!(body, "body");
    }

    #[test]
    fn deserialize_backward_compat_with_author() {
        let input = "<!-- lazyspec\n---\nauthor: old-value\ndate: 2026-03-27\n---\n-->\n\nbody";
        let ctx = sample_context();
        let (meta, body) = deserialize(input, &ctx).unwrap();
        assert_eq!(meta.author, "old-value");
        assert_eq!(body, "body");
    }

    #[test]
    fn serialize_emits_provenance_block() {
        let mut doc = sample_doc();
        doc.provenance = vec!["A".to_string(), "B".to_string()];
        let result = serialize(&doc, "");
        assert!(
            result.contains("provenance:"),
            "expected provenance block, got: {}",
            result
        );
        assert!(result.contains("- A"));
        assert!(result.contains("- B"));
    }

    #[test]
    fn serialize_omits_provenance_when_empty() {
        let doc = sample_doc();
        let result = serialize(&doc, "");
        assert!(
            !result.contains("provenance:"),
            "should not emit empty provenance, got: {}",
            result
        );
    }

    #[test]
    fn deserialize_reads_provenance() {
        let input = "<!-- lazyspec\n---\nauthor: agent-7\ndate: 2026-03-27\nprovenance:\n- Workshop 2026-04-12\n- Jane Doe\n---\n-->\n\nbody";
        let ctx = sample_context();
        let (meta, _) = deserialize(input, &ctx).unwrap();
        assert_eq!(
            meta.provenance,
            vec!["Workshop 2026-04-12".to_string(), "Jane Doe".to_string()]
        );
    }

    #[test]
    fn deserialize_missing_provenance_defaults_empty() {
        let input = "<!-- lazyspec\n---\nauthor: agent-7\ndate: 2026-03-27\n---\n-->\n\nbody";
        let ctx = sample_context();
        let (meta, _) = deserialize(input, &ctx).unwrap();
        assert!(meta.provenance.is_empty());
    }

    #[test]
    fn roundtrip_preserves_provenance() {
        let mut doc = sample_doc();
        doc.provenance = vec![
            "Workshop 2026-04-12".to_string(),
            "Privacy Act 1988".to_string(),
        ];
        let serialized = serialize(&doc, "body");
        let ctx = sample_context();
        let (meta, _) = deserialize(&serialized, &ctx).unwrap();
        assert_eq!(meta.provenance, doc.provenance);
    }

    #[test]
    fn custom_type_defaults_to_configured_type_when_not_in_known_types() {
        let labels = vec!["lazyspec:task".to_string(), "team-beta".to_string()];
        let known = known_pairs(&["rfc", "story"]);
        let (dt, tags) = extract_type_and_tags(&labels, &known, None, "testgh");
        assert_eq!(dt.as_str(), "testgh");
        // `lazyspec:task` is not a recognized type here, so it survives as a tag.
        assert_eq!(tags, vec!["lazyspec:task", "team-beta"]);
    }

    // AC1: a rule with neither tag nor issue_type matches by its label, exactly
    // as before ITERATION-261/263.
    #[test]
    fn extract_type_and_tags_label_only_rule_matches_by_label() {
        let labels = vec!["lazyspec:bug".to_string(), "cache".to_string()];
        let known = vec![label_rule("bug")];
        let (dt, tags) = extract_type_and_tags(&labels, &known, None, "spec");
        assert_eq!(dt.as_str(), "bug");
        assert_eq!(tags, vec!["cache"]);
    }

    // AC2: a rule with only `tag` set matches on that plain label even when the
    // issue carries no `lazyspec:{name}` label at all -- the label check is
    // skipped, not additionally required.
    #[test]
    fn extract_type_and_tags_tag_only_rule_matches_without_lazyspec_label() {
        let labels = vec!["needs-triage".to_string(), "cache".to_string()];
        let known = vec![TypeMatchRule {
            name: "bug".to_string(),
            label: "lazyspec:bug".to_string(),
            tag: Some("needs-triage".to_string()),
            issue_type: None,
        }];
        let (dt, tags) = extract_type_and_tags(&labels, &known, None, "spec");
        assert_eq!(dt.as_str(), "bug");
        assert_eq!(tags, vec!["cache"]);
    }

    // AC3: a rule with only `issue_type` set matches on the issue's native type
    // even when the issue carries no matching label of any kind.
    #[test]
    fn extract_type_and_tags_issue_type_only_rule_matches_without_any_label() {
        let labels = vec!["cache".to_string()];
        let known = vec![TypeMatchRule {
            name: "bug".to_string(),
            label: "lazyspec:bug".to_string(),
            tag: None,
            issue_type: Some("Bug".to_string()),
        }];
        let (dt, tags) = extract_type_and_tags(&labels, &known, Some("Bug"), "spec");
        assert_eq!(dt.as_str(), "bug");
        assert_eq!(tags, vec!["cache"]);
    }

    // AC4: a rule with both `tag` and `issue_type` set is an AND, not an OR --
    // satisfying only one half does not match; satisfying both does.
    #[test]
    fn extract_type_and_tags_tag_and_issue_type_both_required_and_not_or() {
        let rule = TypeMatchRule {
            name: "bug".to_string(),
            label: "lazyspec:bug".to_string(),
            tag: Some("hot".to_string()),
            issue_type: Some("Bug".to_string()),
        };
        let known = vec![rule];

        // Only the tag holds (native type differs) -> no match, falls back.
        let (dt, _) = extract_type_and_tags(&["hot".to_string()], &known, Some("Task"), "spec");
        assert_eq!(dt.as_str(), "spec", "tag alone must not match");

        // Only the issue_type holds (tag label absent) -> no match, falls back.
        let (dt, _) = extract_type_and_tags(&["cache".to_string()], &known, Some("Bug"), "spec");
        assert_eq!(dt.as_str(), "spec", "issue_type alone must not match");

        // Both hold -> match.
        let (dt, _) = extract_type_and_tags(&["hot".to_string()], &known, Some("Bug"), "spec");
        assert_eq!(dt.as_str(), "bug", "tag AND issue_type must match");
    }

    // AC5: every rule is evaluated independently with no first-hit short circuit.
    // Two rules both match the issue; the first wins the doc_type, but the second
    // rule's classification label ("hot") must still be consumed (never carried as
    // a tag). doc_type resolution and classifying-label exclusion share a single
    // loop, so a regression that short-circuited that loop once doc_type is found
    // would leave "hot" in the returned tags and this assertion would fail.
    #[test]
    fn extract_type_and_tags_evaluates_every_rule_independently_no_short_circuit() {
        let labels = vec![
            "lazyspec:story".to_string(),
            "hot".to_string(),
            "other".to_string(),
        ];
        let known = vec![
            label_rule("story"),
            TypeMatchRule {
                name: "urgent".to_string(),
                label: "lazyspec:urgent".to_string(),
                tag: Some("hot".to_string()),
                issue_type: None,
            },
        ];
        let (dt, tags) = extract_type_and_tags(&labels, &known, None, "spec");
        assert_eq!(dt.as_str(), "story", "first satisfied rule wins doc_type");
        assert_eq!(
            tags,
            vec!["other"],
            "second rule's tag label must be consumed, proving it was evaluated"
        );
    }
}
