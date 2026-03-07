use crate::engine::config::{Config, Severity, ValidationRule};
use crate::engine::document::{DocType, RelationType, Status};
use std::path::PathBuf;

#[derive(Debug)]
pub enum ValidationIssue {
    BrokenLink { source: PathBuf, target: PathBuf },
    MissingParentLink {
        path: PathBuf,
        rule_name: String,
        child_type: String,
        parent_type: String,
    },
    MissingRelation {
        path: PathBuf,
        rule_name: String,
        doc_type: String,
    },
    SupersededParent { path: PathBuf, parent: PathBuf },
    RejectedParent { path: PathBuf, parent: PathBuf },
    OrphanedAcceptance { path: PathBuf, parent: PathBuf },
    AllChildrenAccepted {
        parent: PathBuf,
        children: Vec<PathBuf>,
    },
    UpwardOrphanedAcceptance {
        path: PathBuf,
        parent: PathBuf,
    },
}

#[derive(Debug, Default)]
pub struct ValidationResult {
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationIssue::BrokenLink { source, target } => {
                write!(f, "broken link: {} -> {}", source.display(), target.display())
            }
            ValidationIssue::MissingParentLink { path, rule_name, child_type, parent_type } => {
                write!(f, "missing parent link [{}]: {} ({} needs {})", rule_name, path.display(), child_type, parent_type)
            }
            ValidationIssue::MissingRelation { path, rule_name, doc_type } => {
                write!(f, "missing relation [{}]: {} ({} needs a relation)", rule_name, path.display(), doc_type)
            }
            ValidationIssue::SupersededParent { path, parent } => {
                write!(f, "implements superseded document: {} -> {}", path.display(), parent.display())
            }
            ValidationIssue::RejectedParent { path, parent } => {
                write!(f, "implements rejected document: {} -> {}", path.display(), parent.display())
            }
            ValidationIssue::OrphanedAcceptance { path, parent } => {
                write!(f, "accepted but parent not accepted: {} -> {}", path.display(), parent.display())
            }
            ValidationIssue::AllChildrenAccepted { parent, children } => {
                write!(f, "all children accepted but parent not accepted: {} ({} children)", parent.display(), children.len())
            }
            ValidationIssue::UpwardOrphanedAcceptance { path, parent } => {
                write!(f, "accepted child but parent not accepted: {} -> {}", path.display(), parent.display())
            }
        }
    }
}

fn emit_issue(result: &mut ValidationResult, severity: &Severity, issue: ValidationIssue) {
    match severity {
        Severity::Error => result.errors.push(issue),
        Severity::Warning => result.warnings.push(issue),
    }
}

fn rel_type_matches(rel_type: &RelationType, link_str: &str) -> bool {
    rel_type.to_string() == link_str
}

pub fn validate_full(store: &super::store::Store, config: &Config) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Collect parent-child hierarchy pairs from config rules for status-based checks
    let hierarchy: Vec<(String, String, String)> = config
        .rules
        .iter()
        .filter_map(|rule| match rule {
            ValidationRule::ParentChild { parent, child, link, .. } => {
                Some((parent.clone(), child.clone(), link.clone()))
            }
            _ => None,
        })
        .collect();

    for (path, meta) in &store.docs {
        if meta.validate_ignore {
            continue;
        }

        // Broken link check + status-based checks on relations
        for rel in &meta.related {
            let target = PathBuf::from(&rel.target);
            if !store.docs.contains_key(&target) {
                result.errors.push(ValidationIssue::BrokenLink {
                    source: path.clone(),
                    target,
                });
                continue;
            }

            // Status-based checks: use hierarchy to determine parent-child link types
            let is_hierarchy_link = hierarchy.iter().any(|(_, _, link)| rel_type_matches(&rel.rel_type, link));
            if is_hierarchy_link {
                if let Some(parent_doc) = store.docs.get(&target) {
                    if parent_doc.status == Status::Rejected {
                        result.errors.push(ValidationIssue::RejectedParent {
                            path: path.clone(),
                            parent: target.clone(),
                        });
                    } else if parent_doc.status == Status::Superseded
                        && meta.status == Status::Accepted
                    {
                        result.warnings.push(ValidationIssue::SupersededParent {
                            path: path.clone(),
                            parent: target.clone(),
                        });
                    }

                    // OrphanedAcceptance: accepted child with non-accepted parent
                    let is_child_in_hierarchy = hierarchy.iter().any(|(pt, ct, link)| {
                        meta.doc_type == DocType::new(ct)
                            && parent_doc.doc_type == DocType::new(pt)
                            && rel_type_matches(&rel.rel_type, link)
                    });
                    if is_child_in_hierarchy
                        && meta.status == Status::Accepted
                        && parent_doc.status != Status::Accepted
                    {
                        result.warnings.push(ValidationIssue::OrphanedAcceptance {
                            path: path.clone(),
                            parent: target.clone(),
                        });
                    }
                }
            }
        }

        // Rule-driven checks
        for rule in &config.rules {
            match rule {
                ValidationRule::ParentChild { name, child, parent, link, severity } => {
                    if meta.doc_type != DocType::new(child) {
                        continue;
                    }
                    let has_parent_link = meta.related.iter().any(|r| {
                        rel_type_matches(&r.rel_type, link)
                            && store
                                .docs
                                .get(&PathBuf::from(&r.target))
                                .map(|d| d.doc_type == DocType::new(parent))
                                .unwrap_or(false)
                    });
                    if !has_parent_link {
                        emit_issue(&mut result, severity, ValidationIssue::MissingParentLink {
                            path: path.clone(),
                            rule_name: name.clone(),
                            child_type: child.clone(),
                            parent_type: parent.clone(),
                        });
                    }
                }
                ValidationRule::RelationExistence { name, doc_type, severity, .. } => {
                    if meta.doc_type != DocType::new(doc_type) {
                        continue;
                    }
                    if meta.related.is_empty() {
                        emit_issue(&mut result, severity, ValidationIssue::MissingRelation {
                            path: path.clone(),
                            rule_name: name.clone(),
                            doc_type: doc_type.clone(),
                        });
                    }
                }
            }
        }
    }

    // Status-based hierarchy checks: AllChildrenAccepted, UpwardOrphanedAcceptance
    // Build from configured parent-child rules
    for (parent_type, child_type, link) in &hierarchy {
        for (parent_path, meta) in &store.docs {
            if meta.doc_type != DocType::new(parent_type) {
                continue;
            }

            let children: Vec<PathBuf> = store
                .reverse_links
                .get(parent_path)
                .into_iter()
                .flatten()
                .filter(|(rel_type, child_path)| {
                    rel_type_matches(rel_type, link)
                        && store
                            .docs
                            .get(child_path)
                            .map(|d| d.doc_type == DocType::new(child_type) && !d.validate_ignore)
                            .unwrap_or(false)
                })
                .map(|(_, child_path)| child_path.clone())
                .collect();

            if children.is_empty() {
                continue;
            }

            let parent_is_draft_or_review =
                meta.status == Status::Draft || meta.status == Status::Review;

            let all_accepted = children.iter().all(|cp| {
                store
                    .docs
                    .get(cp)
                    .map(|d| d.status == Status::Accepted)
                    .unwrap_or(false)
            });

            if all_accepted && parent_is_draft_or_review {
                result.warnings.push(ValidationIssue::AllChildrenAccepted {
                    parent: parent_path.clone(),
                    children,
                });
                continue;
            }

            if parent_is_draft_or_review {
                for child_path in &children {
                    if let Some(child) = store.docs.get(child_path) {
                        if child.status == Status::Accepted
                            && child.doc_type == DocType::new(child_type)
                        {
                            result
                                .warnings
                                .push(ValidationIssue::UpwardOrphanedAcceptance {
                                    path: child_path.clone(),
                                    parent: parent_path.clone(),
                                });
                        }
                    }
                }
            }
        }
    }

    result
}
