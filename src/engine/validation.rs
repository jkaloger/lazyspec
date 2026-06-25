use crate::engine::config::{AttrKind, Config, Severity, ValidationRule as ConfigRule};
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

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationIssue::BrokenLink { source, target } => {
                write!(f, "broken link: {} -> {}", source.display(), target)
            }
            ValidationIssue::MissingParentLink {
                path,
                rule_name,
                child_type,
                parent_type,
            } => {
                write!(
                    f,
                    "missing parent link [{}]: {} ({} needs {})",
                    rule_name,
                    path.display(),
                    child_type,
                    parent_type
                )
            }
            ValidationIssue::MissingRelation {
                path,
                rule_name,
                doc_type,
            } => {
                write!(
                    f,
                    "missing relation [{}]: {} ({} needs a relation)",
                    rule_name,
                    path.display(),
                    doc_type
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

fn hierarchy_from_config(config: &Config) -> Vec<(String, String, String)> {
    config
        .rules
        .iter()
        .filter_map(|rule| match rule {
            ConfigRule::ParentChild {
                parent,
                child,
                link,
                ..
            } => Some((parent.clone(), child.clone(), link.clone())),
            _ => None,
        })
        .collect()
}

pub struct BrokenLinkRule;

impl Checker for BrokenLinkRule {
    fn check(
        &self,
        store: &super::store::Store,
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let hierarchy = hierarchy_from_config(config);
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

                let is_hierarchy_link = hierarchy
                    .iter()
                    .any(|(_, _, link)| rel.rel_type.to_string() == *link);
                if !is_hierarchy_link {
                    continue;
                }

                let Some(parent_doc) = store.docs.get(&target) else {
                    continue;
                };

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

                let is_child_in_hierarchy = hierarchy.iter().any(|(pt, ct, link)| {
                    meta.doc_type == DocType::new(ct)
                        && parent_doc.doc_type == DocType::new(pt)
                        && rel.rel_type.to_string() == *link
                });
                if is_child_in_hierarchy
                    && meta.status == Status::new("accepted")
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

pub struct ParentLinkRule;

impl Checker for ParentLinkRule {
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

            for rule in &config.rules {
                match rule {
                    ConfigRule::ParentChild {
                        name,
                        child,
                        parent,
                        link,
                        severity,
                        ..
                    } => {
                        if meta.doc_type != DocType::new(child) {
                            continue;
                        }
                        let has_parent_link = meta.related.iter().any(|r| {
                            let resolved = id_to_path
                                .get(&r.target)
                                .cloned()
                                .unwrap_or_else(|| PathBuf::from(&r.target));
                            r.rel_type.to_string() == *link
                                && store
                                    .docs
                                    .get(&resolved)
                                    .map(|d| d.doc_type == DocType::new(parent))
                                    .unwrap_or(false)
                        });
                        if !has_parent_link {
                            issues.push((
                                severity.clone(),
                                ValidationIssue::MissingParentLink {
                                    path: path.clone(),
                                    rule_name: name.clone(),
                                    child_type: child.clone(),
                                    parent_type: parent.clone(),
                                },
                            ));
                        }
                    }
                    ConfigRule::RelationExistence {
                        name,
                        doc_type,
                        severity,
                        ..
                    } => {
                        if meta.doc_type != DocType::new(doc_type) {
                            continue;
                        }
                        if meta.related.is_empty() {
                            issues.push((
                                severity.clone(),
                                ValidationIssue::MissingRelation {
                                    path: path.clone(),
                                    rule_name: name.clone(),
                                    doc_type: doc_type.clone(),
                                },
                            ));
                        }
                    }
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
        config: &Config,
    ) -> Vec<(Severity, ValidationIssue)> {
        let hierarchy = hierarchy_from_config(config);
        let mut issues = Vec::new();

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
                        rel_type.to_string() == *link
                            && store
                                .docs
                                .get(child_path)
                                .map(|d| {
                                    d.doc_type == DocType::new(child_type) && !d.validate_ignore
                                })
                                .unwrap_or(false)
                    })
                    .map(|(_, child_path)| child_path.clone())
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
                    if child.status == Status::new("accepted")
                        && child.doc_type == DocType::new(child_type)
                    {
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

pub struct UnknownRelationshipRule;

impl Checker for UnknownRelationshipRule {
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
            for rel in &meta.related {
                if config.relationship_by_name(rel.rel_type.as_str()).is_none() {
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
        Box::new(ParentLinkRule),
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
            chain_relationships: vec!["implements".to_string()],
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
            chain_relationships: vec!["implements".to_string()],
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
