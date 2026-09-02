use crate::engine::config::{
    AttrKind, Config, EdgeDef, RelSelector, Severity, StoreBackend, TypeDef, TypeSelector,
};
use crate::engine::document::{AttrValue, DocMeta, DocType, Status};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Debug)]
pub enum ValidationIssue {
    BrokenLink {
        source: PathBuf,
        target: String,
    },
    UnsatisfiedEdge {
        path: PathBuf,
        edge_name: String,
        from_type: String,
        to: TypeSelector,
        via: RelSelector,
    },
    SupersededParent {
        path: PathBuf,
        parent: PathBuf,
    },
    RejectedParent {
        path: PathBuf,
        parent: PathBuf,
    },
    OrphanedAcceptance {
        path: PathBuf,
        parent: PathBuf,
    },
    AllChildrenAccepted {
        parent: PathBuf,
        children: Vec<PathBuf>,
    },
    UpwardOrphanedAcceptance {
        path: PathBuf,
        parent: PathBuf,
    },
    DuplicateId {
        id: String,
        paths: Vec<PathBuf>,
    },
    InvalidAcSlug {
        path: PathBuf,
        slug: String,
        reason: String,
    },
    RefCountExceeded {
        path: PathBuf,
        count: usize,
        ceiling: usize,
    },
    CrossModuleRefs {
        path: PathBuf,
        module_count: usize,
    },
    OrphanRef {
        path: PathBuf,
        ref_target: String,
    },
    SingletonViolation {
        type_name: String,
        paths: Vec<PathBuf>,
    },
    ParentTypeViolation {
        path: PathBuf,
        type_name: String,
        expected_dir: String,
    },
    ParentTypeNotSingleton {
        type_name: String,
        parent_type: String,
    },
    UnknownRelationship {
        path: PathBuf,
        name: String,
    },
    AttributeKindMismatch {
        path: PathBuf,
        attr: String,
        expected: String,
    },
    AttributeBadEnumValue {
        path: PathBuf,
        attr: String,
        allowed: Vec<String>,
    },
    MissingRequiredAttribute {
        path: PathBuf,
        attr: String,
    },
    UndeclaredAttribute {
        path: PathBuf,
        attr: String,
    },
    UnknownProjectFieldOption {
        path: PathBuf,
        attr: String,
        allowed: Vec<String>,
    },
    StatusAuthorityLifecycleConflict {
        type_name: String,
        status_authority: String,
    },
    StatusAuthorityWrongStore {
        type_name: String,
        store: String,
        status_authority: String,
    },
    StatusAuthorityNotABoard {
        type_name: String,
        status_authority: String,
    },
}

#[derive(Debug, Default)]
pub struct ValidationResult {
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

impl ValidationResult {
    fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

/// How an unsatisfied edge's `via` reads in the finding. A wildcard is a config
/// spelling, not a relationship name, so quoting it back says nothing about what
/// the document is missing. A set is a disjunction (ADR-032), so it reads as the
/// target set beside it does: any one member satisfies the row.
fn via_phrase(via: &RelSelector) -> String {
    let quoted = |name: &String| format!("\"{name}\"");
    match via {
        RelSelector::Any => "any relationship".to_string(),
        RelSelector::Named(names) => match names.as_slice() {
            [only] => quoted(only),
            many => format!(
                "one of: {}",
                many.iter().map(quoted).collect::<Vec<_>>().join(", ")
            ),
        },
    }
}

/// How an unsatisfied edge's target set reads in the finding. The `/lazy` skill
/// reports the same set when it stops at a type boundary, and reads it back in
/// this wording; `shipped_router_phrases_a_target_set_as_the_finding_does`
/// (`src/cli/skills.rs`) holds the two together.
pub(crate) fn to_phrase(to: &TypeSelector) -> String {
    match to {
        TypeSelector::Any => "to a document of any type".to_string(),
        TypeSelector::Types(names) => format!("to one of: {}", names.join(", ")),
    }
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationIssue::BrokenLink { source, target } => {
                write!(f, "broken link: {} -> {}", source.display(), target)
            }
            ValidationIssue::UnsatisfiedEdge {
                path,
                edge_name,
                from_type,
                to,
                via,
            } => {
                write!(
                    f,
                    "unsatisfied edge [{}]: {} ({} needs {} {})",
                    edge_name,
                    path.display(),
                    from_type,
                    via_phrase(via),
                    to_phrase(to)
                )
            }
            ValidationIssue::SupersededParent { path, parent } => {
                write!(
                    f,
                    "implements superseded document: {} -> {}",
                    path.display(),
                    parent.display()
                )
            }
            ValidationIssue::RejectedParent { path, parent } => {
                write!(
                    f,
                    "implements rejected document: {} -> {}",
                    path.display(),
                    parent.display()
                )
            }
            ValidationIssue::OrphanedAcceptance { path, parent } => {
                write!(
                    f,
                    "accepted but parent not accepted: {} -> {}",
                    path.display(),
                    parent.display()
                )
            }
            ValidationIssue::AllChildrenAccepted { parent, children } => {
                write!(
                    f,
                    "all children accepted but parent not accepted: {} ({} children)",
                    parent.display(),
                    children.len()
                )
            }
            ValidationIssue::UpwardOrphanedAcceptance { path, parent } => {
                write!(
                    f,
                    "accepted child but parent not accepted: {} -> {}",
                    path.display(),
                    parent.display()
                )
            }
            ValidationIssue::DuplicateId { id, paths } => {
                let path_strs: Vec<String> =
                    paths.iter().map(|p| p.display().to_string()).collect();
                write!(f, "duplicate id: {} ({})", id, path_strs.join(", "))
            }
            ValidationIssue::InvalidAcSlug { path, slug, reason } => {
                write!(
                    f,
                    "invalid AC slug in {}: \"{}\" ({})",
                    path.display(),
                    slug,
                    reason
                )
            }
            ValidationIssue::RefCountExceeded {
                path,
                count,
                ceiling,
            } => {
                write!(
                    f,
                    "spec {} has {} @ref targets (ceiling {}); consider splitting",
                    path.display(),
                    count,
                    ceiling
                )
            }
            ValidationIssue::CrossModuleRefs { path, module_count } => {
                write!(
                    f,
                    "spec {} refs span {} modules; may cover a cross-cutting concern",
                    path.display(),
                    module_count
                )
            }
            ValidationIssue::OrphanRef { path, ref_target } => {
                write!(
                    f,
                    "orphan ref in {}: @ref {} target does not exist",
                    path.display(),
                    ref_target
                )
            }
            ValidationIssue::SingletonViolation { type_name, paths } => {
                let path_strs: Vec<String> =
                    paths.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "singleton violation: type \"{}\" allows only one document, found {} ({})",
                    type_name,
                    paths.len(),
                    path_strs.join(", ")
                )
            }
            ValidationIssue::ParentTypeViolation {
                path,
                type_name,
                expected_dir,
            } => {
                write!(
                    f,
                    "parent type violation: {} (type \"{}\") must reside within {}",
                    path.display(),
                    type_name,
                    expected_dir
                )
            }
            ValidationIssue::ParentTypeNotSingleton {
                type_name,
                parent_type,
            } => {
                write!(
                    f,
                    "parent type not singleton: type \"{}\" references parent type \"{}\" which is not a singleton",
                    type_name, parent_type
                )
            }
            ValidationIssue::UnknownRelationship { path, name } => {
                write!(
                    f,
                    "unknown relationship \"{}\": {} (not declared in [[relationships]])",
                    name,
                    path.display()
                )
            }
            ValidationIssue::AttributeKindMismatch {
                path,
                attr,
                expected,
            } => {
                write!(
                    f,
                    "attribute \"{}\" in {} is not a valid {}",
                    attr,
                    path.display(),
                    expected
                )
            }
            ValidationIssue::AttributeBadEnumValue {
                path,
                attr,
                allowed,
            } => {
                write!(
                    f,
                    "attribute \"{}\" in {} is not one of the allowed values: {}",
                    attr,
                    path.display(),
                    allowed.join(", ")
                )
            }
            ValidationIssue::MissingRequiredAttribute { path, attr } => {
                write!(
                    f,
                    "missing required attribute \"{}\": {}",
                    attr,
                    path.display()
                )
            }
            ValidationIssue::UndeclaredAttribute { path, attr } => {
                write!(f, "undeclared attribute \"{}\": {}", attr, path.display())
            }
            ValidationIssue::UnknownProjectFieldOption {
                path,
                attr,
                allowed,
            } => {
                write!(
                    f,
                    "project field \"{}\" in {} is not one of the board's options: {}",
                    attr,
                    path.display(),
                    allowed.join(", ")
                )
            }
            ValidationIssue::StatusAuthorityLifecycleConflict {
                type_name,
                status_authority,
            } => {
                write!(
                    f,
                    "status_authority conflict: type \"{}\" declares a lifecycle board {} could not have produced; the nominated board owns this type's states, so a declared lifecycle does not survive fetch",
                    type_name, status_authority
                )
            }
            ValidationIssue::StatusAuthorityWrongStore {
                type_name,
                store,
                status_authority,
            } => {
                write!(
                    f,
                    "status_authority = \"{}\" on type \"{}\" needs store = \"github-issues\", but this type's store is \"{}\": only a github issue can be an item of a Projects v2 board, so no status of this type could ever reach the board",
                    status_authority, type_name, store
                )
            }
            ValidationIssue::StatusAuthorityNotABoard {
                type_name,
                status_authority,
            } => {
                write!(
                    f,
                    "status_authority = \"{}\" on type \"{}\" names no Projects v2 board; the key takes a board number (e.g. \"PROJECT-7\"), and a value that does not silently behaves as no authority at all",
                    status_authority, type_name
                )
            }
        }
    }
}

pub trait Checker {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)>;
}

pub struct BrokenLinkRule;

impl Checker for BrokenLinkRule {
    fn check(
        &self,
        store: &super::store::Store,
        _config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        let id_to_path: HashMap<String, PathBuf> = store
            .docs
            .values()
            .map(|doc| (doc.id.clone(), doc.path.clone()))
            .collect();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }

            for rel in &meta.related {
                let resolved = id_to_path.get(&rel.target).cloned().or_else(|| {
                    let p = PathBuf::from(&rel.target);
                    if store.docs.contains_key(&p) {
                        Some(p)
                    } else {
                        None
                    }
                });

                let Some(target) = resolved else {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::BrokenLink {
                            source: path.clone(),
                            target: rel.target.clone(),
                        },
                    ));
                    continue;
                };

                let Some(parent_doc) = store.docs.get(&target) else {
                    continue;
                };

                // The whole triple, endpoint types included, where the old
                // reading asked only "is this relationship chain anywhere?".
                // That narrows these two findings wherever a row names concrete
                // endpoints, and the narrowing is the point: after STORY-259 a
                // chain row is the only declaration of hierarchy a config has,
                // so "any chain relationship, whatever its endpoints" has
                // nothing left to read. A config that means the broad reading
                // spells it with a wildcard row, which is exactly what
                // `fix --config` writes for a global traversal marker.
                if !store.traversal_walk.walks_chain(
                    meta.doc_type.as_str(),
                    rel.rel_type.as_str(),
                    parent_doc.doc_type.as_str(),
                ) {
                    continue;
                }

                if parent_doc.status == Status::new("rejected") {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::RejectedParent {
                            path: path.clone(),
                            parent: target.clone(),
                        },
                    ));
                } else if parent_doc.status == Status::new("superseded")
                    && meta.status == Status::new("accepted")
                {
                    issues.push((
                        Severity::Warning,
                        ValidationIssue::SupersededParent {
                            path: path.clone(),
                            parent: target.clone(),
                        },
                    ));
                }

                // No second lookup for "is this pair hierarchy": the row that
                // admitted the triple above said both, so the type-pair test
                // and the relationship test have collapsed into one call. Under
                // a wildcard chain row that widens this finding to pairs the
                // rules table never listed -- an accepted document hanging off
                // a draft parent of any type -- and it should: the row says
                // every such link is the chain, and the walk has always agreed.
                // Exempting the unlisted pairs was the two-declarations defect
                // RFC-067 §Problem.1 names, not a rule anyone wrote.
                if meta.status == Status::new("accepted")
                    && parent_doc.status != Status::new("accepted")
                {
                    issues.push((
                        Severity::Warning,
                        ValidationIssue::OrphanedAcceptance {
                            path: path.clone(),
                            parent: target.clone(),
                        },
                    ));
                }
            }
        }

        issues
    }
}

/// The demands that still speak for a document, out of the rows that apply to
/// it. ADR-031 ranges resolution over the rows that *state* `required`: a row
/// that omits it declares the edge legal and takes no part at any specificity,
/// so a documentation-only narrow row cannot silence a broad demand. Among the
/// demanding rows a strictly more specific one displaces every row it overlaps.
/// What survives is not the maximal-specificity rows: two rows that overlap
/// nothing displace nothing, so both survive at whatever specificity each has.
/// Demanding rows of equal specificity that disagree never reach here: the
/// config rejects them at load, so resolution never has to break a tie.
fn undisplaced_demands<'a>(rows: &[&'a EdgeDef]) -> Vec<(&'a EdgeDef, &'a Severity)> {
    let demands: Vec<(&'a EdgeDef, &'a Severity)> = rows
        .iter()
        .filter_map(|row| row.required.as_ref().map(|severity| (*row, severity)))
        .collect();

    demands
        .iter()
        .filter(|(row, _)| {
            !demands
                .iter()
                .any(|(other, _)| other.specificity() > row.specificity() && other.overlaps(row))
        })
        .copied()
        .collect()
}

pub struct RequiredEdgeRule;

impl Checker for RequiredEdgeRule {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        let id_to_path: HashMap<String, PathBuf> = store
            .docs
            .values()
            .map(|doc| (doc.id.clone(), doc.path.clone()))
            .collect();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }

            let doc_type = meta.doc_type.as_str();
            // `from` gates the document as a whole: a row that does not apply
            // must not read as unsatisfied against an empty `related` list, and
            // it has no standing to displace a row that does apply.
            let applicable: Vec<&EdgeDef> = config
                .edges
                .iter()
                .filter(|edge| edge.from.matches(doc_type))
                .collect();

            for (edge, severity) in undisplaced_demands(&applicable) {
                // `from` already matched to get here, so only `via` and `to`
                // are left to decide. The row's own `via` decides, never "any
                // chain relationship": that is how `[[rules]]` lets `targets`
                // satisfy a rule that meant `implements` (RFC-067 §Problem.1).
                let satisfied = meta.related.iter().any(|r| {
                    let resolved = id_to_path
                        .get(&r.target)
                        .cloned()
                        .unwrap_or_else(|| PathBuf::from(&r.target));
                    // `to = "*"` means any document, not any string in
                    // `related`: a target that resolves to nothing already has
                    // its own broken-link finding.
                    store.docs.get(&resolved).is_some_and(|d| {
                        edge.matches_target(r.rel_type.as_str(), d.doc_type.as_str())
                    })
                });

                if !satisfied {
                    issues.push((
                        severity.clone(),
                        ValidationIssue::UnsatisfiedEdge {
                            path: path.clone(),
                            edge_name: edge.name.clone(),
                            // The finding is about this one document, whose type
                            // is known; `edge.from` may list several.
                            from_type: doc_type.to_string(),
                            to: edge.to.clone(),
                            via: edge.via.clone(),
                        },
                    ));
                }
            }
        }

        issues
    }
}

pub struct StatusConsistencyRule;

impl Checker for StatusConsistencyRule {
    fn check(
        &self,
        store: &super::store::Store,
        _config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        for (parent_path, meta) in &store.docs {
            // One child type at a time, as the rules table's parent/child pairs
            // made it: `AllChildrenAccepted` counts over a single type's
            // children, so a parent whose stories are all accepted and whose
            // bugs are not still carries the finding for its stories. The pairs
            // now come from the edge table's reverse index (STORY-257), the same
            // one the chain walk and the prompt context read.
            for child_type in store.traversal_walk.child_types_for(meta.doc_type.as_str()) {
                let children: Vec<PathBuf> = store
                    .reverse_links
                    .get(parent_path)
                    .into_iter()
                    .flatten()
                    .filter(|link| {
                        store.docs.get(&link.endpoint).is_some_and(|child| {
                            child.doc_type.as_str() == child_type
                                && !child.validate_ignore
                                && store.traversal_walk.walks_chain(
                                    child.doc_type.as_str(),
                                    link.rel_type.as_str(),
                                    meta.doc_type.as_str(),
                                )
                        })
                    })
                    .map(|link| link.endpoint.clone())
                    .collect();

                if children.is_empty() {
                    continue;
                }

                let parent_is_draft_or_review =
                    meta.status == Status::new("draft") || meta.status == Status::new("review");

                if !parent_is_draft_or_review {
                    continue;
                }

                let all_accepted = children.iter().all(|cp| {
                    store
                        .docs
                        .get(cp)
                        .map(|d| d.status == Status::new("accepted"))
                        .unwrap_or(false)
                });

                if all_accepted {
                    issues.push((
                        Severity::Warning,
                        ValidationIssue::AllChildrenAccepted {
                            parent: parent_path.clone(),
                            children,
                        },
                    ));
                    continue;
                }

                for child_path in &children {
                    let Some(child) = store.docs.get(child_path) else {
                        continue;
                    };
                    if child.status == Status::new("accepted") {
                        issues.push((
                            Severity::Warning,
                            ValidationIssue::UpwardOrphanedAcceptance {
                                path: child_path.clone(),
                                parent: parent_path.clone(),
                            },
                        ));
                    }
                }
            }
        }

        issues
    }
}

pub struct DuplicateIdRule;

impl Checker for DuplicateIdRule {
    fn check(
        &self,
        store: &super::store::Store,
        _config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut id_map: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for (path, meta) in &store.docs {
            if meta.validate_ignore || meta.id.is_empty() {
                continue;
            }
            id_map
                .entry(meta.id.clone())
                .or_default()
                .push(path.clone());
        }

        let mut issues = Vec::new();
        for (id, mut paths) in id_map {
            if paths.len() <= 1 {
                continue;
            }
            paths.sort();
            issues.push((Severity::Error, ValidationIssue::DuplicateId { id, paths }));
        }

        issues
    }
}

pub struct AcSlugFormatRule;

static AC_HEADING_PREFIX: &str = "### AC:";
static AC_SLUG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap());

impl AcSlugFormatRule {
    fn is_spec_doc(meta: &DocMeta) -> bool {
        meta.doc_type == DocType::new(DocType::SPEC)
    }

    fn read_body(path: &std::path::Path, store: &super::store::Store) -> Option<String> {
        let full_path = store.root().join(path);
        let content = std::fs::read_to_string(&full_path).ok()?;
        DocMeta::extract_body(&content).ok()
    }

    fn validate_slugs(path: &std::path::Path, body: &str) -> Vec<(Severity, ValidationIssue)> {
        let slug_re = &*AC_SLUG_RE;
        let mut issues = Vec::new();
        let mut seen_slugs = HashSet::new();

        for line in body.lines() {
            let Some(rest) = line.strip_prefix(AC_HEADING_PREFIX) else {
                continue;
            };
            let slug = rest.trim();

            if slug.is_empty() {
                issues.push((
                    Severity::Warning,
                    ValidationIssue::InvalidAcSlug {
                        path: path.to_path_buf(),
                        slug: String::new(),
                        reason: "empty AC slug".to_string(),
                    },
                ));
                continue;
            }

            if !seen_slugs.insert(slug.to_string()) {
                issues.push((
                    Severity::Warning,
                    ValidationIssue::InvalidAcSlug {
                        path: path.to_path_buf(),
                        slug: slug.to_string(),
                        reason: "duplicate AC slug".to_string(),
                    },
                ));
                continue;
            }

            if !slug_re.is_match(slug) {
                issues.push((
                    Severity::Warning,
                    ValidationIssue::InvalidAcSlug {
                        path: path.to_path_buf(),
                        slug: slug.to_string(),
                        reason: "slug must be lowercase kebab-case (a-z0-9 separated by hyphens)"
                            .to_string(),
                    },
                ));
            }
        }

        issues
    }
}

impl Checker for AcSlugFormatRule {
    fn check(
        &self,
        store: &super::store::Store,
        _config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }
            if !Self::is_spec_doc(meta) {
                continue;
            }
            let Some(body) = Self::read_body(path, store) else {
                continue;
            };
            issues.extend(Self::validate_slugs(path, &body));
        }

        issues
    }
}

pub struct RefScopeRule;

static REF_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(super::refs::REF_PATTERN).unwrap());

impl RefScopeRule {
    fn is_spec(meta: &DocMeta) -> bool {
        meta.doc_type == DocType::new(DocType::SPEC)
    }

    fn module_prefix(ref_path: &str) -> Option<String> {
        let parts: Vec<&str> = ref_path.split('/').collect();
        if parts.len() >= 2 {
            Some(format!("{}/{}", parts[0], parts[1]))
        } else {
            None
        }
    }
}

impl Checker for RefScopeRule {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }
            if !Self::is_spec(meta) {
                continue;
            }
            let full_path = store.root().join(path);
            let Ok(content) = std::fs::read_to_string(&full_path) else {
                continue;
            };
            let Ok(body) = DocMeta::extract_body(&content) else {
                continue;
            };

            let ref_re = &*REF_RE;
            let ref_paths: HashSet<String> = ref_re
                .captures_iter(&body)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();

            let count = ref_paths.len();
            if count > config.ref_count_ceiling {
                issues.push((
                    Severity::Warning,
                    ValidationIssue::RefCountExceeded {
                        path: path.clone(),
                        count,
                        ceiling: config.ref_count_ceiling,
                    },
                ));
            }

            let modules: HashSet<String> = ref_paths
                .iter()
                .filter_map(|p| Self::module_prefix(p))
                .collect();

            if modules.len() > 3 {
                issues.push((
                    Severity::Warning,
                    ValidationIssue::CrossModuleRefs {
                        path: path.clone(),
                        module_count: modules.len(),
                    },
                ));
            }
        }

        issues
    }
}

pub struct OrphanRefRule;

impl Checker for OrphanRefRule {
    fn check(
        &self,
        store: &super::store::Store,
        _config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }
            if !RefScopeRule::is_spec(meta) {
                continue;
            }
            let full_path = store.root().join(path);
            let Ok(content) = std::fs::read_to_string(&full_path) else {
                continue;
            };
            let Ok(body) = DocMeta::extract_body(&content) else {
                continue;
            };

            let ref_re = &*REF_RE;
            for cap in ref_re.captures_iter(&body) {
                let Some(ref_path) = cap.get(1).map(|m| m.as_str()) else {
                    continue;
                };
                if !store.root().join(ref_path).exists() {
                    issues.push((
                        Severity::Warning,
                        ValidationIssue::OrphanRef {
                            path: path.clone(),
                            ref_target: ref_path.to_string(),
                        },
                    ));
                }
            }
        }

        issues
    }
}

pub struct TypeConstraintChecker;

impl Checker for TypeConstraintChecker {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        for type_def in &config.documents.types {
            if type_def.singleton {
                let docs = store.list(&super::store::Filter {
                    doc_type: Some(DocType::new(&type_def.name)),
                    ..Default::default()
                });
                if docs.len() > 1 {
                    let mut paths: Vec<PathBuf> = docs.iter().map(|d| d.path.clone()).collect();
                    paths.sort();
                    issues.push((
                        Severity::Error,
                        ValidationIssue::SingletonViolation {
                            type_name: type_def.name.clone(),
                            paths,
                        },
                    ));
                }
            }

            if let Some(board_id) = &type_def.status_authority {
                // Only a github issue can be an item of a Projects v2 board. On
                // any other store the key resolves and rejects every `--status`
                // against the board's columns offline while no board write can
                // ever happen.
                if type_def.store != StoreBackend::GithubIssues {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::StatusAuthorityWrongStore {
                            type_name: type_def.name.clone(),
                            store: type_def.store.to_string(),
                            status_authority: board_id.clone(),
                        },
                    ));
                }
                // A value naming no board number behaves as "no authority", which
                // also suppresses the open/closed status mapping: every doc of the
                // type then caches with an empty status.
                if crate::engine::store_dispatch::board_number(board_id).is_err() {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::StatusAuthorityNotABoard {
                            type_name: type_def.name.clone(),
                            status_authority: board_id.clone(),
                        },
                    ));
                }
                if declared_lifecycle_cannot_be_the_boards(&store.root, type_def, board_id) {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::StatusAuthorityLifecycleConflict {
                            type_name: type_def.name.clone(),
                            status_authority: board_id.clone(),
                        },
                    ));
                }
            }

            let Some(ref parent_type_name) = type_def.parent_type else {
                continue;
            };

            let Some(parent_type_def) = config.type_by_name(parent_type_name) else {
                continue;
            };

            if !parent_type_def.singleton {
                issues.push((
                    Severity::Error,
                    ValidationIssue::ParentTypeNotSingleton {
                        type_name: type_def.name.clone(),
                        parent_type: parent_type_name.clone(),
                    },
                ));
                continue;
            }

            let docs = store.list(&super::store::Filter {
                doc_type: Some(DocType::new(&type_def.name)),
                ..Default::default()
            });
            for doc in docs {
                if !doc.path.starts_with(&parent_type_def.dir) {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::ParentTypeViolation {
                            path: doc.path.clone(),
                            type_name: type_def.name.clone(),
                            expected_dir: parent_type_def.dir.clone(),
                        },
                    ));
                }
            }
        }

        issues
    }
}

/// Whether a `status_authority` type's declared `lifecycle` is provably not the
/// nominated board's own.
///
/// `fetch` persists the board's `Status` columns into the type's `lifecycle`, so
/// one fetch later a board-derived lifecycle and a hand-declared one are the same
/// bytes. "Both keys set" therefore cannot be the conflict -- that is the ordinary
/// post-fetch state of every board-bound type. Only shapes a board can never
/// produce count:
///
/// - Declared `edges`. A board carries column order and no transition rules, so
///   `status_lifecycle` always derives an edgeless lifecycle; any edge set is
///   hand-written and will be dropped by the next fetch.
/// - States the cached snapshot contradicts: the board's `Status` options are
///   known and the declared states are not them (a lifecycle hand-edited after a
///   fetch).
///
/// An absent snapshot contradicts nothing, matching [`check_project_field`]'s
/// offline posture -- the fetch itself is the backstop.
fn declared_lifecycle_cannot_be_the_boards(
    root: &std::path::Path,
    type_def: &TypeDef,
    board_id: &str,
) -> bool {
    if type_def.lifecycle.states.is_empty() {
        return false;
    }
    if !type_def.lifecycle.edges.is_empty() {
        return true;
    }
    let Ok(number) = crate::engine::store_dispatch::board_number(board_id) else {
        return false;
    };
    crate::engine::gh_schema::GhSchemaSnapshot::load(root)
        .status_lifecycle(number)
        .is_some_and(|board| board.states != type_def.lifecycle.states)
}

pub struct UnknownRelationshipRule;

impl Checker for UnknownRelationshipRule {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        // A relation is known if its keyword matches a declared relationship
        // `name` or a declared `inverse`; `relationship_keywords` is the
        // registry's source of truth for both. A stored inverse keyword (e.g.
        // `targeted-by`, the inverse of `targets`) must validate clean.
        let known: std::collections::HashSet<String> =
            config.relationship_keywords().into_iter().collect();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }
            for rel in &meta.related {
                if !known.contains(rel.rel_type.as_str()) {
                    issues.push((
                        Severity::Error,
                        ValidationIssue::UnknownRelationship {
                            path: path.clone(),
                            name: rel.rel_type.as_str().to_string(),
                        },
                    ));
                }
            }
        }

        issues
    }
}

pub struct AttributeSchemaChecker;

impl AttributeSchemaChecker {
    /// The YAML value backing an attribute, whether it is still a raw
    /// (un-coerced) capture or has already been coerced to a typed value. Lets
    /// the checker re-validate regardless of which parse path produced the doc.
    fn as_yaml(value: &AttrValue) -> serde_yaml::Value {
        match value {
            AttrValue::Raw(v) => v.clone(),
            AttrValue::Int(i) => (*i).into(),
            AttrValue::Float(f) => (*f).into(),
            AttrValue::Str(s) => s.clone().into(),
            AttrValue::Bool(b) => (*b).into(),
            AttrValue::Date(d) => d.to_string().into(),
        }
    }
}

impl Checker for AttributeSchemaChecker {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let mut issues = Vec::new();

        for (path, meta) in &store.docs {
            if meta.validate_ignore {
                continue;
            }
            let Some(type_def) = config.type_by_name(meta.doc_type.as_str()) else {
                continue;
            };

            for def in &type_def.attributes {
                match meta.attributes.get(&def.name) {
                    None => {
                        if def.required {
                            issues.push((
                                Severity::Error,
                                ValidationIssue::MissingRequiredAttribute {
                                    path: path.clone(),
                                    attr: def.name.clone(),
                                },
                            ));
                        }
                    }
                    Some(value) => {
                        let yaml = Self::as_yaml(value);
                        if crate::engine::document::coerce_attr(&yaml, def).is_none() {
                            let issue = if def.kind == AttrKind::Enum {
                                ValidationIssue::AttributeBadEnumValue {
                                    path: path.clone(),
                                    attr: def.name.clone(),
                                    allowed: def.values.clone(),
                                }
                            } else {
                                ValidationIssue::AttributeKindMismatch {
                                    path: path.clone(),
                                    attr: def.name.clone(),
                                    expected: format!("{:?}", def.kind).to_lowercase(),
                                }
                            };
                            issues.push((Severity::Error, issue));
                        }
                    }
                }
            }

            for (key, value) in &meta.attributes {
                // Dynamic per-board project fields are namespaced
                // `PROJECT-n.<field>`; they bypass the declared-AttrDef surface
                // and are validated against the gh-schema snapshot instead.
                if let Some((number, field)) =
                    crate::engine::store_dispatch::parse_project_field_key(key)
                {
                    if let Some(issue) =
                        check_project_field(&store.root, path, key, number, field, value)
                    {
                        issues.push((Severity::Error, issue));
                    }
                    continue;
                }
                if !type_def.attributes.iter().any(|d| &d.name == key) {
                    issues.push((
                        Severity::Warning,
                        ValidationIssue::UndeclaredAttribute {
                            path: path.clone(),
                            attr: key.clone(),
                        },
                    ));
                }
            }
        }

        issues
    }
}

/// Offline snapshot-backed check for a single `PROJECT-n.<field>` attribute.
/// SingleSelect/Iteration values must be in the board's option set; Number/Date
/// fields are shape-checked against the value's kind; Text/unknown always pass.
/// An unresolvable field or board (snapshot absent) yields no issue: the live
/// mutation is the backstop.
fn check_project_field(
    root: &std::path::Path,
    path: &std::path::Path,
    attr: &str,
    project_number: u64,
    field_name: &str,
    value: &AttrValue,
) -> Option<ValidationIssue> {
    let snapshot = crate::engine::gh_schema::GhSchemaSnapshot::load(root);
    let field_id = snapshot.field_id(project_number, field_name)?;
    let data_type = snapshot
        .project_fields
        .iter()
        .find(|f| f.project_number == project_number && f.field_name == field_name)
        .map(|f| f.data_type.as_str())?;

    let as_str = || match value {
        AttrValue::Str(s) => Some(s.clone()),
        _ => None,
    };

    match data_type {
        "SINGLE_SELECT" => {
            let v = as_str()?;
            if snapshot.option_id(field_id, &v).is_some() {
                None
            } else {
                let allowed = snapshot
                    .single_select_options
                    .iter()
                    .filter(|o| o.field_id == field_id)
                    .map(|o| o.name.clone())
                    .collect();
                Some(ValidationIssue::UnknownProjectFieldOption {
                    path: path.to_path_buf(),
                    attr: attr.to_string(),
                    allowed,
                })
            }
        }
        "ITERATION" => {
            let v = as_str()?;
            if snapshot.iteration_id(field_id, &v).is_some() {
                None
            } else {
                let allowed = snapshot
                    .iterations
                    .iter()
                    .filter(|i| i.field_id == field_id)
                    .map(|i| i.title.clone())
                    .collect();
                Some(ValidationIssue::UnknownProjectFieldOption {
                    path: path.to_path_buf(),
                    attr: attr.to_string(),
                    allowed,
                })
            }
        }
        "NUMBER" => match value {
            AttrValue::Int(_) | AttrValue::Float(_) => None,
            _ => Some(ValidationIssue::UnknownProjectFieldOption {
                path: path.to_path_buf(),
                attr: attr.to_string(),
                allowed: vec!["<number>".to_string()],
            }),
        },
        "DATE" => match value {
            AttrValue::Date(_) => None,
            _ => Some(ValidationIssue::UnknownProjectFieldOption {
                path: path.to_path_buf(),
                attr: attr.to_string(),
                allowed: vec!["<date>".to_string()],
            }),
        },
        _ => None,
    }
}

fn default_checkers() -> Vec<Box<dyn Checker>> {
    vec![
        Box::new(BrokenLinkRule),
        Box::new(RequiredEdgeRule),
        Box::new(StatusConsistencyRule),
        Box::new(DuplicateIdRule),
        Box::new(AcSlugFormatRule),
        Box::new(RefScopeRule),
        Box::new(OrphanRefRule),
        Box::new(TypeConstraintChecker),
        Box::new(UnknownRelationshipRule),
        Box::new(AttributeSchemaChecker),
    ]
}

pub fn validate_full(store: &super::store::Store, config: &Config) -> ValidationResult {
    let mut result = ValidationResult::default();

    for checker in default_checkers() {
        let issues = checker.check(store, config);
        let mut partial = ValidationResult::default();
        for (severity, issue) in issues {
            match severity {
                Severity::Error => partial.errors.push(issue),
                Severity::Warning => partial.warnings.push(issue),
            }
        }
        result.merge(partial);
    }

    result
}

#[cfg(test)]
mod attr_schema_tests {
    use super::*;
    use crate::engine::config::{AttrDef, AttrKind};
    use crate::engine::document::AttrValue;
    use crate::engine::traversal::TraversalWalk;
    use chrono::NaiveDate;
    use std::collections::{BTreeMap, HashMap};

    fn config_with_story_attrs(attrs: Vec<AttrDef>) -> Config {
        let mut config = Config::default();
        let story = config
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == "story")
            .unwrap();
        story.attributes = attrs;
        config
    }

    fn story_doc(attributes: BTreeMap<String, AttrValue>) -> DocMeta {
        DocMeta {
            path: PathBuf::from("docs/stories/STORY-001.md"),
            title: "S".to_string(),
            doc_type: DocType::new("story"),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            id: "STORY-001".to_string(),
            attributes,
        }
    }

    fn store_with(doc: DocMeta) -> super::super::store::Store {
        let mut docs = HashMap::new();
        docs.insert(doc.path.clone(), doc);
        super::super::store::Store {
            root: PathBuf::from("."),
            docs,
            forward_links: HashMap::new(),
            reverse_links: HashMap::new(),
            children: HashMap::new(),
            parent_of: HashMap::new(),
            parse_errors: Vec::new(),
            // No hierarchy declared, in either place that declares one: every
            // assertion in this module is about one document's attributes.
            traversal_walk: TraversalWalk::default(),
            body_cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn attr(name: &str, kind: AttrKind, required: bool, values: &[&str]) -> AttrDef {
        AttrDef {
            name: name.to_string(),
            kind,
            required,
            values: values.iter().map(|s| s.to_string()).collect(),
        }
    }

    // AC3: a value of the wrong kind is a validation error.
    #[test]
    fn wrong_kind_is_error() {
        let config = config_with_story_attrs(vec![attr("estimate", AttrKind::Int, false, &[])]);
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "estimate".to_string(),
            AttrValue::Raw(serde_yaml::Value::String("notanumber".to_string())),
        );
        let result = validate_full(&store_with(story_doc(attrs)), &config);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::AttributeKindMismatch { .. })));
    }

    // AC3: an enum value outside the declared set is a validation error.
    #[test]
    fn bad_enum_value_is_error() {
        let config = config_with_story_attrs(vec![attr(
            "priority",
            AttrKind::Enum,
            false,
            &["low", "high"],
        )]);
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "priority".to_string(),
            AttrValue::Raw(serde_yaml::Value::String("urgent".to_string())),
        );
        let result = validate_full(&store_with(story_doc(attrs)), &config);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::AttributeBadEnumValue { .. })));
    }

    // AC3: a missing required attribute is a validation error.
    #[test]
    fn missing_required_is_error() {
        let config = config_with_story_attrs(vec![attr("owner", AttrKind::Str, true, &[])]);
        let result = validate_full(&store_with(story_doc(BTreeMap::new())), &config);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::MissingRequiredAttribute { .. })));
    }

    // AC4: an undeclared key is a validation warning (not an error).
    #[test]
    fn undeclared_key_is_warning() {
        let config = config_with_story_attrs(vec![]);
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "mystery".to_string(),
            AttrValue::Raw(serde_yaml::Value::String("x".to_string())),
        );
        let result = validate_full(&store_with(story_doc(attrs)), &config);
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, ValidationIssue::UndeclaredAttribute { .. })));
        assert!(!result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::UndeclaredAttribute { .. })));
    }

    // --- ITERATION-217: PROJECT-n.<field> snapshot-backed validation (AC6) ---

    fn store_with_root(doc: DocMeta, root: PathBuf) -> super::super::store::Store {
        let mut docs = HashMap::new();
        docs.insert(doc.path.clone(), doc);
        super::super::store::Store {
            root,
            docs,
            forward_links: HashMap::new(),
            reverse_links: HashMap::new(),
            children: HashMap::new(),
            parent_of: HashMap::new(),
            parse_errors: Vec::new(),
            // As `store_with` above: no hierarchy, none read.
            traversal_walk: TraversalWalk::default(),
            body_cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn write_status_snapshot(root: &std::path::Path) {
        use crate::engine::gh_schema::{GhSchemaSnapshot, OptionId, ProjectFieldId};
        let snapshot = GhSchemaSnapshot {
            project_fields: vec![ProjectFieldId {
                project_number: 1,
                field_name: "Status".into(),
                id: "F_status".into(),
                data_type: "SINGLE_SELECT".into(),
            }],
            single_select_options: vec![OptionId {
                field_id: "F_status".into(),
                name: "In Progress".into(),
                id: "opt_inprog".into(),
            }],
            ..Default::default()
        };
        snapshot.save(root).unwrap();
    }

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyspec-validation-iter217-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // AC6: an option not in the snapshot is an Error (UnknownProjectFieldOption),
    // never just an undeclared-key warning.
    #[test]
    fn unknown_project_option_is_error() {
        let root = tmp_root("unknown_option");
        write_status_snapshot(&root);
        let config = config_with_story_attrs(vec![]);
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "PROJECT-1.Status".to_string(),
            AttrValue::Str("Frozen".to_string()),
        );
        let result = validate_full(&store_with_root(story_doc(attrs), root), &config);
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationIssue::UnknownProjectFieldOption { .. })),
            "expected UnknownProjectFieldOption error, got: {:?}",
            result.errors
        );
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| matches!(w, ValidationIssue::UndeclaredAttribute { .. })),
            "PROJECT-n.<field> must bypass the undeclared-key warning"
        );
    }

    // AC6 (positive): a known option produces no error and no undeclared warning.
    #[test]
    fn known_project_option_is_clean() {
        let root = tmp_root("known_option");
        write_status_snapshot(&root);
        let config = config_with_story_attrs(vec![]);
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "PROJECT-1.Status".to_string(),
            AttrValue::Str("In Progress".to_string()),
        );
        let result = validate_full(&store_with_root(story_doc(attrs), root), &config);
        assert!(!result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::UnknownProjectFieldOption { .. })));
        assert!(!result
            .warnings
            .iter()
            .any(|w| matches!(w, ValidationIssue::UndeclaredAttribute { .. })));
    }

    // A correctly-typed declared value produces neither error nor warning.
    #[test]
    fn valid_declared_attribute_is_clean() {
        let config = config_with_story_attrs(vec![attr("estimate", AttrKind::Int, true, &[])]);
        let mut attrs = BTreeMap::new();
        attrs.insert("estimate".to_string(), AttrValue::Int(5));
        let result = validate_full(&store_with(story_doc(attrs)), &config);
        assert!(!result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::AttributeKindMismatch { .. }
                | ValidationIssue::MissingRequiredAttribute { .. }
        )));
        assert!(!result
            .warnings
            .iter()
            .any(|w| matches!(w, ValidationIssue::UndeclaredAttribute { .. })));
    }
}

#[cfg(test)]
mod unknown_relationship_tests {
    use super::*;
    use crate::engine::config::RelationshipDef;
    use crate::engine::document::{Relation, RelationType};
    use crate::engine::traversal::TraversalWalk;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn doc_with_relation(rel_type: &str) -> DocMeta {
        DocMeta {
            path: PathBuf::from("docs/milestones/MILESTONE-001.md"),
            title: "M".to_string(),
            doc_type: DocType::new("milestone"),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![Relation {
                rel_type: RelationType::new(rel_type),
                target: "STORY-1".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            id: "MILESTONE-001".to_string(),
            attributes: Default::default(),
        }
    }

    fn store_with(doc: DocMeta) -> super::super::store::Store {
        let mut docs = HashMap::new();
        docs.insert(doc.path.clone(), doc);
        super::super::store::Store {
            root: PathBuf::from("."),
            docs,
            forward_links: HashMap::new(),
            reverse_links: HashMap::new(),
            children: HashMap::new(),
            parent_of: HashMap::new(),
            parse_errors: Vec::new(),
            // No hierarchy declared: this module asserts only on the
            // relationship vocabulary a document's `related` keys are checked
            // against, which no traversal role takes part in.
            traversal_walk: TraversalWalk::default(),
            body_cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    // Production-shaped config: `targeted-by` is the declared *inverse* of the
    // `targets` milestone relationship, never a top-level name.
    fn config_with_targets() -> Config {
        let mut config = Config::default();
        config.relationships.push(RelationshipDef {
            name: "targets".to_string(),
            inverse: Some("targeted-by".to_string()),
            github_native: Some("milestone".to_string()),
            traversal: None,
        });
        config
    }

    // A stored relation keyed by a declared INVERSE keyword validates clean.
    #[test]
    fn unknown_relationship_accepts_declared_inverse_keyword() {
        let config = config_with_targets();
        let issues =
            UnknownRelationshipRule.check(&store_with(doc_with_relation("targeted-by")), &config);
        assert!(
            issues.is_empty(),
            "stored inverse keyword `targeted-by` must validate clean, got: {:?}",
            issues
        );
    }

    // A genuinely undeclared keyword is still flagged as unknown.
    #[test]
    fn unknown_relationship_flags_undeclared_keyword() {
        let config = config_with_targets();
        let issues =
            UnknownRelationshipRule.check(&store_with(doc_with_relation("bogus-rel")), &config);
        assert!(
            issues
                .iter()
                .any(|(_, i)| matches!(i, ValidationIssue::UnknownRelationship { .. })),
            "undeclared keyword `bogus-rel` must be flagged, got: {:?}",
            issues
        );
    }
}

#[cfg(test)]
mod status_authority_tests {
    use super::*;
    use crate::engine::gh_schema::{GhSchemaSnapshot, OptionId, ProjectFieldId};
    use tempfile::TempDir;

    /// A one-type project: a `github-issues`-backed `ticket` whose `[[types]]`
    /// block carries `keys` verbatim, so each test declares only the
    /// `status_authority`/`lifecycle` pair it is about.
    fn ticket_config(keys: &str) -> Config {
        ticket_config_stored("github-issues", keys)
    }

    /// The same one-type project on an arbitrary `store`, for the types that have
    /// no board to nominate.
    fn ticket_config_stored(store: &str, keys: &str) -> Config {
        Config::parse(&format!(
            r#"[naming]
pattern = "{{type}}-{{n:03}}-{{title}}.md"

[templates]
dir = ".lazyspec/templates"

[github]
repo = "octo-org/repo"

[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
store = "{store}"
{keys}

[[relationships]]
name = "related-to"
"#
        ))
        .expect("fixture config parses")
    }

    /// Board 7's `Status` single-select carrying `options` in board order,
    /// written to the project's gh-schema cache.
    fn write_board_7_status_snapshot(root: &std::path::Path, options: &[&str]) {
        let snapshot = GhSchemaSnapshot {
            project_fields: vec![ProjectFieldId {
                project_number: 7,
                field_name: "Status".into(),
                id: "F_b7_status".into(),
                data_type: "SINGLE_SELECT".into(),
            }],
            single_select_options: options
                .iter()
                .map(|name| OptionId {
                    field_id: "F_b7_status".into(),
                    name: (*name).into(),
                    id: format!("opt_{}", name.to_lowercase().replace(' ', "_")),
                })
                .collect(),
            ..Default::default()
        };
        snapshot.save(root).unwrap();
    }

    const BOARD_7_OPTIONS: [&str; 4] = ["Ready To Start", "In Progress", "Review", "Done"];

    fn issues(config: &Config, root: &std::path::Path) -> Vec<(Severity, ValidationIssue)> {
        let store = super::super::store::Store::load(root, config).unwrap();
        TypeConstraintChecker.check(&store, config)
    }

    // A board carries column order but no transition rules, so declared `edges`
    // can only be hand-written -- and fetch will overwrite them.
    #[test]
    fn status_authority_with_declared_edges_is_an_error() {
        let tmp = TempDir::new().unwrap();
        let config = ticket_config(
            r#"status_authority = "PROJECT-7"
lifecycle = { states = ["review", "done"], edges = [{ from = "review", to = "done" }] }"#,
        );

        let found = issues(&config, tmp.path());

        assert_eq!(found.len(), 1, "got: {:?}", found);
        let (severity, issue) = &found[0];
        assert!(matches!(severity, Severity::Error));
        let rendered = issue.to_string();
        assert!(rendered.contains("status_authority"), "got: {rendered}");
        assert!(rendered.contains("lifecycle"), "got: {rendered}");
        assert!(rendered.contains("ticket"), "got: {rendered}");
    }

    // The post-fetch steady state: `persist_board_lifecycles` has written board
    // 7's columns into `lifecycle`, so both keys are set. This is the regression
    // guard against flagging "both keys set" -- a persisted lifecycle is
    // indistinguishable from a hand-declared edgeless one, so firing here would
    // fail every board-bound project's `validate` after its first fetch.
    #[test]
    fn status_authority_with_board_derived_lifecycle_is_not_an_error() {
        let tmp = TempDir::new().unwrap();
        write_board_7_status_snapshot(tmp.path(), &BOARD_7_OPTIONS);
        let config = ticket_config(
            r#"status_authority = "PROJECT-7"
lifecycle = { states = ["ready to start", "in progress", "review", "done"], edges = [] }"#,
        );

        let found = issues(&config, tmp.path());

        assert!(found.is_empty(), "got: {:?}", found);
    }

    #[test]
    fn status_authority_with_lifecycle_diverging_from_the_board_is_an_error() {
        let tmp = TempDir::new().unwrap();
        write_board_7_status_snapshot(tmp.path(), &BOARD_7_OPTIONS);
        let config = ticket_config(
            r#"status_authority = "PROJECT-7"
lifecycle = { states = ["ready to start", "in progress", "review", "done", "blocked"], edges = [] }"#,
        );

        let found = issues(&config, tmp.path());

        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert!(matches!(
            found[0].1,
            ValidationIssue::StatusAuthorityLifecycleConflict { .. }
        ));
    }

    // Offline posture: with no cached snapshot the board's columns are unknown,
    // so an edgeless lifecycle cannot be contradicted (mirrors
    // `check_project_field`, which also yields nothing without a snapshot).
    #[test]
    fn status_authority_with_no_snapshot_and_edgeless_lifecycle_is_not_an_error() {
        let tmp = TempDir::new().unwrap();
        let config = ticket_config(
            r#"status_authority = "PROJECT-7"
lifecycle = { states = ["triage", "shipped"], edges = [] }"#,
        );

        let found = issues(&config, tmp.path());

        assert!(found.is_empty(), "got: {:?}", found);
    }

    // Only a github issue can be an item of a Projects v2 board, so on any other
    // store the key is unsatisfiable: every `--status` would be resolved and
    // rejected against the board's columns offline while no board write could ever
    // happen.
    #[test]
    fn status_authority_on_a_type_that_is_not_github_issues_is_an_error() {
        for store in ["filesystem", "github-milestones", "clickup-tasks"] {
            let tmp = TempDir::new().unwrap();
            let config = ticket_config_stored(store, r#"status_authority = "PROJECT-7""#);

            let found = issues(&config, tmp.path());

            assert_eq!(found.len(), 1, "{store}, got: {found:?}");
            let (severity, issue) = &found[0];
            assert!(matches!(severity, Severity::Error));
            let rendered = issue.to_string();
            assert!(rendered.contains("status_authority"), "got: {rendered}");
            assert!(rendered.contains("ticket"), "got: {rendered}");
            assert!(rendered.contains("PROJECT-7"), "got: {rendered}");
            assert!(rendered.contains(store), "got: {rendered}");
        }
    }

    // A value naming no board number silently behaves as "no authority": it also
    // suppresses the open/closed status mapping, so every doc of the type caches
    // with an empty status.
    #[test]
    fn status_authority_that_names_no_board_number_is_an_error() {
        let tmp = TempDir::new().unwrap();
        let config = ticket_config(r#"status_authority = "PROJECT-seven""#);

        let found = issues(&config, tmp.path());

        assert_eq!(found.len(), 1, "got: {:?}", found);
        let (severity, issue) = &found[0];
        assert!(matches!(severity, Severity::Error));
        assert!(matches!(
            issue,
            ValidationIssue::StatusAuthorityNotABoard { .. }
        ));
        let rendered = issue.to_string();
        assert!(rendered.contains("status_authority"), "got: {rendered}");
        assert!(rendered.contains("ticket"), "got: {rendered}");
        assert!(rendered.contains("PROJECT-seven"), "got: {rendered}");
    }

    // The shape the key is for: a `github-issues` type naming a real board number
    // is clean, so neither new error fires on the steady state.
    #[test]
    fn status_authority_naming_a_board_on_a_github_issues_type_is_not_an_error() {
        let tmp = TempDir::new().unwrap();
        let config = ticket_config(r#"status_authority = "PROJECT-7""#);

        let found = issues(&config, tmp.path());

        assert!(found.is_empty(), "got: {:?}", found);
    }

    // STORY-248 AC10 / STORY-224 regression guard: every type in the shipped
    // default config declares edges, so a predicate that ignored
    // `status_authority` would light up the whole default config.
    #[test]
    fn type_without_status_authority_is_never_flagged() {
        let tmp = TempDir::new().unwrap();
        let config = ticket_config(
            r#"lifecycle = { states = ["review", "done"], edges = [{ from = "review", to = "done" }] }"#,
        );

        let found = issues(&config, tmp.path());

        assert!(found.is_empty(), "got: {:?}", found);
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    use crate::engine::config::{EdgeDef, RelSelector, RelationshipDef, Traversal};
    use crate::engine::document::{Relation, RelationType};
    use crate::engine::traversal::TraversalWalk;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn doc(path: &str, doc_type: &str, id: &str, related: Vec<Relation>) -> DocMeta {
        DocMeta {
            path: PathBuf::from(path),
            title: "T".to_string(),
            doc_type: DocType::new(doc_type),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            provenance: vec![],
            related,
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            id: id.to_string(),
            attributes: Default::default(),
        }
    }

    fn rel(rel_type: &str, target: &str) -> Relation {
        Relation {
            rel_type: RelationType::new(rel_type),
            target: target.to_string(),
        }
    }

    /// No hierarchy declared. Every assertion here is about `UnsatisfiedEdge`,
    /// which reads a row's own `via` and `to` and asks nothing of the traversal
    /// table.
    fn store_from(docs: Vec<DocMeta>) -> super::super::store::Store {
        let mut map = HashMap::new();
        for d in docs {
            map.insert(d.path.clone(), d);
        }
        super::super::store::Store {
            root: PathBuf::from("."),
            docs: map,
            forward_links: HashMap::new(),
            reverse_links: HashMap::new(),
            children: HashMap::new(),
            parent_of: HashMap::new(),
            parse_errors: Vec::new(),
            traversal_walk: TraversalWalk::default(),
            body_cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn iterations_implement_work(required: Option<Severity>) -> EdgeDef {
        EdgeDef {
            name: "iterations-implement-work".to_string(),
            from: TypeSelector::Types(vec!["iteration".to_string()]),
            to: TypeSelector::Types(vec![
                "spike".to_string(),
                "story".to_string(),
                "bug".to_string(),
            ]),
            via: RelSelector::Named(vec!["implements".to_string()]),
            required,
            traversal: None,
        }
    }

    fn targets_relationship() -> RelationshipDef {
        RelationshipDef {
            name: "targets".to_string(),
            inverse: Some("targeted-by".to_string()),
            github_native: None,
            traversal: Some(Traversal::Chain),
        }
    }

    /// The edge under test and nothing else, so any finding can only have come
    /// from the edge checker.
    fn config_with_edge(edge: EdgeDef) -> Config {
        config_with_edges(vec![edge])
    }

    fn config_with_edges(edges: Vec<EdgeDef>) -> Config {
        let mut relationships = crate::engine::config::starter_relationships();
        relationships.push(targets_relationship());
        Config {
            relationships,
            edges,
            ..Config::default()
        }
    }

    fn unsatisfied_edges(result: &ValidationResult) -> Vec<&ValidationIssue> {
        result
            .errors
            .iter()
            .chain(result.warnings.iter())
            .filter(|i| matches!(i, ValidationIssue::UnsatisfiedEdge { .. }))
            .collect()
    }

    // AC1: one edge to any single member of `to` satisfies the edge -- the set is
    // a disjunction, not a demand for one link per member.
    #[test]
    fn any_one_permitted_target_type_satisfies_the_edge() {
        for (target_type, target_id, target_path) in [
            ("spike", "SPIKE-001", "docs/spikes/SPIKE-001.md"),
            ("story", "STORY-001", "docs/stories/STORY-001.md"),
            ("bug", "BUG-001", "docs/bugs/BUG-001.md"),
        ] {
            let store = store_from(vec![
                doc(target_path, target_type, target_id, vec![]),
                doc(
                    "docs/iterations/ITERATION-001.md",
                    "iteration",
                    "ITERATION-001",
                    vec![rel("implements", target_id)],
                ),
            ]);

            let result = validate_full(
                &store,
                &config_with_edge(iterations_implement_work(Some(Severity::Error))),
            );

            assert!(
                unsatisfied_edges(&result).is_empty(),
                "an iteration implementing a {target_type} must satisfy the edge, got: {:?}",
                unsatisfied_edges(&result)
            );
        }
    }

    // AC2: the finding names the edge and every permitted target type. "an
    // iteration needs a story" is the wrong message when spikes and bugs are
    // equally valid.
    #[test]
    fn absent_edge_reports_one_error_naming_the_whole_target_set() {
        let store = store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        )]);

        let result = validate_full(
            &store,
            &config_with_edge(iterations_implement_work(Some(Severity::Error))),
        );

        let found = unsatisfied_edges(&result);
        assert_eq!(found.len(), 1, "got: {found:?}");
        assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
        let rendered = found[0].to_string();
        for expected in [
            "iterations-implement-work",
            "docs/iterations/ITERATION-001.md",
            "implements",
            "spike",
            "story",
            "bug",
        ] {
            assert!(
                rendered.contains(expected),
                "finding must name {expected}, got: {rendered}"
            );
        }
    }

    // AC3 / RFC-067 §Problem.1: `targets` also walks the chain, so under the
    // `[[rules]]` semantics it wrongly satisfies any parent-child rule. The edge
    // checker matches on `via` by name, so it does not inherit that hole.
    #[test]
    fn a_relationship_other_than_via_does_not_satisfy_the_edge() {
        let store = store_from(vec![
            doc("docs/stories/STORY-001.md", "story", "STORY-001", vec![]),
            doc(
                "docs/iterations/ITERATION-001.md",
                "iteration",
                "ITERATION-001",
                vec![rel("targets", "STORY-001")],
            ),
        ]);

        let result = validate_full(
            &store,
            &config_with_edge(iterations_implement_work(Some(Severity::Error))),
        );

        assert_eq!(
            unsatisfied_edges(&result).len(),
            1,
            "a `targets` link must not satisfy an `implements` edge, got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }

    // An `implements` link to a type outside `to` is no better than no link.
    #[test]
    fn a_target_outside_the_permitted_set_does_not_satisfy_the_edge() {
        let store = store_from(vec![
            doc("docs/rfcs/RFC-001.md", "rfc", "RFC-001", vec![]),
            doc(
                "docs/iterations/ITERATION-001.md",
                "iteration",
                "ITERATION-001",
                vec![rel("implements", "RFC-001")],
            ),
        ]);

        let result = validate_full(
            &store,
            &config_with_edge(iterations_implement_work(Some(Severity::Error))),
        );

        assert_eq!(
            unsatisfied_edges(&result).len(),
            1,
            "got: {:?}",
            result.errors
        );
    }

    // `required = "warning"` reports at that severity, not as an error.
    #[test]
    fn required_severity_decides_which_bucket_the_finding_lands_in() {
        let store = store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        )]);

        let result = validate_full(
            &store,
            &config_with_edge(iterations_implement_work(Some(Severity::Warning))),
        );

        assert_eq!(unsatisfied_edges(&result).len(), 1);
        assert!(
            result
                .errors
                .iter()
                .all(|e| !matches!(e, ValidationIssue::UnsatisfiedEdge { .. })),
            "got: {:?}",
            result.errors
        );
    }

    // An edge with no `required` is legal but not demanded: its absence is not a
    // finding.
    #[test]
    fn an_edge_without_required_is_not_checked() {
        let store = store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        )]);

        let result = validate_full(&store, &config_with_edge(iterations_implement_work(None)));

        assert!(
            unsatisfied_edges(&result).is_empty(),
            "got: {:?}",
            unsatisfied_edges(&result)
        );
    }

    // A `validate_ignore` document is exempt from edges as it is from rules.
    #[test]
    fn a_validate_ignored_document_is_exempt_from_the_edge() {
        let mut iteration = doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        );
        iteration.validate_ignore = true;
        let store = store_from(vec![iteration]);

        let result = validate_full(
            &store,
            &config_with_edge(iterations_implement_work(Some(Severity::Error))),
        );

        assert!(
            unsatisfied_edges(&result).is_empty(),
            "got: {:?}",
            unsatisfied_edges(&result)
        );
    }

    // STORY-259 ends the dual-declaration window this module once asserted:
    // `rules_and_edges_both_report_from_the_same_config` tested a coexistence
    // that no longer exists, since a config declaring `[[rules]]` does not
    // load. `strict_load_refuses_a_config_declaring_rules_and_names_fix_config`
    // in `engine::config` stands in its place.

    /// The shape `relation-existence` translates to (RFC-067 §Design): any
    /// relationship, to a document of any type.
    fn iterations_need_some_relation() -> EdgeDef {
        EdgeDef {
            name: "iterations-need-relations".to_string(),
            from: TypeSelector::Types(vec!["iteration".to_string()]),
            to: TypeSelector::Any,
            via: RelSelector::Any,
            required: Some(Severity::Error),
            traversal: None,
        }
    }

    // STORY-256 AC6: with both endpoints wildcarded, an iteration carrying no
    // relation at all is the finding -- the edge stands in for the legacy
    // `relation-existence` rule.
    #[test]
    fn a_document_with_no_relations_fails_a_wildcard_via_and_to_edge() {
        let store = store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        )]);

        let result = validate_full(&store, &config_with_edge(iterations_need_some_relation()));

        assert_eq!(
            unsatisfied_edges(&result).len(),
            1,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }

    // AC6, the other half: any one relationship to any resolvable document
    // satisfies the row, whatever the relationship or the target's type.
    #[test]
    fn any_single_relation_to_a_resolvable_document_satisfies_a_wildcard_edge() {
        for (rel_type, target_type, target_id, target_path) in [
            (
                "implements",
                "story",
                "STORY-001",
                "docs/stories/STORY-001.md",
            ),
            ("related-to", "adr", "ADR-001", "docs/adrs/ADR-001.md"),
            ("targets", "rfc", "RFC-001", "docs/rfcs/RFC-001.md"),
        ] {
            let store = store_from(vec![
                doc(target_path, target_type, target_id, vec![]),
                doc(
                    "docs/iterations/ITERATION-001.md",
                    "iteration",
                    "ITERATION-001",
                    vec![rel(rel_type, target_id)],
                ),
            ]);

            let result = validate_full(&store, &config_with_edge(iterations_need_some_relation()));

            assert!(
                unsatisfied_edges(&result).is_empty(),
                "a {rel_type} link to a {target_type} must satisfy the wildcard edge, got: {:?}",
                unsatisfied_edges(&result)
            );
        }
    }

    // `to = "*"` means any *document*, not any string in `related`: a target
    // that resolves to nothing already has its own broken-link finding, so it
    // must not quietly satisfy the edge.
    #[test]
    fn a_dangling_target_does_not_satisfy_a_wildcard_to_edge() {
        let store = store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![rel("related-to", "STORY-404")],
        )]);

        let result = validate_full(&store, &config_with_edge(iterations_need_some_relation()));

        assert_eq!(
            unsatisfied_edges(&result).len(),
            1,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }

    /// A row reached by either of two relationships (ADR-032): the shape the
    /// migration gives a rule translated against a config marking two
    /// relationships chain.
    fn iterations_reach_stories_either_way() -> EdgeDef {
        EdgeDef {
            name: "iterations-implement-work".to_string(),
            from: TypeSelector::Types(vec!["iteration".to_string()]),
            to: TypeSelector::Types(vec!["story".to_string()]),
            via: RelSelector::Named(vec!["implements".to_string(), "targets".to_string()]),
            required: Some(Severity::Error),
            traversal: None,
        }
    }

    // ADR-032: a `via` set is a disjunction, so either member satisfies the row.
    // Two rows apiece would have been a conjunction, demanding both links.
    #[test]
    fn either_member_of_a_via_set_satisfies_the_row() {
        for rel_type in ["implements", "targets"] {
            let store = store_from(vec![
                doc("docs/stories/STORY-001.md", "story", "STORY-001", vec![]),
                doc(
                    "docs/iterations/ITERATION-001.md",
                    "iteration",
                    "ITERATION-001",
                    vec![rel(rel_type, "STORY-001")],
                ),
            ]);

            let result = validate_full(
                &store,
                &config_with_edge(iterations_reach_stories_either_way()),
            );

            assert!(
                unsatisfied_edges(&result).is_empty(),
                "a {rel_type} link must satisfy the set-valued row, got: {:?}",
                unsatisfied_edges(&result)
            );
        }
    }

    // The set is one demand, not one per member, so a document carrying neither
    // relationship is told once.
    #[test]
    fn a_document_carrying_no_member_of_a_via_set_is_reported_once() {
        let store = store_from(vec![
            doc("docs/stories/STORY-001.md", "story", "STORY-001", vec![]),
            doc(
                "docs/iterations/ITERATION-001.md",
                "iteration",
                "ITERATION-001",
                vec![rel("related-to", "STORY-001")],
            ),
        ]);

        let result = validate_full(
            &store,
            &config_with_edge(iterations_reach_stories_either_way()),
        );

        assert_eq!(
            unsatisfied_edges(&result).len(),
            1,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }

    /// The one finding an edge produces against a lone iteration carrying no
    /// relation, rendered.
    fn rendered_finding_for(edge: EdgeDef) -> String {
        let store = store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        )]);

        let result = validate_full(&store, &config_with_edge(edge));

        let found = unsatisfied_edges(&result);
        assert_eq!(found.len(), 1, "got: {found:?}");
        found[0].to_string()
    }

    fn iteration_path() -> String {
        PathBuf::from("docs/iterations/ITERATION-001.md")
            .display()
            .to_string()
    }

    // STORY-256: a firing wildcard row must produce a sentence. `needs "*" to
    // one of: *` names the config spelling, not what the document is missing.
    #[test]
    fn a_wildcard_via_and_to_render_as_prose() {
        assert_eq!(
            rendered_finding_for(iterations_need_some_relation()),
            format!(
                "unsatisfied edge [iterations-need-relations]: {} \
                 (iteration needs any relationship to a document of any type)",
                iteration_path()
            )
        );
    }

    // The two positions are independent: each wildcard reads as prose while its
    // concrete counterpart keeps the wording the concrete-row tests assert.
    #[test]
    fn each_wildcard_position_composes_with_a_concrete_counterpart() {
        let mut any_via = iterations_implement_work(Some(Severity::Error));
        any_via.via = RelSelector::Any;
        assert_eq!(
            rendered_finding_for(any_via),
            format!(
                "unsatisfied edge [iterations-implement-work]: {} \
                 (iteration needs any relationship to one of: spike, story, bug)",
                iteration_path()
            )
        );

        let mut any_to = iterations_implement_work(Some(Severity::Error));
        any_to.to = TypeSelector::Any;
        assert_eq!(
            rendered_finding_for(any_to),
            format!(
                "unsatisfied edge [iterations-implement-work]: {} \
                 (iteration needs \"implements\" to a document of any type)",
                iteration_path()
            )
        );
    }

    // A set-valued `via` is satisfied by any one of its members, so the finding
    // has to name them all -- naming one would send the reader to add a link the
    // row does not single out. The target set beside it reads the same way.
    #[test]
    fn a_via_set_renders_as_the_disjunction_it_is() {
        assert_eq!(
            rendered_finding_for(iterations_reach_stories_either_way()),
            format!(
                "unsatisfied edge [iterations-implement-work]: {} \
                 (iteration needs one of: \"implements\", \"targets\" to one of: story)",
                iteration_path()
            )
        );
    }

    // A finding is about one document, and that document has one type. Naming
    // every type the row's `from` lists would read "iteration, story needs
    // ..." against a file that is only ever one of them.
    #[test]
    fn a_finding_names_the_documents_own_type_not_the_rows_source_set() {
        let mut many_from = iterations_implement_work(Some(Severity::Error));
        many_from.from = types(&["iteration", "story"]);

        assert_eq!(
            rendered_finding_for(many_from),
            format!(
                "unsatisfied edge [iterations-implement-work]: {} \
                 (iteration needs \"implements\" to one of: spike, story, bug)",
                iteration_path()
            )
        );
    }

    fn edge(
        name: &str,
        from: TypeSelector,
        to: TypeSelector,
        via: RelSelector,
        required: Option<Severity>,
    ) -> EdgeDef {
        EdgeDef {
            name: name.to_string(),
            from,
            to,
            via,
            required,
            traversal: None,
        }
    }

    fn types(names: &[&str]) -> TypeSelector {
        TypeSelector::Types(names.iter().map(|n| n.to_string()).collect())
    }

    fn lone_iteration() -> super::super::store::Store {
        store_from(vec![doc(
            "docs/iterations/ITERATION-001.md",
            "iteration",
            "ITERATION-001",
            vec![],
        )])
    }

    // STORY-256 AC2 / ADR-031: a wildcard row and a concrete row can both match
    // one edge. Requiredness comes from the more specific row, so the document
    // gets one finding at that row's severity -- not one finding per matching
    // row.
    #[test]
    fn the_more_specific_of_two_overlapping_rows_decides_requiredness() {
        let config = config_with_edges(vec![
            edge(
                "iterations-implement-something",
                types(&["iteration"]),
                TypeSelector::Any,
                RelSelector::Named(vec!["implements".to_string()]),
                Some(Severity::Warning),
            ),
            edge(
                "iterations-implement-stories",
                types(&["iteration"]),
                types(&["story"]),
                RelSelector::Named(vec!["implements".to_string()]),
                Some(Severity::Error),
            ),
        ]);

        let result = validate_full(&lone_iteration(), &config);

        let found = unsatisfied_edges(&result);
        assert_eq!(
            found.len(),
            1,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
        assert!(
            found[0]
                .to_string()
                .contains("iterations-implement-stories"),
            "the concrete row must be the one that fires, got: {}",
            found[0]
        );
        assert!(
            result
                .warnings
                .iter()
                .all(|w| !matches!(w, ValidationIssue::UnsatisfiedEdge { .. })),
            "the wildcard row's `warning` must not survive resolution, got: {:?}",
            result.warnings
        );
    }

    // ADR-031: a row that omits `required` is documentation and takes no part in
    // resolution, so however specific it is it cannot silence a broader demand.
    // The narrow row here scores three to the demand's one and still displaces
    // nothing: an iteration with no relations at all is a finding.
    #[test]
    fn a_more_specific_row_without_required_does_not_displace_a_demand() {
        let config = config_with_edges(vec![
            edge(
                "iterations-may-implement-stories",
                types(&["iteration"]),
                types(&["story"]),
                RelSelector::Named(vec!["implements".to_string()]),
                None,
            ),
            edge(
                "iterations-need-relations",
                types(&["iteration"]),
                TypeSelector::Any,
                RelSelector::Any,
                Some(Severity::Error),
            ),
        ]);

        let result = validate_full(&lone_iteration(), &config);

        let found = unsatisfied_edges(&result);
        assert_eq!(
            found.len(),
            1,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
        assert!(
            found[0].to_string().contains("iterations-need-relations"),
            "the demand must survive the documentation-only row, got: {}",
            found[0]
        );
    }

    // Resolution only silences a row some more specific row displaces. Two rows
    // that cannot both match one edge are independent demands, and both fire.
    #[test]
    fn two_rows_that_do_not_overlap_both_fire() {
        let config = config_with_edges(vec![
            edge(
                "iterations-implement-stories",
                types(&["iteration"]),
                types(&["story"]),
                RelSelector::Named(vec!["implements".to_string()]),
                Some(Severity::Error),
            ),
            edge(
                "iterations-target-rfcs",
                types(&["iteration"]),
                types(&["rfc"]),
                RelSelector::Named(vec!["targets".to_string()]),
                Some(Severity::Warning),
            ),
        ]);

        let result = validate_full(&lone_iteration(), &config);

        assert_eq!(
            unsatisfied_edges(&result).len(),
            2,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }

    // Rows overlap on a `from` type this document is not, so the specific row
    // never applied here and has nothing to displace.
    #[test]
    fn a_more_specific_row_for_another_from_type_does_not_displace() {
        let config = config_with_edges(vec![
            edge(
                "work-implements-something",
                types(&["iteration", "adr"]),
                TypeSelector::Any,
                RelSelector::Named(vec!["implements".to_string()]),
                Some(Severity::Error),
            ),
            edge(
                "adrs-implement-rfcs",
                types(&["adr"]),
                types(&["rfc"]),
                RelSelector::Named(vec!["implements".to_string()]),
                Some(Severity::Warning),
            ),
        ]);

        let result = validate_full(&lone_iteration(), &config);

        assert_eq!(
            unsatisfied_edges(&result).len(),
            1,
            "got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }
}

/// The five findings that outlive `[[rules]]`, read off a config whose ONLY
/// declaration of hierarchy is an `[[edges]]` row: no `[[rules]]` block and no
/// `RelationshipDef.traversal` marker anywhere. Before ITERATION-380 every one
/// of these needed a `parent-child` rule plus a global chain marker to fire.
#[cfg(test)]
mod hierarchy_from_edges_tests {
    use super::*;
    use crate::engine::config::{EdgeDef, RelSelector, RelationshipDef, Traversal, TypeSelector};
    use crate::engine::store::test_support::store_from_with_config;
    use crate::engine::store::Store;
    use tempfile::TempDir;

    fn doc_md(title: &str, doc_type: &str, status: &str, related: &str) -> String {
        let related_block = if related == "[]" {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: {status}\nauthor: t\ndate: 2026-09-01\ntags: []\n{related_block}\n---\n\n{title} body\n"
        )
    }

    /// `implements` and `blocks` declared with no traversal marker of their own,
    /// so nothing but a row can make a relation hierarchy.
    fn unmarked_relationships() -> Vec<RelationshipDef> {
        ["implements", "blocks"]
            .into_iter()
            .map(|name| RelationshipDef {
                name: name.to_string(),
                inverse: None,
                github_native: None,
                traversal: None,
            })
            .collect()
    }

    /// One chain row, `story -implements-> rfc`, and no rules at all.
    fn stories_implement_rfcs() -> Config {
        Config {
            relationships: unmarked_relationships(),
            edges: vec![EdgeDef {
                name: "stories-implement-rfcs".to_string(),
                from: TypeSelector::Types(vec!["story".to_string()]),
                to: TypeSelector::Types(vec!["rfc".to_string()]),
                via: RelSelector::Named(vec!["implements".to_string()]),
                required: None,
                traversal: Some(Traversal::Chain),
            }],
            ..Config::default()
        }
    }

    /// A `rejected` rfc and a story linked to it by `rel`.
    fn story_linked_to_rejected_rfc(rel: &str) -> (TempDir, Store) {
        store_from_with_config(
            &[
                (
                    "docs/rfcs/RFC-001-dead.md",
                    &doc_md("Dead", "rfc", "rejected", "[]"),
                ),
                (
                    "docs/stories/STORY-001-live.md",
                    &doc_md("Live", "story", "draft", &format!("- {rel}: RFC-001")),
                ),
            ],
            &stories_implement_rfcs(),
        )
    }

    #[test]
    fn a_chain_edge_row_alone_reports_a_rejected_parent() {
        let (_tmp, store) = story_linked_to_rejected_rfc("implements");

        let result = validate_full(&store, &stories_implement_rfcs());

        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationIssue::RejectedParent { .. })),
            "the row is the whole declaration of hierarchy, got errors {:?} warnings {:?}",
            result.errors,
            result.warnings
        );
    }

    #[test]
    fn a_relation_no_chain_row_covers_is_not_a_parent_link() {
        let (_tmp, store) = story_linked_to_rejected_rfc("blocks");

        let result = validate_full(&store, &stories_implement_rfcs());

        assert!(
            !result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationIssue::RejectedParent { .. })),
            "`blocks` is hierarchy nowhere in this config, got errors {:?}",
            result.errors
        );
    }
}
