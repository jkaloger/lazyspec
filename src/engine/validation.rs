use crate::engine::document::{DocType, RelationType, Status};
use std::path::PathBuf;

#[derive(Debug)]
pub enum ValidationIssue {
    BrokenLink { source: PathBuf, target: PathBuf },
    UnlinkedIteration { path: PathBuf },
    UnlinkedAdr { path: PathBuf },
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
            ValidationIssue::UnlinkedIteration { path } => {
                write!(f, "iteration without story link: {}", path.display())
            }
            ValidationIssue::UnlinkedAdr { path } => {
                write!(f, "ADR without any relation: {}", path.display())
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
                write!(f, "accepted story but parent RFC not accepted: {} -> {}", path.display(), parent.display())
            }
        }
    }
}

pub fn validate_full(store: &super::store::Store) -> ValidationResult {
    let mut result = ValidationResult::default();

    for (path, meta) in &store.docs {
        for rel in &meta.related {
            let target = PathBuf::from(&rel.target);
            if !store.docs.contains_key(&target) {
                result.errors.push(ValidationIssue::BrokenLink {
                    source: path.clone(),
                    target,
                });
                continue;
            }

            if rel.rel_type == RelationType::Implements {
                if let Some(parent) = store.docs.get(&target) {
                    if parent.status == Status::Rejected {
                        result.errors.push(ValidationIssue::RejectedParent {
                            path: path.clone(),
                            parent: target.clone(),
                        });
                    } else if parent.status == Status::Superseded
                        && meta.status == Status::Accepted
                    {
                        result.warnings.push(ValidationIssue::SupersededParent {
                            path: path.clone(),
                            parent: target.clone(),
                        });
                    }

                    if meta.status == Status::Accepted
                        && meta.doc_type == DocType::Iteration
                        && parent.doc_type == DocType::Story
                        && parent.status != Status::Accepted
                    {
                        result.warnings.push(ValidationIssue::OrphanedAcceptance {
                            path: path.clone(),
                            parent: target.clone(),
                        });
                    }
                }
            }
        }

        if meta.doc_type == DocType::Iteration {
            let has_story_link = meta.related.iter().any(|r| {
                r.rel_type == RelationType::Implements
                    && store
                        .docs
                        .get(&PathBuf::from(&r.target))
                        .map(|d| d.doc_type == DocType::Story)
                        .unwrap_or(false)
            });
            if !has_story_link {
                result.errors.push(ValidationIssue::UnlinkedIteration {
                    path: path.clone(),
                });
            }
        }

        if meta.doc_type == DocType::Adr && meta.related.is_empty() {
            result.errors.push(ValidationIssue::UnlinkedAdr {
                path: path.clone(),
            });
        }
    }

    for (parent_path, meta) in &store.docs {
        if meta.doc_type != DocType::Rfc && meta.doc_type != DocType::Story {
            continue;
        }

        let expected_child_type = match meta.doc_type {
            DocType::Rfc => DocType::Story,
            DocType::Story => DocType::Iteration,
            _ => continue,
        };

        let children: Vec<PathBuf> = store
            .reverse_links
            .get(parent_path)
            .into_iter()
            .flatten()
            .filter(|(rel_type, child_path)| {
                *rel_type == RelationType::Implements
                    && store
                        .docs
                        .get(child_path)
                        .map(|d| d.doc_type == expected_child_type)
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

        if parent_is_draft_or_review && meta.doc_type == DocType::Rfc {
            for child_path in &children {
                if let Some(child) = store.docs.get(child_path) {
                    if child.status == Status::Accepted
                        && child.doc_type == DocType::Story
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

    result
}
