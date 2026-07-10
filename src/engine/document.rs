use crate::engine::config::{AttrDef, AttrKind, TypeDef};
use crate::engine::fs::FileSystem;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// serde_yaml 0.9 parses bare `YYYY-MM-DD` as a YAML date tag, not a string.
/// Chrono's default `NaiveDate` deserializer expects a string, so we need a
/// custom deserializer that handles both representations.
pub(crate) fn deserialize_naive_date<'de, D>(
    deserializer: D,
) -> std::result::Result<NaiveDate, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    let date_str = match &value {
        serde_yaml::Value::String(s) => s.clone(),
        // serde_yaml 0.9 parses bare YYYY-MM-DD as a tagged timestamp
        serde_yaml::Value::Tagged(tagged) => match &tagged.value {
            serde_yaml::Value::String(s) => s.clone(),
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "expected date string, got: {:?}",
                    value
                )))
            }
        },
        // Some serde_yaml versions parse bare dates as a single-key mapping
        serde_yaml::Value::Mapping(m) if m.len() == 1 => {
            let key = m.keys().next().unwrap();
            match key {
                serde_yaml::Value::String(s) => s.clone(),
                _ => {
                    return Err(serde::de::Error::custom(format!(
                        "expected date string, got: {:?}",
                        value
                    )))
                }
            }
        }
        _ => {
            return Err(serde::de::Error::custom(format!(
                "expected date string, got: {:?}",
                value
            )))
        }
    };
    NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DocType(String);

impl DocType {
    pub const RFC: &str = "rfc";
    pub const STORY: &str = "story";
    pub const ITERATION: &str = "iteration";
    pub const ADR: &str = "adr";
    pub const SPEC: &str = "spec";

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

/// A document status (e.g. `draft`, `in-progress`). Mirrors [`RelationType`]:
/// an open string newtype validated against the owning type's lifecycle states
/// rather than by the type system. `FromStr`/`Deserialize` are pure (any string
/// is a valid value); membership is checked separately by `TypeDef::accepts_status`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Status(String);

impl Status {
    pub fn new(s: &str) -> Self {
        Status(s.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Status(s.to_lowercase()))
    }
}

/// A relationship name (e.g. `implements`, `related-to`). Mirrors [`DocType`]:
/// an open string newtype validated against the config `[[relationships]]`
/// registry rather than by the type system. `FromStr` is pure (any string is a
/// valid value); `link`/`validate` reject names absent from the registry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RelationType(String);

impl RelationType {
    pub fn new(s: &str) -> Self {
        RelationType(s.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for RelationType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(RelationType(s.to_lowercase()))
    }
}

impl std::str::FromStr for RelationType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RelationType::new(s))
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
        Ok(Status::new(s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub rel_type: RelationType,
    pub target: String,
}

/// A typed custom frontmatter attribute value. Declared attributes are coerced
/// to their `kind`; undeclared keys are preserved as [`AttrValue::Raw`].
#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Date(NaiveDate),
    Raw(serde_yaml::Value),
}

/// Serialize each variant as its bare JSON value (no enum tag). `Date` is emitted
/// as a `YYYY-MM-DD` string rather than serde's default tuple, and `Raw` passes
/// the underlying YAML value through transparently.
impl Serialize for AttrValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AttrValue::Int(i) => serializer.serialize_i64(*i),
            AttrValue::Float(f) => serializer.serialize_f64(*f),
            AttrValue::Str(s) => serializer.serialize_str(s),
            AttrValue::Bool(b) => serializer.serialize_bool(*b),
            AttrValue::Date(d) => serializer.serialize_str(&d.format("%Y-%m-%d").to_string()),
            AttrValue::Raw(v) => v.serialize(serializer),
        }
    }
}

/// Parse a `NaiveDate` out of a YAML value, accepting the same representations
/// as [`deserialize_naive_date`] (plain string, tagged timestamp, or the
/// single-key-mapping form serde_yaml 0.9 sometimes produces for bare dates).
fn naive_date_from_yaml(value: &serde_yaml::Value) -> Option<NaiveDate> {
    let date_str = match value {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Tagged(tagged) => match &tagged.value {
            serde_yaml::Value::String(s) => s.clone(),
            _ => return None,
        },
        serde_yaml::Value::Mapping(m) if m.len() == 1 => match m.keys().next() {
            Some(serde_yaml::Value::String(s)) => s.clone(),
            _ => return None,
        },
        _ => return None,
    };
    NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").ok()
}

/// Coerce a raw YAML value against a declared attribute kind. Returns `None`
/// when the value does not match the kind (a validation error, surfaced by the
/// attribute-schema checker rather than failing the parse).
pub(crate) fn coerce_attr(value: &serde_yaml::Value, def: &AttrDef) -> Option<AttrValue> {
    match def.kind {
        AttrKind::Int => value.as_i64().map(AttrValue::Int),
        AttrKind::Float => value.as_f64().map(AttrValue::Float),
        AttrKind::Str => value.as_str().map(|s| AttrValue::Str(s.to_string())),
        AttrKind::Enum => value.as_str().and_then(|s| {
            if def.values.iter().any(|v| v == s) {
                Some(AttrValue::Str(s.to_string()))
            } else {
                None
            }
        }),
        AttrKind::Date => naive_date_from_yaml(value).map(AttrValue::Date),
        AttrKind::Bool => value.as_bool().map(AttrValue::Bool),
    }
}

/// Coerce and validate raw `key=value` attribute updates against a type's
/// declared [`AttrDef`]s, inserting the typed [`AttrValue`]s into `meta`.
///
/// This is the single seam through which both stores turn raw CLI strings into
/// typed attributes, so coercion fidelity is identical across backends. Any
/// failure (unknown key, kind mismatch, bad enum option, or a now-missing
/// required attribute) returns an error before `meta` is persisted.
pub fn apply_attrs(type_def: &TypeDef, meta: &mut DocMeta, attrs: &[(&str, &str)]) -> Result<()> {
    for (key, value) in attrs {
        let def = type_def
            .attributes
            .iter()
            .find(|d| d.name == *key)
            .ok_or_else(|| anyhow!("unknown attribute '{}' for type '{}'", key, type_def.name))?;

        // String/enum kinds take the raw text verbatim; numeric/bool/date kinds
        // are parsed as YAML scalars so `coerce_attr` sees a typed value (e.g.
        // "3" -> Number(3), not String("3")).
        let yaml = match def.kind {
            AttrKind::Str | AttrKind::Enum => serde_yaml::Value::String((*value).to_string()),
            _ => serde_yaml::from_str(value)
                .unwrap_or_else(|_| serde_yaml::Value::String((*value).to_string())),
        };
        let coerced = coerce_attr(&yaml, def).ok_or_else(|| {
            if def.kind == AttrKind::Enum {
                anyhow!(
                    "invalid value for attribute '{}': '{}' is not one of [{}]",
                    key,
                    value,
                    def.values.join(", ")
                )
            } else {
                anyhow!(
                    "invalid value for attribute '{}': '{}' is not a valid {}",
                    key,
                    value,
                    format!("{:?}", def.kind).to_lowercase()
                )
            }
        })?;

        meta.attributes.insert((*key).to_string(), coerced);
    }

    for def in &type_def.attributes {
        if def.required && !meta.attributes.contains_key(&def.name) {
            return Err(anyhow!(
                "missing required attribute '{}' for type '{}'",
                def.name,
                type_def.name
            ));
        }
    }

    Ok(())
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
    pub provenance: Vec<String>,
    pub related: Vec<Relation>,
    pub validate_ignore: bool,
    pub virtual_doc: bool,
    pub id: String,
    /// Custom frontmatter attributes, keyed by their frontmatter name. Declared
    /// attributes carry a coerced [`AttrValue`]; undeclared keys are preserved as
    /// [`AttrValue::Raw`].
    pub attributes: BTreeMap<String, AttrValue>,
}

#[derive(Deserialize)]
struct RawFrontmatter {
    title: String,
    #[serde(rename = "type")]
    doc_type: DocType,
    status: Status,
    author: String,
    #[serde(deserialize_with = "deserialize_naive_date")]
    date: NaiveDate,
    tags: Vec<String>,
    #[serde(default)]
    provenance: Vec<String>,
    #[serde(default)]
    related: Vec<serde_yaml::Value>,
    #[serde(default, rename = "validate-ignore")]
    validate_ignore: bool,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

pub fn rewrite_frontmatter<F>(path: &Path, fs: &dyn FileSystem, mutate: F) -> Result<()>
where
    F: FnOnce(&mut serde_yaml::Value) -> Result<()>,
{
    let content = fs.read_to_string(path)?;
    let (yaml, body) = split_frontmatter(&content)?;
    let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
    mutate(&mut value)?;
    let new_yaml = serde_yaml::to_string(&value)?;
    let output = compose_frontmatter(&new_yaml, &body);
    fs.write(path, &output)?;
    Ok(())
}

/// Reconstruct a markdown document from a YAML frontmatter block and body.
///
/// Inverse of [`split_frontmatter`]: composing the parts that `split_frontmatter`
/// returned reproduces the original document. The body is preserved byte-for-byte
/// (including any leading newline that follows the closing `---` delimiter), so
/// repeated split/compose cycles do not accumulate blank lines.
pub fn compose_frontmatter(yaml: &str, body: &str) -> String {
    if yaml.ends_with('\n') {
        format!("---\n{}---{}", yaml, body)
    } else {
        format!("---\n{}\n---{}", yaml, body)
    }
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

pub(crate) fn parse_relation(value: &serde_yaml::Value) -> Result<Relation> {
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
    /// Parse without an attribute schema: every undeclared frontmatter key is
    /// preserved as [`AttrValue::Raw`].
    pub fn parse(content: &str) -> Result<Self> {
        Self::parse_with_schema(content, &[])
    }

    /// Parse against a document type's declared attribute `schema`. Keys naming a
    /// declared attribute are coerced to the attribute's kind (an unmatched
    /// coercion preserves the value as [`AttrValue::Raw`] so the schema checker can
    /// report it); keys not in the schema are kept as [`AttrValue::Raw`].
    pub fn parse_with_schema(content: &str, schema: &[AttrDef]) -> Result<Self> {
        let (frontmatter, _) = split_frontmatter(content)?;
        let raw: RawFrontmatter = serde_yaml::from_str(&frontmatter)?;

        for entry in &raw.provenance {
            if entry.is_empty() {
                return Err(anyhow!(
                    "provenance entry must not be empty (title: {})",
                    raw.title
                ));
            }
        }

        let related = raw
            .related
            .iter()
            .map(parse_relation)
            .collect::<Result<Vec<_>>>()?;

        let attributes = raw
            .extra
            .into_iter()
            .map(|(key, value)| {
                let coerced = match schema.iter().find(|d| d.name == key) {
                    Some(def) => coerce_attr(&value, def).unwrap_or(AttrValue::Raw(value)),
                    None => AttrValue::Raw(value),
                };
                (key, coerced)
            })
            .collect();

        Ok(DocMeta {
            path: PathBuf::new(),
            title: raw.title,
            doc_type: raw.doc_type,
            status: raw.status,
            author: raw.author,
            date: raw.date,
            tags: raw.tags,
            provenance: raw.provenance,
            related,
            validate_ignore: raw.validate_ignore,
            virtual_doc: false,
            id: String::new(),
            attributes,
        })
    }

    pub fn extract_body(content: &str) -> Result<String> {
        let (_, body) = split_frontmatter(content)?;
        Ok(body.trim_start_matches('\n').to_string())
    }

    pub fn display_name(&self) -> &str {
        &self.id
    }

    pub fn sort_by_date(a: &DocMeta, b: &DocMeta) -> std::cmp::Ordering {
        a.date.cmp(&b.date).then_with(|| a.path.cmp(&b.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_doc(date: &str, path: &str) -> DocMeta {
        DocMeta {
            path: PathBuf::from(path),
            title: String::new(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: String::new(),
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            id: String::new(),
            attributes: BTreeMap::new(),
        }
    }

    #[test]
    fn sort_by_date_oldest_first() {
        let old = make_doc("2025-01-01", "a.md");
        let new = make_doc("2026-03-17", "b.md");
        let mut docs = [new, old];
        docs.sort_by(DocMeta::sort_by_date);
        assert_eq!(docs[0].date, NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        assert_eq!(docs[1].date, NaiveDate::from_ymd_opt(2026, 3, 17).unwrap());
    }

    #[test]
    fn sort_by_date_same_date_tiebreak_by_path() {
        let a = make_doc("2026-01-01", "aaa.md");
        let b = make_doc("2026-01-01", "zzz.md");
        let mut docs = [b, a];
        docs.sort_by(DocMeta::sort_by_date);
        assert_eq!(docs[0].path, PathBuf::from("aaa.md"));
        assert_eq!(docs[1].path, PathBuf::from("zzz.md"));
    }

    #[test]
    fn sort_by_date_single_and_empty() {
        let mut empty: Vec<DocMeta> = vec![];
        empty.sort_by(DocMeta::sort_by_date);
        assert!(empty.is_empty());

        let mut single = [make_doc("2026-01-01", "only.md")];
        single.sort_by(DocMeta::sort_by_date);
        assert_eq!(single.len(), 1);
    }

    #[test]
    fn provenance_loads_in_order() {
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: a
date: 2026-01-01
tags: []
provenance:
  - "Workshop 2026-04-12"
  - "Jane Doe"
  - "Privacy Act 1988"
---

Body.
"#;
        let meta = DocMeta::parse(content).unwrap();
        assert_eq!(
            meta.provenance,
            vec![
                "Workshop 2026-04-12".to_string(),
                "Jane Doe".to_string(),
                "Privacy Act 1988".to_string(),
            ]
        );
    }

    #[test]
    fn provenance_missing_defaults_empty() {
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: a
date: 2026-01-01
tags: []
---

Body.
"#;
        let meta = DocMeta::parse(content).unwrap();
        assert!(meta.provenance.is_empty());
    }

    #[test]
    fn provenance_empty_list_loads() {
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: a
date: 2026-01-01
tags: []
provenance: []
---

Body.
"#;
        let meta = DocMeta::parse(content).unwrap();
        assert!(meta.provenance.is_empty());
    }

    #[test]
    fn split_compose_roundtrip_preserves_content() {
        let cases = [
            "---\ntitle: foo\n---\nbody\n",
            "---\ntitle: foo\n---\n\nbody with blank line\n",
            "---\ntitle: foo\n---\n",
            "---\ntitle: foo\n---",
            "---\ntitle: foo\n---\nbody without trailing newline",
        ];
        for original in cases {
            let (yaml, body) = split_frontmatter(original).unwrap();
            let yaml_with_newline = format!("{}\n", yaml);
            let recomposed = compose_frontmatter(&yaml_with_newline, &body);
            assert_eq!(recomposed, original, "roundtrip failed for: {:?}", original);
        }
    }

    #[test]
    fn rewrite_frontmatter_is_idempotent() {
        use crate::engine::fs::FileSystem;
        use std::cell::RefCell;
        use std::collections::HashMap;

        struct InMemFs(RefCell<HashMap<PathBuf, String>>);
        impl FileSystem for InMemFs {
            fn read_to_string(&self, p: &Path) -> Result<String> {
                self.0
                    .borrow()
                    .get(p)
                    .cloned()
                    .ok_or_else(|| anyhow!("not found: {}", p.display()))
            }
            fn write(&self, p: &Path, c: &str) -> Result<()> {
                self.0.borrow_mut().insert(p.to_path_buf(), c.to_string());
                Ok(())
            }
            fn rename(&self, _: &Path, _: &Path) -> Result<()> {
                Ok(())
            }
            fn read_dir(&self, _: &Path) -> Result<Vec<PathBuf>> {
                Ok(vec![])
            }
            fn exists(&self, p: &Path) -> bool {
                self.0.borrow().contains_key(p)
            }
            fn create_dir_all(&self, _: &Path) -> Result<()> {
                Ok(())
            }
            fn is_dir(&self, _: &Path) -> bool {
                false
            }
        }

        let initial = "---\ntitle: foo\n---\nbody\n";
        let path = PathBuf::from("doc.md");
        let mut map = HashMap::new();
        map.insert(path.clone(), initial.to_string());
        let fs = InMemFs(RefCell::new(map));

        rewrite_frontmatter(&path, &fs, |_| Ok(())).unwrap();
        let after_first = fs.read_to_string(&path).unwrap();

        for _ in 0..5 {
            rewrite_frontmatter(&path, &fs, |_| Ok(())).unwrap();
        }
        let after_many = fs.read_to_string(&path).unwrap();

        assert_eq!(
            after_first, after_many,
            "no-op rewrite must not accumulate newlines across runs"
        );
    }

    #[test]
    fn provenance_empty_string_rejected() {
        let content = r#"---
title: "Doc"
type: rfc
status: draft
author: a
date: 2026-01-01
tags: []
provenance:
  - ""
  - "ok"
---

Body.
"#;
        let err = DocMeta::parse(content).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("empty"),
            "expected error to mention empty, got: {}",
            err
        );
    }

    #[test]
    fn relation_type_new_lowercases_and_displays_inner() {
        assert_eq!(RelationType::new("Tracks").to_string(), "tracks");
        assert_eq!(RelationType::new("RELATED-TO").as_str(), "related-to");
    }

    #[test]
    fn relation_type_fromstr_is_pure_and_never_errors() {
        let rt: RelationType = "anything-goes".parse().unwrap();
        assert_eq!(rt.to_string(), "anything-goes");
        assert_eq!(rt, RelationType::new("anything-goes"));
    }

    // AC2: declared int and date attributes coerce to typed AttrValue.
    #[test]
    fn attributes_coerce_against_schema() {
        use crate::engine::config::{AttrDef, AttrKind};
        let content = r#"---
title: "Doc"
type: story
status: draft
author: a
date: 2026-01-01
tags: []
estimate: 5
due: 2026-03-15
---

Body.
"#;
        let schema = vec![
            AttrDef {
                name: "estimate".to_string(),
                kind: AttrKind::Int,
                required: false,
                values: vec![],
            },
            AttrDef {
                name: "due".to_string(),
                kind: AttrKind::Date,
                required: false,
                values: vec![],
            },
        ];
        let meta = DocMeta::parse_with_schema(content, &schema).unwrap();
        assert_eq!(meta.attributes["estimate"], AttrValue::Int(5));
        assert_eq!(
            meta.attributes["due"],
            AttrValue::Date(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())
        );
    }

    // AC4: an undeclared key parses (does not fail) and is preserved as Raw.
    #[test]
    fn undeclared_attribute_preserved_as_raw() {
        let content = r#"---
title: "Doc"
type: story
status: draft
author: a
date: 2026-01-01
tags: []
mystery: hello
---

Body.
"#;
        let meta = DocMeta::parse(content).unwrap();
        match &meta.attributes["mystery"] {
            AttrValue::Raw(v) => assert_eq!(v.as_str(), Some("hello")),
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    // AC1 support: AttrValue serializes as the bare typed JSON value, not a tagged enum.
    #[test]
    fn attr_value_serializes_as_bare_value() {
        use serde_json::json;
        assert_eq!(serde_json::to_value(AttrValue::Int(5)).unwrap(), json!(5));
        assert_eq!(
            serde_json::to_value(AttrValue::Float(2.5)).unwrap(),
            json!(2.5)
        );
        assert_eq!(
            serde_json::to_value(AttrValue::Str("hi".to_string())).unwrap(),
            json!("hi")
        );
        assert_eq!(
            serde_json::to_value(AttrValue::Bool(true)).unwrap(),
            json!(true)
        );
    }

    // Date must serialize as the YYYY-MM-DD string, not serde's default tuple form.
    #[test]
    fn attr_value_date_serializes_as_iso_string() {
        let v = AttrValue::Date(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
        assert_eq!(
            serde_json::to_value(v).unwrap(),
            serde_json::json!("2026-03-15")
        );
    }

    // Raw passes the underlying YAML value through transparently.
    #[test]
    fn attr_value_raw_serializes_inner_value() {
        let raw = AttrValue::Raw(serde_yaml::Value::String("passthrough".to_string()));
        assert_eq!(
            serde_json::to_value(raw).unwrap(),
            serde_json::json!("passthrough")
        );
        let raw_num = AttrValue::Raw(serde_yaml::Value::Number(7.into()));
        assert_eq!(serde_json::to_value(raw_num).unwrap(), serde_json::json!(7));
    }

    #[test]
    fn status_newtype_fromstr_is_pure_and_lowercases() {
        let s: Status = "In-Progress".parse().unwrap();
        assert_eq!(s.to_string(), "in-progress");
        let arbitrary: Status = "frozen".parse().unwrap();
        assert_eq!(arbitrary, Status::new("frozen"));
    }

    fn type_def_with_attrs(attrs: Vec<AttrDef>) -> TypeDef {
        use crate::engine::config::{NumberingStrategy, StoreBackend};
        TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: "docs/stories".to_string(),
            prefix: "STORY".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::Filesystem,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: attrs,
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        }
    }

    fn blank_meta() -> DocMeta {
        DocMeta {
            path: PathBuf::new(),
            title: String::new(),
            doc_type: DocType::new("story"),
            status: Status::new("draft"),
            author: String::new(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            id: String::new(),
            attributes: BTreeMap::new(),
        }
    }

    // AC4: multiple attrs coerce per-kind; estimate becomes Int, not Str.
    #[test]
    fn apply_attrs_coerces_per_kind() {
        let td = type_def_with_attrs(vec![
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
        ]);
        let mut meta = blank_meta();
        apply_attrs(&td, &mut meta, &[("owner", "jkaloger"), ("estimate", "3")]).unwrap();
        assert_eq!(
            meta.attributes["owner"],
            AttrValue::Str("jkaloger".to_string())
        );
        assert_eq!(meta.attributes["estimate"], AttrValue::Int(3));
    }

    // AC2: bad enum option errors and names the key.
    #[test]
    fn apply_attrs_rejects_bad_enum() {
        let td = type_def_with_attrs(vec![AttrDef {
            name: "priority".to_string(),
            kind: AttrKind::Enum,
            required: false,
            values: vec!["low".to_string(), "med".to_string(), "high".to_string()],
        }]);
        let mut meta = blank_meta();
        let err = apply_attrs(&td, &mut meta, &[("priority", "urgent")]).unwrap_err();
        assert!(err.to_string().contains("priority"), "got: {err}");
        assert!(meta.attributes.is_empty());
    }

    // AC2: kind mismatch errors and names the key.
    #[test]
    fn apply_attrs_rejects_kind_mismatch() {
        let td = type_def_with_attrs(vec![AttrDef {
            name: "estimate".to_string(),
            kind: AttrKind::Int,
            required: false,
            values: vec![],
        }]);
        let mut meta = blank_meta();
        let err = apply_attrs(&td, &mut meta, &[("estimate", "notanumber")]).unwrap_err();
        assert!(err.to_string().contains("estimate"), "got: {err}");
    }

    #[test]
    fn apply_attrs_rejects_unknown_key() {
        let td = type_def_with_attrs(vec![]);
        let mut meta = blank_meta();
        let err = apply_attrs(&td, &mut meta, &[("mystery", "x")]).unwrap_err();
        assert!(err.to_string().contains("mystery"), "got: {err}");
    }

    #[test]
    fn apply_attrs_enforces_required() {
        let td = type_def_with_attrs(vec![AttrDef {
            name: "owner".to_string(),
            kind: AttrKind::Str,
            required: true,
            values: vec![],
        }]);
        let mut meta = blank_meta();
        let err = apply_attrs(&td, &mut meta, &[]).unwrap_err();
        assert!(err.to_string().contains("owner"), "got: {err}");
    }
}
