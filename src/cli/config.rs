use crate::cli::wizard::Prompter;
use crate::engine::config::{
    AttrDef, AttrKind, Authorship, Config, Edge, Lifecycle, NumberingStrategy, Severity,
    StoreBackend, TypeDef, ValidationRule,
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
    /// Append a new document type to .lazyspec.toml. With all four positionals
    /// omitted on a TTY, prompts for the core fields interactively.
    AddType {
        /// Type name (e.g. spike)
        name: Option<String>,
        /// Plural form used for directory listings (e.g. spikes)
        plural: Option<String>,
        /// Directory the type's documents live in (e.g. docs/spikes)
        dir: Option<String>,
        /// ID prefix for the type (e.g. SPIKE)
        prefix: Option<String>,
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

    config.documents.types.push(type_def_from_parts(
        name,
        plural,
        dir,
        prefix,
        icon,
        parent_type,
        singleton,
        store.map(parse_store).transpose()?.unwrap_or_default(),
        numbering
            .map(parse_numbering)
            .transpose()?
            .unwrap_or_default(),
        intent,
        authorship
            .map(parse_authorship)
            .transpose()?
            .unwrap_or_default(),
        clickup_task_type,
        attributes,
    ));

    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(())
}

/// Assemble a `TypeDef` from already-parsed pieces. Shared by the flag path
/// (`run_add_type`) and the in-memory init wizard (`apply_collected_type`) so the
/// full field set is written the same way from both entry points.
#[allow(clippy::too_many_arguments)]
fn type_def_from_parts(
    name: &str,
    plural: &str,
    dir: &str,
    prefix: &str,
    icon: Option<&str>,
    parent_type: Option<&str>,
    singleton: bool,
    store: StoreBackend,
    numbering: NumberingStrategy,
    intent: Option<&str>,
    authorship: Authorship,
    clickup_task_type: Option<i64>,
    attributes: Vec<AttrDef>,
) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        plural: plural.to_string(),
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: icon.map(str::to_string),
        numbering,
        subdirectory: false,
        store,
        singleton,
        parent_type: parent_type.map(str::to_string),
        agents: Vec::new(),
        intent: intent.map(str::to_string),
        authorship,
        lifecycle: Lifecycle::default(),
        attributes,
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        clickup_list_id: None,
        clickup_task_type,
        clickup_custom_field_map: None,
    }
}

/// How `config add-type` should proceed given its four positionals: all four
/// present runs the flag path, all four absent prompts, and any partial mix is
/// a usage error.
#[derive(Debug, PartialEq, Eq)]
pub enum AddTypeInvocation {
    Positional,
    Prompt,
}

pub fn classify_add_type_args(positionals: [&Option<String>; 4]) -> Result<AddTypeInvocation> {
    let supplied = positionals.iter().filter(|p| p.is_some()).count();
    match supplied {
        4 => Ok(AddTypeInvocation::Positional),
        0 => Ok(AddTypeInvocation::Prompt),
        _ => bail!(
            "config add-type needs all four of name, plural, dir, prefix (or none, to prompt interactively)"
        ),
    }
}

/// The fields a single interactive type-authoring session collects, before any
/// disk write. Produced by `collect_type_interactive` and applied either to a
/// parsed-from-disk config (via `run_add_type` and friends) or to an in-memory
/// config being designed by `init` (via `apply_collected_type`).
pub struct CollectedType {
    pub name: String,
    pub plural: String,
    pub dir: String,
    pub prefix: String,
    pub icon: Option<String>,
    pub store: String,
    pub numbering: String,
    pub singleton: bool,
    pub authorship: String,
    pub attributes: Vec<String>,
    pub parent_type: Option<String>,
    pub lifecycle: Option<(Vec<String>, Vec<String>)>,
    pub gate: Option<(String, String)>,
}

/// Prompt for a type's fields on a TTY, validating each section against `config`
/// (an in-memory view of the project as it stands, so name/prefix/parent/gate
/// checks see prior additions) without touching disk. Every optional section
/// pre-validates prompt-side and re-asks on failure rather than aborting.
pub fn collect_type_interactive(
    config: &Config,
    prompter: &mut dyn Prompter,
) -> Result<CollectedType> {
    let default_authorship = match Authorship::default() {
        Authorship::Human => "human",
        Authorship::Assisted => "assisted",
        Authorship::Generated => "generated",
    };

    // Re-prompt the identity fields until neither the name nor the prefix
    // collides with an existing type, rather than aborting the whole session.
    let (name, plural, dir, prefix) = loop {
        let name = prompter.ask("Type name", None)?;
        let plural = prompter.ask("Plural", None)?;
        let dir = prompter.ask("Directory", Some(&format!("docs/{plural}")))?;
        let prefix = prompter.ask("ID prefix", Some(&name.to_uppercase()))?;

        if config.type_by_name(&name).is_some() {
            println!("type \"{name}\" already exists; choose another");
            continue;
        }
        if config.documents.types.iter().any(|t| t.prefix == prefix) {
            println!("prefix \"{prefix}\" is already in use; choose another");
            continue;
        }
        break (name, plural, dir, prefix);
    };

    let icon = prompter.ask("Icon", None)?;
    let icon = if icon.is_empty() { None } else { Some(icon) };
    let store = prompter.select(
        "Store",
        &[
            "filesystem",
            "github-issues",
            "github-milestones",
            "github-projects",
            "git-ref",
            "clickup-tasks",
        ],
        "filesystem",
    )?;
    let numbering = prompter.select(
        "Numbering",
        &["incremental", "sqids", "reserved"],
        "incremental",
    )?;
    let singleton = prompter.confirm("Singleton", false)?;
    let authorship = prompter.select(
        "Authorship",
        &["human", "assisted", "generated"],
        default_authorship,
    )?;

    // Attributes: keep offering to add one until declined. A malformed spec or a
    // name already collected re-asks in place (no fresh "add another?" prompt) so
    // a typo never costs the value or ends the session.
    let mut attributes: Vec<String> = Vec::new();
    let mut attribute_names: Vec<String> = Vec::new();
    while prompter.confirm("Add an attribute", false)? {
        loop {
            let spec = prompter.ask("Attribute (NAME:KIND[:required][:VALUES])", None)?;
            let def = match parse_attr_spec(&spec) {
                Ok(def) => def,
                Err(e) => {
                    println!("{e}; try again");
                    continue;
                }
            };
            if attribute_names.contains(&def.name) {
                println!(
                    "attribute \"{}\" was already added; choose another",
                    def.name
                );
                continue;
            }
            attribute_names.push(def.name);
            attributes.push(spec);
            break;
        }
    }

    // Parent: only an already-defined type may be chosen. Skip entirely when the
    // project has no types to point at.
    let type_names: Vec<&str> = config
        .documents
        .types
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    let parent_type = if !type_names.is_empty() && prompter.confirm("Set a parent type", false)? {
        Some(loop {
            let choice = prompter.select("Parent", &type_names, type_names[0])?;
            if type_names.contains(&choice.as_str()) {
                break choice;
            }
            println!("\"{choice}\" is not a defined type; choose one of the listed names");
        })
    } else {
        None
    };

    // Lifecycle: declining leaves the type's lifecycle empty so it inherits the
    // store preset via `effective_lifecycle`. When designing one, an edge naming a
    // state outside the collected set (source `*` excepted) re-asks in place.
    let custom_lifecycle = if prompter.confirm("Design a custom lifecycle", false)? {
        let states = loop {
            let states = prompter.multi_select("Lifecycle states", &[], &[])?;
            if states.is_empty() {
                println!("at least one state is required");
                continue;
            }
            break states;
        };
        let mut edges: Vec<String> = Vec::new();
        loop {
            let spec = prompter.ask("Edge FROM:TO (blank to finish)", None)?;
            if spec.is_empty() {
                break;
            }
            let edge = match parse_edge(&spec) {
                Ok(edge) => edge,
                Err(e) => {
                    println!("{e}; try again");
                    continue;
                }
            };
            let to_ok = states.contains(&edge.to);
            let from_ok = edge.from == "*" || states.contains(&edge.from);
            if !to_ok || !from_ok {
                println!("edge \"{spec}\" names a state that isn't in the lifecycle; try again");
                continue;
            }
            edges.push(spec);
        }
        Some((states, edges))
    } else {
        None
    };

    // Gate: attach `require_parent_status` to an existing parent-child rule. Only
    // a status the parent type's effective lifecycle carries is accepted.
    let parent_child_rules: Vec<(String, String)> = config
        .rules
        .iter()
        .filter_map(|r| match r {
            ValidationRule::ParentChild { name, parent, .. } => {
                Some((name.clone(), parent.clone()))
            }
            ValidationRule::RelationExistence { .. } => None,
        })
        .collect();
    let gate = if !parent_child_rules.is_empty()
        && prompter.confirm("Gate a parent-child rule", false)?
    {
        let rule_names: Vec<&str> = parent_child_rules.iter().map(|(n, _)| n.as_str()).collect();
        let rule = loop {
            let choice = prompter.select("Rule", &rule_names, rule_names[0])?;
            if rule_names.contains(&choice.as_str()) {
                break choice;
            }
            println!("\"{choice}\" is not a parent-child rule; choose one of the listed names");
        };
        let parent_name = &parent_child_rules
            .iter()
            .find(|(n, _)| *n == rule)
            .expect("selected rule came from this list")
            .1;
        let states = config
            .type_by_name(parent_name)
            .map(|t| t.effective_lifecycle().states.clone())
            .unwrap_or_default();
        let status = loop {
            let answer = prompter.ask("Required parent status", None)?;
            if states.contains(&answer) {
                break answer;
            }
            println!("\"{answer}\" is not a lifecycle state of \"{parent_name}\"; choose another");
        };
        Some((rule, status))
    } else {
        None
    };

    Ok(CollectedType {
        name,
        plural,
        dir,
        prefix,
        icon,
        store,
        numbering,
        singleton,
        authorship,
        attributes,
        parent_type,
        lifecycle: custom_lifecycle,
        gate,
    })
}

/// A stable, dedup-guarded name for the parent-child rule linking `child` to
/// `parent`, built from their plural forms (e.g. `stories-need-rfcs`). Falls back
/// to a naive `{name}s` plural when a type is absent, and appends `-2`, `-3`, ...
/// if the base name already names a rule.
fn parent_child_rule_name(config: &Config, child: &str, parent: &str) -> String {
    let plural = |name: &str| {
        config
            .type_by_name(name)
            .map(|t| t.plural.clone())
            .unwrap_or_else(|| format!("{name}s"))
    };
    let base = format!("{}-need-{}", plural(child), plural(parent));
    if !config.rules.iter().any(|r| rule_name(r) == base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !config.rules.iter().any(|r| rule_name(r) == candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Prompt for a single parent-child rule against `config` (an in-memory view of
/// the project as designed so far). Child and parent are each chosen from the
/// defined type names -- an unknown answer re-asks rather than aborting. Severity
/// defaults to `warning`. An optional gate re-asks until the chosen status names
/// a state in the parent type's effective lifecycle. Pure: no disk IO, fully
/// driveable by a `ScriptedPrompter`.
pub fn collect_parent_child_rule(
    config: &Config,
    prompter: &mut dyn Prompter,
) -> Result<ValidationRule> {
    let type_names: Vec<&str> = config
        .documents
        .types
        .iter()
        .map(|t| t.name.as_str())
        .collect();

    let pick = |prompter: &mut dyn Prompter, label: &str| -> Result<String> {
        loop {
            let choice = prompter.select(label, &type_names, type_names[0])?;
            if type_names.contains(&choice.as_str()) {
                break Ok(choice);
            }
            println!("\"{choice}\" is not a defined type; choose one of the listed names");
        }
    };

    let child = pick(prompter, "Child type")?;
    let parent = pick(prompter, "Parent type")?;
    let name = parent_child_rule_name(config, &child, &parent);

    let severity =
        parse_severity(&prompter.select("Severity", &["warning", "error"], "warning")?)?;

    let require_parent_status = if prompter.confirm("Gate on a parent status", false)? {
        let states = config
            .type_by_name(&parent)
            .map(|t| t.effective_lifecycle().states.clone())
            .unwrap_or_default();
        Some(loop {
            let answer = prompter.ask("Required parent status", None)?;
            if states.contains(&answer) {
                break answer;
            }
            println!("\"{answer}\" is not a lifecycle state of \"{parent}\"; choose another");
        })
    } else {
        None
    };

    Ok(ValidationRule::ParentChild {
        name,
        child,
        parent,
        severity,
        require_parent_status,
    })
}

/// Push a collected type onto an in-memory `Config` and apply its optional
/// lifecycle and gate, without any disk IO. Used by the `init` wizard, which
/// serializes the whole `Config` at the end rather than editing a file in place.
pub fn apply_collected_type(config: &mut Config, collected: &CollectedType) -> Result<()> {
    if config.type_by_name(&collected.name).is_some() {
        bail!("type \"{}\" already exists", collected.name);
    }

    let attributes = collected
        .attributes
        .iter()
        .map(|spec| parse_attr_spec(spec))
        .collect::<Result<Vec<_>>>()?;

    config.documents.types.push(type_def_from_parts(
        &collected.name,
        &collected.plural,
        &collected.dir,
        &collected.prefix,
        collected.icon.as_deref(),
        collected.parent_type.as_deref(),
        collected.singleton,
        parse_store(&collected.store)?,
        parse_numbering(&collected.numbering)?,
        None,
        parse_authorship(&collected.authorship)?,
        None,
        attributes,
    ));

    if let Some((states, edges)) = &collected.lifecycle {
        let parsed_edges = edges.iter().map(|e| parse_edge(e)).collect::<Result<_>>()?;
        let type_def = config
            .documents
            .types
            .iter_mut()
            .find(|t| t.name == collected.name)
            .expect("just-pushed type is present");
        type_def.lifecycle = Lifecycle {
            states: states.clone(),
            edges: parsed_edges,
        };
    }

    if let Some((rule, status)) = &collected.gate {
        match config.rules.iter_mut().find(|r| rule_name(r) == rule) {
            Some(ValidationRule::ParentChild {
                require_parent_status,
                ..
            }) => {
                *require_parent_status = Some(status.clone());
            }
            _ => bail!("unknown parent-child rule \"{}\"", rule),
        }
    }

    Ok(())
}

/// Prompt for a type's fields on a TTY and drive the same writers the flag path
/// uses. After the core fields it optionally collects attributes and a parent
/// type (fed to `run_add_type`), a custom lifecycle (`run_set_lifecycle`), and a
/// gate on an existing parent-child rule (`run_add_gate`). Every optional section
/// pre-validates prompt-side and re-asks on failure rather than aborting.
pub fn run_add_type_interactive(
    root: &Path,
    fs: &dyn FileSystem,
    prompter: &mut dyn Prompter,
) -> Result<()> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let config = Config::parse(&src)?;

    let CollectedType {
        name,
        plural,
        dir,
        prefix,
        icon,
        store,
        numbering,
        singleton,
        authorship,
        attributes,
        parent_type,
        lifecycle: custom_lifecycle,
        gate,
    } = collect_type_interactive(&config, prompter)?;

    run_add_type(
        root,
        fs,
        &name,
        &plural,
        &dir,
        &prefix,
        icon.as_deref(),
        parent_type.as_deref(),
        singleton,
        Some(&store),
        Some(&numbering),
        None,
        Some(&authorship),
        None,
        &attributes,
    )?;

    if let Some((states, edges)) = custom_lifecycle {
        run_set_lifecycle(root, fs, &name, &states, &edges)?;
    }
    if let Some((rule, status)) = gate {
        run_add_gate(root, fs, &rule, &status)?;
    }
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

pub fn parse_severity(value: &str) -> Result<Severity> {
    match value {
        "warning" => Ok(Severity::Warning),
        "error" => Ok(Severity::Error),
        other => bail!("unknown severity \"{}\"", other),
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
    use crate::cli::wizard::ScriptedPrompter;
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
    // Interactive add-type writes byte-for-byte the same config as the
    // equivalent flag call: same start config, same delegated TypeDef.
    #[test]
    fn interactive_add_type_matches_flag_call() {
        let (_dir_a, path_a, fs_a) = fixture(SRC);
        let mut prompter = ScriptedPrompter::new(
            [
                "spike",       // name
                "spikes",      // plural
                "docs/spikes", // dir
                "SPIKE",       // prefix
                "◆",           // icon
                "filesystem",  // store
                "incremental", // numbering
                "y",           // singleton -> true
                "generated",   // authorship
                "n",           // add an attribute? no
                "n",           // set a parent type? no
                "n",           // design a custom lifecycle? no
                "n",           // gate a parent-child rule? no
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );
        run_add_type_interactive(path_a.parent().unwrap(), &fs_a, &mut prompter).unwrap();
        let interactive_out = std::fs::read_to_string(&path_a).unwrap();

        let (_dir_b, path_b, fs_b) = fixture(SRC);
        run_add_type(
            path_b.parent().unwrap(),
            &fs_b,
            "spike",
            "spikes",
            "docs/spikes",
            "SPIKE",
            Some("◆"),
            None,
            true,
            Some("filesystem"),
            Some("incremental"),
            None,
            Some("generated"),
            None,
            &[],
        )
        .unwrap();
        let flag_out = std::fs::read_to_string(&path_b).unwrap();

        assert_eq!(interactive_out, flag_out);
    }

    // A duplicate name on the first pass re-prompts (consuming the next queued
    // identity) rather than aborting, and never writes a duplicate.
    #[test]
    fn interactive_add_type_reprompts_on_duplicate_name() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = ScriptedPrompter::new(
            [
                "rfc", "rfcs", "", "", // first pass: rfc already exists -> reprompt
                "spike", "spikes", "", "", // second pass: unique
                "", // icon
                "", // store -> filesystem
                "", // numbering -> incremental
                "", // singleton -> false
                "", // authorship -> default
                "n", "n", "n", "n", // attributes / parent / lifecycle / gate declined
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        assert_eq!(
            config
                .documents
                .types
                .iter()
                .filter(|t| t.name == "rfc")
                .count(),
            1,
            "rfc must not be duplicated"
        );
        assert!(config.type_by_name("spike").is_some());
    }

    // Blank dir and prefix answers fall back to docs/<plural> and UPPERCASE(name).
    #[test]
    fn interactive_add_type_applies_default_dir_and_prefix() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = ScriptedPrompter::new(
            [
                "task", "tasks", "", "", // dir and prefix blank -> defaults
                "", "", "", "", "", // icon / store / numbering / singleton / authorship
                "n", "n", "n", "n", // attributes / parent / lifecycle / gate declined
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        let task = config.type_by_name("task").unwrap();
        assert_eq!(task.dir, "docs/tasks");
        assert_eq!(task.prefix, "TASK");
    }

    #[test]
    fn classify_add_type_args_enforces_all_or_none() {
        let some = Some("x".to_string());
        assert_eq!(
            classify_add_type_args([&some, &some, &some, &some]).unwrap(),
            AddTypeInvocation::Positional
        );
        assert_eq!(
            classify_add_type_args([&None, &None, &None, &None]).unwrap(),
            AddTypeInvocation::Prompt
        );
        let err = classify_add_type_args([&some, &some, &None, &None]).unwrap_err();
        assert!(
            err.to_string().contains("all four"),
            "partial positionals should error: {err}"
        );
    }

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

    fn scripted(answers: &[&str]) -> ScriptedPrompter {
        ScriptedPrompter::new(answers.iter().map(|s| s.to_string()).collect())
    }

    // STORY-226 AC1: an interactively entered attribute spec is validated and
    // written into the new type's frontmatter definition.
    #[test]
    fn interactive_add_type_collects_attributes() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget",
            "widgets",
            "",
            "", // core fields (dir/prefix default)
            "",
            "",
            "",
            "",
            "", // icon/store/numbering/singleton/authorship
            "y",
            "priority:enum:low,medium,high", // add one attribute
            "n",                             // add another? no
            "n",                             // parent? no
            "n",                             // custom lifecycle? no
            "n",                             // gate? no
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        let widget = config.type_by_name("widget").unwrap();
        assert_eq!(
            widget.attributes,
            vec![AttrDef {
                name: "priority".to_string(),
                kind: AttrKind::Enum,
                required: false,
                values: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
            }]
        );
    }

    // STORY-226 AC1: a malformed attribute spec (unknown kind, enum without
    // values) is rejected and re-asked in place, not aborted, and the eventual
    // valid spec is written.
    #[test]
    fn interactive_add_type_reprompts_bad_attr() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget",
            "widgets",
            "",
            "",
            "",
            "",
            "",
            "",
            "",                       // core fields
            "y",                      // add an attribute
            "priority:bogus",         // unknown kind -> re-ask
            "priority:enum",          // enum without values -> re-ask
            "priority:enum:low,high", // valid
            "n",                      // add another? no
            "n",
            "n",
            "n", // parent / lifecycle / gate declined
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        let widget = config.type_by_name("widget").unwrap();
        assert_eq!(widget.attributes.len(), 1);
        assert_eq!(widget.attributes[0].name, "priority");
        assert_eq!(widget.attributes[0].kind, AttrKind::Enum);
        assert_eq!(
            widget.attributes[0].values,
            vec!["low".to_string(), "high".to_string()]
        );
    }

    // STORY-226 AC2: while designing a custom lifecycle, an edge naming a state
    // outside the collected set is rejected and re-asked; the valid edge is kept.
    #[test]
    fn interactive_add_type_custom_lifecycle_reprompts_bad_edge() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget",
            "widgets",
            "",
            "",
            "",
            "",
            "",
            "",
            "",           // core fields
            "n",          // no attributes
            "n",          // no parent
            "y",          // design a custom lifecycle
            "draft,done", // lifecycle states (comma-separated)
            "draft:nope", // `nope` isn't a state -> re-ask
            "draft:done", // valid
            "",           // blank to finish edges
            "n",          // no gate
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        let widget = config.type_by_name("widget").unwrap();
        assert_eq!(
            widget.lifecycle.states,
            vec!["draft".to_string(), "done".to_string()]
        );
        assert_eq!(widget.lifecycle.edges.len(), 1);
        assert_eq!(widget.lifecycle.edges[0].from, "draft");
        assert_eq!(widget.lifecycle.edges[0].to, "done");
    }

    // ITERATION-329: lifecycle states are collected in one `multi_select` prompt.
    // A comma-separated answer becomes the full state list, and a blank answer
    // trips the empty-guard re-ask before a valid answer is accepted.
    #[test]
    fn collect_type_multi_select_states_reasks_on_blank() {
        let config = Config::parse(SRC).unwrap();
        let mut prompter = scripted(&[
            "widget",
            "widgets",
            "",
            "", // core identity
            "",
            "",
            "",
            "",
            "",                  // icon/store/numbering/singleton/authorship
            "n",                 // no attribute
            "n",                 // no parent
            "y",                 // design a custom lifecycle
            "",                  // blank states -> empty-guard re-ask
            "draft,review,done", // valid states
            "",                  // blank to finish edges
            "n",                 // no gate
        ]);
        let collected = collect_type_interactive(&config, &mut prompter).unwrap();
        let (states, edges) = collected
            .lifecycle
            .expect("a custom lifecycle was designed");
        assert_eq!(
            states,
            vec![
                "draft".to_string(),
                "review".to_string(),
                "done".to_string()
            ]
        );
        assert!(edges.is_empty());
    }

    // STORY-226 AC2: declining the custom-lifecycle prompt leaves the lifecycle
    // empty, so the type inherits the store preset via effective_lifecycle.
    #[test]
    fn interactive_add_type_declined_lifecycle_inherits_preset() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget", "widgets", "", "", "", "", "", "", "", // core fields
            "n", "n", "n", "n", // attributes / parent / lifecycle / gate declined
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        let widget = config.type_by_name("widget").unwrap();
        // Empty lifecycle -> inherits the store preset via effective_lifecycle.
        assert!(widget.lifecycle.states.is_empty());
        assert!(widget.lifecycle.edges.is_empty());
        assert!(widget.effective_lifecycle().states.is_empty());
    }

    // STORY-226 AC3: only already-defined types are selectable as a parent; an
    // unknown answer is rejected and re-asked rather than accepted.
    #[test]
    fn interactive_add_type_parent_only_from_existing() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget", "widgets", "", "", "", "", "", "", "",      // core fields
            "n",     // no attributes
            "y",     // set a parent type
            "bogus", // not a defined type -> re-ask
            "rfc",   // valid existing type
            "n", "n", // lifecycle / gate declined
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let config = Config::parse(&after).unwrap();
        assert_eq!(
            config
                .type_by_name("widget")
                .unwrap()
                .parent_type
                .as_deref(),
            Some("rfc")
        );
    }

    // STORY-226 AC4: gating a parent-child rule with a status the parent's
    // lifecycle lacks is rejected and re-asked; the valid status is written.
    #[test]
    fn interactive_add_type_gate_reprompts_unknown_status() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget",
            "widgets",
            "",
            "",
            "",
            "",
            "",
            "",
            "", // core fields
            "n",
            "n",
            "n",                 // attributes / parent / lifecycle declined
            "y",                 // gate a parent-child rule
            "stories-need-rfcs", // the only parent-child rule (parent = rfc)
            "shipped",           // rfc lifecycle lacks `shipped` -> re-ask
            "review",            // rfc lifecycle has `review`
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let json = show(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(
            rule_named(&json, "stories-need-rfcs")["require_parent_status"],
            "review"
        );
    }

    // STORY-226 AC5: the full interactive flow produces a byte-identical config to
    // the equivalent add-type(+attrs+parent) -> set-lifecycle -> add-gate flag
    // chain, and the result reparses cleanly.
    #[test]
    fn interactive_full_flow_matches_flag_chain() {
        let (_dir_a, path_a, fs_a) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget",
            "widgets",
            "",
            "", // core fields (dir/prefix default)
            "",
            "",
            "",
            "",
            "", // icon/store/numbering/singleton/authorship
            "y",
            "priority:enum:low,medium,high",
            "n", // one attribute
            "y",
            "rfc", // parent = rfc
            "y",
            "draft,done",
            "draft:done",
            "", // custom lifecycle
            "y",
            "stories-need-rfcs",
            "review", // gate
        ]);
        run_add_type_interactive(path_a.parent().unwrap(), &fs_a, &mut prompter).unwrap();
        let interactive_out = std::fs::read_to_string(&path_a).unwrap();

        let (_dir_b, path_b, fs_b) = fixture(SRC);
        let root_b = path_b.parent().unwrap();
        run_add_type(
            root_b,
            &fs_b,
            "widget",
            "widgets",
            "docs/widgets",
            "WIDGET",
            None,
            Some("rfc"),
            false,
            Some("filesystem"),
            Some("incremental"),
            None,
            Some("assisted"),
            None,
            &["priority:enum:low,medium,high".to_string()],
        )
        .unwrap();
        run_set_lifecycle(
            root_b,
            &fs_b,
            "widget",
            &["draft".to_string(), "done".to_string()],
            &["draft:done".to_string()],
        )
        .unwrap();
        run_add_gate(root_b, &fs_b, "stories-need-rfcs", "review").unwrap();
        let flag_out = std::fs::read_to_string(&path_b).unwrap();

        assert_eq!(interactive_out, flag_out);
        Config::parse(&interactive_out).unwrap();
    }
}
