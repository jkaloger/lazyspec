use crate::engine::config::{
    AttrDef, AttrKind, Authorship, Config, Edge, Lifecycle, NumberingStrategy, StoreBackend,
    TypeDef, ValidationRule,
};
use crate::engine::config_write::write_config_in_place;
use crate::engine::fs::FileSystem;
use anyhow::{bail, Result};
use clap::Subcommand;
use std::path::Path;

// AddType dwarfs the other variants; one instance exists per process, so the
// size skew is irrelevant and boxing it would only complicate clap's derive.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print the resolved configuration as JSON
    Show {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Print the JSON Schema for .lazyspec.toml
    Schema {
        /// Output as JSON (schema output is JSON regardless)
        #[arg(long)]
        json: bool,
    },
    /// Append a new document type to .lazyspec.toml
    AddType {
        /// Type name (e.g. spike)
        name: String,
        /// Plural form used for directory listings (e.g. spikes)
        plural: String,
        /// Directory the type's documents live in (e.g. docs/spikes)
        dir: String,
        /// ID prefix for the type (e.g. SPIKE)
        prefix: String,
        /// Icon shown in the TUI
        #[arg(long)]
        icon: Option<String>,
        /// Parent type name, gating creation and validation
        #[arg(long)]
        parent_type: Option<String>,
        /// Mark the type as a singleton (a single document, not numbered series)
        #[arg(long)]
        singleton: bool,
        /// Storage backend: filesystem, github-issues, or git-ref
        #[arg(long)]
        store: Option<String>,
        /// Numbering strategy: incremental, sqids, or reserved
        #[arg(long)]
        numbering: Option<String>,
        /// One-line statement of what the type is for
        #[arg(long)]
        intent: Option<String>,
        /// Authorship ceiling: human, assisted, or generated
        #[arg(long)]
        authorship: Option<String>,
        /// ClickUp custom task type (numeric custom_item_id); only valid on store = clickup-tasks
        #[arg(long)]
        clickup_task_type: Option<i64>,
        /// A custom frontmatter attribute as NAME:KIND[:required][:VAL1,VAL2,...]
        /// (kind: int, float, string, enum, date, bool; values only for enum; repeat per attribute)
        #[arg(long = "attribute")]
        attributes: Vec<String>,
    },
    /// Replace a type's lifecycle states and edges
    SetLifecycle {
        /// Type name to set the lifecycle on
        name: String,
        /// A lifecycle state (repeat for each state)
        #[arg(long = "state")]
        states: Vec<String>,
        /// A permitted transition as FROM:TO (`*` matches any source; repeat per edge)
        #[arg(long = "edge")]
        edges: Vec<String>,
    },
    /// Set the require_parent_status gate on a parent-child rule
    AddGate {
        /// Rule name to gate
        name: String,
        /// Parent status required before a child may be created
        #[arg(long)]
        status: String,
    },
}

pub fn run_show_json(config: &Config) -> Result<String> {
    Ok(serde_json::to_string_pretty(config)?)
}

pub fn run_schema_json() -> Result<String> {
    let schema = crate::engine::config::config_schema();
    Ok(serde_json::to_string_pretty(&schema)?)
}

#[allow(clippy::too_many_arguments)]
pub fn run_add_type(
    root: &Path,
    fs: &dyn FileSystem,
    name: &str,
    plural: &str,
    dir: &str,
    prefix: &str,
    icon: Option<&str>,
    parent_type: Option<&str>,
    singleton: bool,
    store: Option<&str>,
    numbering: Option<&str>,
    intent: Option<&str>,
    authorship: Option<&str>,
    clickup_task_type: Option<i64>,
    attributes: &[String],
) -> Result<()> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let mut config = Config::parse(&src)?;

    if config.type_by_name(name).is_some() {
        bail!("type \"{}\" already exists", name);
    }

    let attributes = attributes
        .iter()
        .map(|spec| parse_attr_spec(spec))
        .collect::<Result<Vec<_>>>()?;
    if let Some(dup) = attributes
        .iter()
        .enumerate()
        .find(|(i, a)| attributes[..*i].iter().any(|b| b.name == a.name))
    {
        bail!("attribute \"{}\" is declared more than once", dup.1.name);
    }

    config.documents.types.push(TypeDef {
        name: name.to_string(),
        plural: plural.to_string(),
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: icon.map(str::to_string),
        numbering: numbering
            .map(parse_numbering)
            .transpose()?
            .unwrap_or_default(),
        subdirectory: false,
        store: store.map(parse_store).transpose()?.unwrap_or_default(),
        singleton,
        parent_type: parent_type.map(str::to_string),
        agents: Vec::new(),
        intent: intent.map(str::to_string),
        authorship: authorship
            .map(parse_authorship)
            .transpose()?
            .unwrap_or_default(),
        lifecycle: Lifecycle::default(),
        attributes,
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        clickup_list_id: None,
        clickup_task_type,
        clickup_custom_field_map: None,
    });

    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(())
}

pub fn run_set_lifecycle(
    root: &Path,
    fs: &dyn FileSystem,
    name: &str,
    states: &[String],
    edges: &[String],
) -> Result<()> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let mut config = Config::parse(&src)?;

    let parsed_edges = edges.iter().map(|e| parse_edge(e)).collect::<Result<_>>()?;
    let lifecycle = Lifecycle {
        states: states.to_vec(),
        edges: parsed_edges,
    };

    let Some(type_def) = config.documents.types.iter_mut().find(|t| t.name == name) else {
        bail!("unknown type \"{}\"", name);
    };
    type_def.lifecycle = lifecycle;

    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(())
}

pub fn run_add_gate(root: &Path, fs: &dyn FileSystem, name: &str, status: &str) -> Result<()> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let mut config = Config::parse(&src)?;

    let rule = config.rules.iter_mut().find(|r| rule_name(r) == name);
    match rule {
        None => bail!("unknown rule \"{}\"", name),
        Some(ValidationRule::RelationExistence { .. }) => {
            bail!(
                "rule \"{}\" is a relation-existence rule; gates apply only to parent-child rules",
                name
            )
        }
        Some(ValidationRule::ParentChild {
            require_parent_status,
            ..
        }) => {
            *require_parent_status = Some(status.to_string());
        }
    }

    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(())
}

fn rule_name(rule: &ValidationRule) -> &str {
    match rule {
        ValidationRule::ParentChild { name, .. } => name,
        ValidationRule::RelationExistence { name, .. } => name,
    }
}

fn parse_edge(spec: &str) -> Result<Edge> {
    let Some((from, to)) = spec.split_once(':') else {
        bail!("edge \"{}\" must be FROM:TO", spec);
    };
    if from.is_empty() || to.is_empty() {
        bail!("edge \"{}\" must be FROM:TO with both ends set", spec);
    }
    Ok(Edge {
        from: from.to_string(),
        to: to.to_string(),
    })
}

// An `--attribute` spec: NAME:KIND, then any of a literal `required` segment
// and (for enum kinds only) one comma-separated values segment, in either
// order -- mirroring `--edge FROM:TO`'s colon-spec style.
fn parse_attr_spec(spec: &str) -> Result<AttrDef> {
    let mut parts = spec.split(':');
    let name = parts.next().unwrap_or_default();
    if name.is_empty() {
        bail!(
            "attribute \"{}\" must be NAME:KIND[:required][:VALUES]",
            spec
        );
    }
    let Some(kind_str) = parts.next() else {
        bail!(
            "attribute \"{}\" must be NAME:KIND[:required][:VALUES]",
            spec
        );
    };
    let kind = parse_attr_kind(kind_str)?;

    let mut required = false;
    let mut values: Vec<String> = Vec::new();
    for part in parts {
        if part == "required" {
            required = true;
        } else if values.is_empty() && !part.is_empty() {
            values = part.split(',').map(str::to_string).collect();
        } else {
            bail!(
                "attribute \"{}\" has an unrecognized segment \"{}\"",
                spec,
                part
            );
        }
    }
    if kind == AttrKind::Enum && values.is_empty() {
        bail!(
            "enum attribute \"{}\" needs values: {}:enum[:required]:VAL1,VAL2,...",
            name,
            name
        );
    }
    if kind != AttrKind::Enum && !values.is_empty() {
        bail!(
            "attribute \"{}\" declares values, which only enum kinds accept",
            name
        );
    }
    Ok(AttrDef {
        name: name.to_string(),
        kind,
        required,
        values,
    })
}

fn parse_attr_kind(value: &str) -> Result<AttrKind> {
    match value {
        "int" => Ok(AttrKind::Int),
        "float" => Ok(AttrKind::Float),
        "string" => Ok(AttrKind::Str),
        "enum" => Ok(AttrKind::Enum),
        "date" => Ok(AttrKind::Date),
        "bool" => Ok(AttrKind::Bool),
        other => bail!("unknown attribute kind \"{}\"", other),
    }
}

fn parse_numbering(value: &str) -> Result<NumberingStrategy> {
    match value {
        "incremental" => Ok(NumberingStrategy::Incremental),
        "sqids" => Ok(NumberingStrategy::Sqids),
        "reserved" => Ok(NumberingStrategy::Reserved),
        other => bail!("unknown numbering strategy \"{}\"", other),
    }
}

fn parse_store(value: &str) -> Result<StoreBackend> {
    match value {
        "filesystem" => Ok(StoreBackend::Filesystem),
        "github-issues" => Ok(StoreBackend::GithubIssues),
        "github-milestones" => Ok(StoreBackend::GithubMilestones),
        "github-projects" => Ok(StoreBackend::GithubProjects),
        "git-ref" => Ok(StoreBackend::GitRef),
        "clickup-tasks" => Ok(StoreBackend::ClickupTasks),
        other => bail!("unknown store backend \"{}\"", other),
    }
}

fn parse_authorship(value: &str) -> Result<Authorship> {
    match value {
        "human" => Ok(Authorship::Human),
        "assisted" => Ok(Authorship::Assisted),
        "generated" => Ok(Authorship::Generated),
        other => bail!("unknown authorship \"{}\"", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fs::RealFileSystem;
    use serde_json::Value;
    use std::path::PathBuf;

    // A config carrying lifecycles, a directional relationship, a parent-child
    // rule, and a relation-existence rule -- with standalone and inline comments
    // and a non-default section order -- so the preservation tests have decor and
    // ordering to protect.
    const SRC: &str = r#"# lazyspec configuration
[naming]
pattern = "{type}-{n:03}-{title}.md"  # filename template

[templates]
dir = ".lazyspec/templates"

# document types follow
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
intent = "propose a design"
lifecycle = { states = ["draft", "review"], edges = [{ from = "draft", to = "review" }] }

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
parent_type = "rfc"
lifecycle = { states = ["draft", "done"], edges = [{ from = "draft", to = "done" }] }

[[relationships]]
name = "implements"
inverse = "implemented-by"

# the gateable rule
[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning"

[[rules]]
name = "adrs-need-relations"
shape = "relation-existence"
type = "adr"
require = "any-relation"
severity = "error"
"#;

    fn fixture(src: &str) -> (tempfile::TempDir, PathBuf, RealFileSystem) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".lazyspec.toml");
        std::fs::write(&path, src).unwrap();
        (dir, path, RealFileSystem)
    }

    fn show(src: &str) -> Value {
        let config = Config::parse(src).unwrap();
        serde_json::from_str(&run_show_json(&config).unwrap()).unwrap()
    }

    fn type_named<'a>(json: &'a Value, name: &str) -> &'a Value {
        json["types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap()
    }

    fn rule_named<'a>(json: &'a Value, name: &str) -> &'a Value {
        json["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap()
    }

    // `config schema` emits parseable JSON, and the flagless call needs no project
    // state (it never touches a .lazyspec.toml).
    #[test]
    fn schema_json_is_valid_json() {
        let out = run_schema_json().unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert!(
            parsed.is_object(),
            "schema should be a JSON object: {parsed}"
        );
    }

    // AC1: every type serializes with all three STORY-145 axes, and the lifecycle
    // carries both states and edges.
    #[test]
    fn show_json_emits_type_axes() {
        let json = show(SRC);
        for ty in json["types"].as_array().unwrap() {
            assert!(ty.get("intent").is_some(), "intent axis present: {ty}");
            assert!(
                ty.get("authorship").is_some(),
                "authorship axis present: {ty}"
            );
            let lifecycle = &ty["lifecycle"];
            assert!(lifecycle["states"].is_array());
            assert!(lifecycle["edges"].is_array());
        }
        let rfc = type_named(&json, "rfc");
        assert_eq!(rfc["intent"], "propose a design");
        assert_eq!(rfc["authorship"], "assisted");
        assert_eq!(rfc["lifecycle"]["states"][0], "draft");
        assert_eq!(rfc["lifecycle"]["edges"][0]["from"], "draft");
        assert_eq!(rfc["lifecycle"]["edges"][0]["to"], "review");
    }

    // AC2: relationships and rules arrays serialize out, and a parent-child rule
    // can carry require_parent_status. Guards against a future #[serde(skip)].
    #[test]
    fn show_json_emits_relationships_rules_and_gate() {
        let json = show(SRC);
        assert!(json["relationships"].is_array());
        assert!(json["rules"].is_array());

        let gated = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning"
require_parent_status = "accepted"
"#;
        let json = show(gated);
        assert_eq!(
            rule_named(&json, "stories-need-rfcs")["require_parent_status"],
            "accepted"
        );
    }

    // AC3: add-type appends the type with the supplied fields and is idempotent
    // (a second show is byte-identical).
    #[test]
    fn add_type_round_trips() {
        let (_dir, path, fs) = fixture(SRC);
        run_add_type(
            path.parent().unwrap(),
            &fs,
            "spike",
            "spikes",
            "docs/spikes",
            "SPIKE",
            Some("◆"),
            Some("rfc"),
            true,
            None,
            None,
            Some("throwaway exploration"),
            Some("generated"),
            None,
            &[],
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let json = show(&after);
        let spike = type_named(&json, "spike");
        assert_eq!(spike["plural"], "spikes");
        assert_eq!(spike["dir"], "docs/spikes");
        assert_eq!(spike["prefix"], "SPIKE");
        assert_eq!(spike["icon"], "◆");
        assert_eq!(spike["parent_type"], "rfc");
        assert_eq!(spike["singleton"], true);
        assert_eq!(spike["intent"], "throwaway exploration");
        assert_eq!(spike["authorship"], "generated");

        let first = run_show_json(&Config::parse(&after).unwrap()).unwrap();
        let second = run_show_json(&Config::parse(&after).unwrap()).unwrap();
        assert_eq!(first, second);
    }

    // STORY-213 AC2: add-type accepts --attribute NAME:KIND[:required][:VALUES]
    // specs; the written config reparses with the declared AttrDefs intact.
    #[test]
    fn add_type_writes_attribute_definitions() {
        let (_dir, path, fs) = fixture(SRC);
        run_add_type(
            path.parent().unwrap(),
            &fs,
            "bug",
            "bugs",
            "docs/bugs",
            "BUG",
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            &[
                "estimate:int".to_string(),
                "owner:string:required".to_string(),
                "severity:enum:required:low,medium,high".to_string(),
            ],
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("[[types.attributes]]"), "got: {after}");
        let bug = Config::parse(&after)
            .unwrap()
            .type_by_name("bug")
            .unwrap()
            .clone();
        assert_eq!(
            bug.attributes,
            vec![
                AttrDef {
                    name: "estimate".to_string(),
                    kind: AttrKind::Int,
                    required: false,
                    values: vec![],
                },
                AttrDef {
                    name: "owner".to_string(),
                    kind: AttrKind::Str,
                    required: true,
                    values: vec![],
                },
                AttrDef {
                    name: "severity".to_string(),
                    kind: AttrKind::Enum,
                    required: true,
                    values: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
                },
            ]
        );
    }

    #[test]
    fn attr_spec_parses_all_kinds_and_segment_order() {
        for (spec, kind) in [
            ("a:int", AttrKind::Int),
            ("a:float", AttrKind::Float),
            ("a:string", AttrKind::Str),
            ("a:date", AttrKind::Date),
            ("a:bool", AttrKind::Bool),
        ] {
            let attr = parse_attr_spec(spec).unwrap();
            assert_eq!(attr.kind, kind, "{spec}");
            assert!(!attr.required);
            assert!(attr.values.is_empty());
        }
        // `required` and the values list are accepted in either order.
        let a = parse_attr_spec("p:enum:required:low,high").unwrap();
        let b = parse_attr_spec("p:enum:low,high:required").unwrap();
        assert_eq!(a, b);
        assert!(a.required);
        assert_eq!(a.values, vec!["low".to_string(), "high".to_string()]);
    }

    #[test]
    fn attr_spec_rejects_malformed_input() {
        for (spec, needle) in [
            ("noskind", "must be NAME:KIND"),
            (":int", "must be NAME:KIND"),
            ("a:blob", "unknown attribute kind"),
            ("a:enum", "needs values"),
            ("a:int:low,high", "only enum kinds"),
            ("a:enum:low:high,mid", "unrecognized segment"),
        ] {
            let err = parse_attr_spec(spec).unwrap_err();
            assert!(err.to_string().contains(needle), "{spec}: got {}", err);
        }
    }

    #[test]
    fn add_type_rejects_duplicate_attribute_names_without_writing() {
        let (_dir, path, fs) = fixture(SRC);
        let before = std::fs::read_to_string(&path).unwrap();
        let err = run_add_type(
            path.parent().unwrap(),
            &fs,
            "bug",
            "bugs",
            "docs/bugs",
            "BUG",
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            &["estimate:int".to_string(), "estimate:bool".to_string()],
        )
        .unwrap_err();
        assert!(err.to_string().contains("more than once"), "got: {err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    // A clickup_task_type supplied to add-type is written for a clickup-tasks type
    // and surfaces in `config --json` as a number after a reload.
    #[test]
    fn add_type_writes_clickup_task_type() {
        let (_dir, path, fs) = fixture(SRC);
        run_add_type(
            path.parent().unwrap(),
            &fs,
            "task",
            "tasks",
            "docs/tasks",
            "TASK",
            None,
            None,
            false,
            Some("clickup-tasks"),
            None,
            None,
            None,
            Some(1001),
            &[],
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let json = show(&after);
        assert_eq!(type_named(&json, "task")["clickup_task_type"], 1001);
    }

    #[test]
    fn add_type_rejects_duplicate_without_writing() {
        let (_dir, path, fs) = fixture(SRC);
        let before = std::fs::read_to_string(&path).unwrap();
        let err = run_add_type(
            path.parent().unwrap(),
            &fs,
            "rfc",
            "rfcs",
            "docs/rfcs",
            "RFC",
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            &[],
        )
        .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    // AC4: set-lifecycle replaces the whole lifecycle (not a merge) and is gated
    // by an existing type.
    #[test]
    fn set_lifecycle_replaces_states_and_edges() {
        let (_dir, path, fs) = fixture(SRC);
        run_set_lifecycle(
            path.parent().unwrap(),
            &fs,
            "rfc",
            &["draft".into(), "accepted".into(), "done".into()],
            &["draft:accepted".into(), "*:rejected".into()],
        )
        .unwrap();

        let json = show(&std::fs::read_to_string(&path).unwrap());
        let lifecycle = &type_named(&json, "rfc")["lifecycle"];
        assert_eq!(
            lifecycle["states"].as_array().unwrap(),
            &vec![
                Value::from("draft"),
                Value::from("accepted"),
                Value::from("done")
            ]
        );
        let edges = lifecycle["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0]["from"], "draft");
        assert_eq!(edges[0]["to"], "accepted");
        assert_eq!(edges[1]["from"], "*");
        assert_eq!(edges[1]["to"], "rejected");
        // The old `review` state is gone -- replace, not merge.
        assert!(!lifecycle["states"]
            .as_array()
            .unwrap()
            .contains(&Value::from("review")));
    }

    #[test]
    fn set_lifecycle_rejects_unknown_type() {
        let (_dir, path, fs) = fixture(SRC);
        let err =
            run_set_lifecycle(path.parent().unwrap(), &fs, "nope", &["a".into()], &[]).unwrap_err();
        assert!(err.to_string().contains("unknown type"));
    }

    // AC5: add-gate sets require_parent_status on a parent-child rule and rejects
    // unknown rules and relation-existence targets.
    #[test]
    fn add_gate_sets_require_parent_status() {
        let (_dir, path, fs) = fixture(SRC);
        run_add_gate(path.parent().unwrap(), &fs, "stories-need-rfcs", "accepted").unwrap();
        let json = show(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(
            rule_named(&json, "stories-need-rfcs")["require_parent_status"],
            "accepted"
        );
    }

    #[test]
    fn add_gate_rejects_unknown_rule() {
        let (_dir, path, fs) = fixture(SRC);
        let err = run_add_gate(path.parent().unwrap(), &fs, "nope", "accepted").unwrap_err();
        assert!(err.to_string().contains("unknown rule"));
    }

    #[test]
    fn add_gate_rejects_relation_existence_rule() {
        let (_dir, path, fs) = fixture(SRC);
        let err = run_add_gate(
            path.parent().unwrap(),
            &fs,
            "adrs-need-relations",
            "accepted",
        )
        .unwrap_err();
        assert!(err.to_string().contains("relation-existence"));
    }

    // AC6: each mutator preserves comments and the section order of untouched
    // blocks, changes only its intended block, and emits a reparseable config.
    #[test]
    fn add_type_preserves_comments_and_order() {
        let (_dir, path, fs) = fixture(SRC);
        run_add_type(
            path.parent().unwrap(),
            &fs,
            "spike",
            "spikes",
            "docs/spikes",
            "SPIKE",
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            &[],
        )
        .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# lazyspec configuration"));
        assert!(after.contains("# filename template"));
        assert!(after.contains("# document types follow"));
        assert!(after.contains("# the gateable rule"));
        // The new block is appended to the [[types]] array (after the last type,
        // before [[relationships]]); the relationship comment still follows it.
        let spike_at = after.find(r#"name = "spike""#).unwrap();
        let rels_at = after.find("[[relationships]]").unwrap();
        assert!(spike_at < rels_at, "new type sits inside the types array");
        // Untouched blocks keep their order: types -> relationships -> rules.
        assert!(after.find("# document types follow").unwrap() < rels_at);
        assert!(rels_at < after.find("# the gateable rule").unwrap());
        Config::parse(&after).unwrap();
    }

    #[test]
    fn set_lifecycle_preserves_comments_and_only_changes_one_type() {
        let (_dir, path, fs) = fixture(SRC);
        run_set_lifecycle(
            path.parent().unwrap(),
            &fs,
            "rfc",
            &["draft".into(), "done".into()],
            &["draft:done".into()],
        )
        .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# lazyspec configuration"));
        assert!(after.contains("# document types follow"));
        assert!(after.contains("# the gateable rule"));
        // The other type's lifecycle is untouched.
        assert!(after
            .contains(r#"states = ["draft", "done"], edges = [{ from = "draft", to = "done" }]"#));
        let json = show(&after);
        // story keeps its original lifecycle.
        assert_eq!(type_named(&json, "story")["lifecycle"]["states"][1], "done");
        Config::parse(&after).unwrap();
    }

    #[test]
    fn add_gate_preserves_comments_and_only_changes_one_rule() {
        let (_dir, path, fs) = fixture(SRC);
        let before = std::fs::read_to_string(&path).unwrap();
        run_add_gate(path.parent().unwrap(), &fs, "stories-need-rfcs", "accepted").unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# lazyspec configuration"));
        assert!(after.contains("# the gateable rule"));
        // Exactly one line was added (the require_parent_status key).
        assert_eq!(after.lines().count(), before.lines().count() + 1);
        assert!(after.contains(r#"require_parent_status = "accepted""#));
        // The relation-existence rule is untouched.
        assert!(after.contains(r#"require = "any-relation""#));
        Config::parse(&after).unwrap();
    }
}
