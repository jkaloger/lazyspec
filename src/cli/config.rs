use crate::cli::wizard::Prompter;
use crate::engine::config::{
    AttrDef, AttrKind, Authorship, Config, Edge, EdgeDef, Lifecycle, NumberingStrategy,
    RelSelector, Severity, StoreBackend, Traversal, TypeDef, TypeSelector,
};
use crate::engine::config_write::write_config_in_place;
use crate::engine::fs::FileSystem;
use anyhow::{bail, Context, Result};
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
        /// GitHub issue tag (label); only valid on store = github-issues
        #[arg(long)]
        github_issue_tag: Option<String>,
        /// GitHub issue type; only valid on store = github-issues
        #[arg(long)]
        github_issue_type: Option<String>,
        /// ClickUp list ID documents are created in; only valid on store = clickup-tasks
        #[arg(long)]
        clickup_list_id: Option<String>,
        /// ClickUp custom task type (numeric custom_item_id); only valid on store = clickup-tasks
        #[arg(long)]
        clickup_task_type: Option<i64>,
        /// A custom frontmatter attribute as NAME:KIND[:required][:VAL1,VAL2,...]
        /// (kind: int, float, string, enum, date, bool; values only for enum; repeat per attribute)
        #[arg(long = "attribute")]
        attributes: Vec<String>,
    },
    /// Replace a type's lifecycle states and status transitions
    SetLifecycle {
        /// Type name to set the lifecycle on
        name: String,
        /// A lifecycle state (repeat for each state)
        #[arg(long = "state")]
        states: Vec<String>,
        /// A permitted status transition as FROM:TO (`*` matches any source;
        /// repeat per transition). Not a DAG edge -- for those see `config add-edge`
        #[arg(long = "edge")]
        edges: Vec<String>,
    },
    /// Append a row to the `[[edges]]` table: a kind of directed edge in the
    /// document DAG, unrelated to `set-lifecycle --edge`'s status transitions
    AddEdge {
        /// Name for the row; errors and later edits address it by this
        name: String,
        /// A source type, or `*` for any type (repeat per type)
        #[arg(long = "from", required = true)]
        from: Vec<String>,
        /// A permitted target type, or `*` for any type (repeat per type)
        #[arg(long = "to", required = true)]
        to: Vec<String>,
        /// A relationship that realizes the edge, or `*` for any (repeat per relationship)
        #[arg(long = "via", required = true)]
        via: Vec<String>,
        /// Severity of the finding when the edge is absent: error or warning
        /// (omitted leaves the edge legal but not demanded)
        #[arg(long)]
        required: Option<String>,
        /// Traversal role the edge joins: chain or related (omitted names no role)
        #[arg(long)]
        traversal: Option<String>,
        /// Print the row that landed as JSON (accepted here as well as before
        /// the subcommand, as on `config show`)
        #[arg(long)]
        json: bool,
    },
    /// Change fields on an existing `[[edges]]` row. An omitted flag leaves its
    /// field as it stands; unsetting an optional has its own flag. A row is
    /// addressed by the `name` it was written with and cannot be renamed here,
    /// since the writer renames by dropping the block and appending a new one,
    /// which loses the block's comments
    SetEdge {
        /// Name of the row to edit
        name: String,
        /// The source types, replacing the ones declared, or `*` for any type
        /// (repeat per type)
        #[arg(long = "from")]
        from: Option<Vec<String>>,
        /// The permitted target types, REPLACING the ones declared rather than
        /// joining them, or `*` for any type (repeat per type)
        #[arg(long = "to")]
        to: Option<Vec<String>>,
        /// The relationships that realize the edge, replacing the ones
        /// declared, or `*` for any (repeat per relationship)
        #[arg(long = "via")]
        via: Option<Vec<String>>,
        /// Severity of the finding when the edge is absent: error or warning
        #[arg(long)]
        required: Option<String>,
        /// Drop `required`, leaving the edge legal but not demanded
        #[arg(long = "no-required", conflicts_with = "required")]
        no_required: bool,
        /// Traversal role the edge joins: chain or related
        #[arg(long)]
        traversal: Option<String>,
        /// Drop `traversal`, leaving the edge naming no role
        #[arg(long = "no-traversal", conflicts_with = "traversal")]
        no_traversal: bool,
        /// Print the row after the edit as JSON (accepted here as well as
        /// before the subcommand, as on `config show`)
        #[arg(long)]
        json: bool,
    },
    /// Drop a row from the `[[edges]]` table. A config declaring no edges is
    /// legal, so removing the last row is not refused -- the DAG it described
    /// simply stops being described
    RemoveEdge {
        /// Name of the row to remove
        name: String,
        /// Print the row that was removed as JSON (accepted here as well as
        /// before the subcommand, as on `config show`)
        #[arg(long)]
        json: bool,
    },
}

/// What an edit says about one optional field. `Option<T>` cannot say it: a
/// missing flag and a flag that clears the field are different instructions,
/// and both would be `None`.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum FieldEdit<T> {
    #[default]
    Leave,
    Unset,
    Set(T),
}

impl<T> FieldEdit<T> {
    /// The pair of flags clap collects for one optional field -- `--x VALUE`
    /// and `--no-x`, which clap already refuses together -- read as one
    /// instruction.
    pub fn from_flags(value: Option<T>, unset: bool) -> Self {
        match value {
            Some(value) => FieldEdit::Set(value),
            None if unset => FieldEdit::Unset,
            None => FieldEdit::Leave,
        }
    }
}

/// The fields `config set-edge` was told to change. `None` on a set-valued
/// position means the flag was absent, so the declared set stands; `Some` is
/// the whole new set rather than members to add.
#[derive(Debug, Default)]
pub struct EdgeEdit {
    pub from: Option<Vec<String>>,
    pub to: Option<Vec<String>>,
    pub via: Option<Vec<String>>,
    pub required: FieldEdit<String>,
    pub traversal: FieldEdit<String>,
}

/// `Config::edges` is skipped when empty so the TOML writer never emits a bare
/// `edges = []` above the tables, but the JSON contract is an always-present
/// array: an agent reading `edges` should never have to branch on null.
pub fn run_show_json(config: &Config) -> Result<String> {
    let mut value = serde_json::to_value(config)?;
    if let Some(object) = value.as_object_mut() {
        object
            .entry("edges")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    }
    Ok(serde_json::to_string_pretty(&value)?)
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
    github_issue_tag: Option<&str>,
    github_issue_type: Option<&str>,
    clickup_list_id: Option<&str>,
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
        github_issue_tag,
        github_issue_type,
        clickup_list_id,
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
    github_issue_tag: Option<&str>,
    github_issue_type: Option<&str>,
    clickup_list_id: Option<&str>,
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
        github_issue_tag: github_issue_tag.map(str::to_string),
        github_issue_type: github_issue_type.map(str::to_string),
        status_authority: None,
        clickup_list_id: clickup_list_id.map(str::to_string),
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
    pub github_issue_tag: Option<String>,
    pub github_issue_type: Option<String>,
    pub clickup_list_id: Option<String>,
    pub clickup_task_type: Option<i64>,
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

    let mut github_issue_tag = None;
    let mut github_issue_type = None;
    let mut clickup_list_id = None;
    let mut clickup_task_type = None;
    match store.as_str() {
        "clickup-tasks" => {
            let list_id = prompter.ask("ClickUp list ID", None)?;
            clickup_list_id = (!list_id.is_empty()).then_some(list_id);
            clickup_task_type = loop {
                let answer = prompter.ask("ClickUp task type (numeric custom_item_id)", None)?;
                if answer.is_empty() {
                    break None;
                }
                match answer.parse::<i64>() {
                    Ok(n) => break Some(n),
                    Err(_) => println!("\"{answer}\" is not a number; try again"),
                }
            };
        }
        "github-issues" => {
            let tag = prompter.ask("GitHub issue tag", None)?;
            github_issue_tag = (!tag.is_empty()).then_some(tag);
            let issue_type = prompter.ask("GitHub issue type", None)?;
            github_issue_type = (!issue_type.is_empty()).then_some(issue_type);
        }
        _ => {}
    }

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
        github_issue_tag,
        github_issue_type,
        clickup_list_id,
        clickup_task_type,
    })
}

/// Push a collected type onto an in-memory `Config` and apply its optional
/// lifecycle, without any disk IO. Used by the `init` wizard, which
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
        collected.github_issue_tag.as_deref(),
        collected.github_issue_type.as_deref(),
        collected.clickup_list_id.as_deref(),
        collected.clickup_task_type,
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

    Ok(())
}

/// Prompt for a type's fields on a TTY and drive the same writers the flag path
/// uses. After the core fields it optionally collects attributes and a parent
/// type (fed to `run_add_type`) and a custom lifecycle (`run_set_lifecycle`).
/// Every optional section pre-validates prompt-side and re-asks on failure
/// rather than aborting.
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
        github_issue_tag,
        github_issue_type,
        clickup_list_id,
        clickup_task_type,
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
        github_issue_tag.as_deref(),
        github_issue_type.as_deref(),
        clickup_list_id.as_deref(),
        clickup_task_type,
        &attributes,
    )?;

    if let Some((states, edges)) = custom_lifecycle {
        run_set_lifecycle(root, fs, &name, &states, &edges)?;
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

/// Append one `[[edges]]` row from the flags as given and return it, so the
/// caller can report what landed without re-reading the config.
///
/// Whether the row makes sense is the loader's question, and it is asked on the
/// next command rather than here (STORY-261 AC5). The two things checked are the
/// ones no later load can recover from: a name already in the table, which the
/// writer reconciles by and would silently rewrite, and a wildcard mixed with
/// names, which is neither selector.
#[allow(clippy::too_many_arguments)]
pub fn run_add_edge(
    root: &Path,
    fs: &dyn FileSystem,
    name: &str,
    from: &[String],
    to: &[String],
    via: &[String],
    required: Option<&str>,
    traversal: Option<&str>,
) -> Result<EdgeDef> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let mut config = Config::parse(&src)?;

    if config.edges.iter().any(|edge| edge.name == name) {
        bail!("edge \"{}\" already exists", name);
    }

    let edge = EdgeDef {
        name: name.to_string(),
        from: TypeSelector::from_names(from.to_vec()).context("reading the `--from` flags")?,
        to: TypeSelector::from_names(to.to_vec()).context("reading the `--to` flags")?,
        via: RelSelector::from_names(via.to_vec()).context("reading the `--via` flags")?,
        required: required.map(parse_severity).transpose()?,
        traversal: traversal.map(parse_traversal).transpose()?,
    };

    config.edges.push(edge.clone());
    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(edge)
}

/// What `config add-edge --json` answers with. Dictum 2: the result carries the
/// row itself, serialized the way `config --json` serializes it, so a caller
/// reads what landed rather than re-reading the config to find out.
pub fn run_add_edge_json(edge: &EdgeDef) -> Result<String> {
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "action": "edge-added",
        "name": edge.name,
        "edge": edge,
    }))?)
}

/// Apply an edit to the `[[edges]]` row `name` addresses and return the row as
/// it now stands. Untouched fields, and the decor of every block the row does
/// not own, survive: the writer edits the surviving source table in place.
///
/// This merges where `set-lifecycle` beside it replaces, which is a deliberate
/// divergence. A lifecycle is one thing spelled in two keys, so re-passing it
/// whole is no burden; an edge has six fields, and a replace spelling would
/// make changing `required` alone mean re-passing `from`, `to` and `via` --
/// getting one of them wrong silently rewrites the DAG. So an omitted flag
/// means "leave it", and the two optionals get explicit `--no-` flags, because
/// omitting `--required` cannot mean both "leave it" and "remove it".
///
/// The one position that does replace is a set: repeated `--to` gives the new
/// set, not additions to the old one, or a set could never be shrunk from the
/// CLI. The TUI answers the same question with a picker that adds and removes
/// members (STORY-260); two surfaces, two affordances, one resulting row.
///
/// `name` is an address, not a field. Renaming a row is remove-old +
/// append-new to the writer, which would drop the block's comments and move it
/// to the end of the table -- accepted for a *translated* block (ADR-032) but
/// not for an edited one (ITERATION-388) -- so there is no `--name`, and a
/// rename is an edit to the file, where the decor at stake is visible.
///
/// Whether the edited row makes sense stays the loader's question, asked on the
/// next command (STORY-261 AC5), exactly as it is for `add-edge`.
pub fn run_set_edge(
    root: &Path,
    fs: &dyn FileSystem,
    name: &str,
    edit: &EdgeEdit,
) -> Result<EdgeDef> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let mut config = Config::parse(&src)?;

    let Some(edge) = config.edges.iter_mut().find(|edge| edge.name == name) else {
        bail!("unknown edge \"{}\"", name);
    };

    if let Some(from) = &edit.from {
        edge.from = TypeSelector::from_names(from.clone()).context("reading the `--from` flags")?;
    }
    if let Some(to) = &edit.to {
        edge.to = TypeSelector::from_names(to.clone()).context("reading the `--to` flags")?;
    }
    if let Some(via) = &edit.via {
        edge.via = RelSelector::from_names(via.clone()).context("reading the `--via` flags")?;
    }
    match &edit.required {
        FieldEdit::Leave => {}
        FieldEdit::Unset => edge.required = None,
        FieldEdit::Set(value) => edge.required = Some(parse_severity(value)?),
    }
    match &edit.traversal {
        FieldEdit::Leave => {}
        FieldEdit::Unset => edge.traversal = None,
        FieldEdit::Set(value) => edge.traversal = Some(parse_traversal(value)?),
    }
    let edited = edge.clone();

    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(edited)
}

/// [`run_add_edge_json`] for an edit: the same envelope, carrying the row after
/// the edit rather than the row that was appended.
pub fn run_set_edge_json(edge: &EdgeDef) -> Result<String> {
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "action": "edge-updated",
        "name": edge.name,
        "edge": edge,
    }))?)
}

/// Drop the `[[edges]]` row `name` addresses and return it as it stood.
///
/// Deletion needs no writer of its own: `write_config_in_place` reconciles the
/// table to the buffer by name, so a name the buffer no longer carries is a
/// block that goes, its own comments with it, and its neighbours keep theirs.
/// The last row taking the whole `[[edges]]` table with it falls out of the same
/// mechanism -- an emptied array-of-tables renders as nothing.
///
/// `retain` drops *every* row carrying the name, not the first one. A config
/// declaring two rows under one name does not load at all, so the `Config::parse`
/// above is what reports that and the case is unreachable here; the total
/// spelling is the one that stays right if the collision guard ever loosens,
/// rather than leaving half a pair behind.
///
/// A config declaring no edges is legal -- strict load demands no minimum -- so
/// removing the last row is not refused, and neither is removing a row whose
/// absence changes what `validate` reports: an edge condition never refuses a
/// command (RFC-067). Dropping a `required` row silences its findings and
/// dropping a `traversal` row shortens every chain that walked it. Neither is
/// warned about here, which is why the whole row comes back: a caller that wants
/// to say so cannot re-read what is gone.
pub fn run_remove_edge(root: &Path, fs: &dyn FileSystem, name: &str) -> Result<EdgeDef> {
    let path = root.join(".lazyspec.toml");
    let src = fs.read_to_string(&path)?;
    let mut config = Config::parse(&src)?;

    let Some(removed) = config.edges.iter().find(|edge| edge.name == name).cloned() else {
        bail!("unknown edge \"{}\"", name);
    };
    config.edges.retain(|edge| edge.name != name);

    let out = write_config_in_place(&src, &config)?;
    fs.write(&path, &out)?;
    Ok(removed)
}

/// [`run_add_edge_json`] for a removal: the same envelope, carrying the row as
/// it stood before it went.
pub fn run_remove_edge_json(edge: &EdgeDef) -> Result<String> {
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "action": "edge-removed",
        "name": edge.name,
        "edge": edge,
    }))?)
}

fn parse_severity(value: &str) -> Result<Severity> {
    match value {
        "error" => Ok(Severity::Error),
        "warning" => Ok(Severity::Warning),
        other => bail!("unknown required severity \"{}\" (error or warning)", other),
    }
}

fn parse_traversal(value: &str) -> Result<Traversal> {
    match value {
        "chain" => Ok(Traversal::Chain),
        "related" => Ok(Traversal::Related),
        other => bail!("unknown traversal \"{}\" (chain or related)", other),
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
    use crate::cli::wizard::ScriptedPrompter;
    use crate::cli::{Cli, Commands};
    use crate::engine::fs::RealFileSystem;
    use clap::Parser;
    use serde_json::Value;
    use std::path::PathBuf;

    // A config carrying lifecycles and a directional relationship -- with
    // standalone and inline comments and a non-default section order -- so the
    // preservation tests have decor and ordering to protect. It declares no
    // `[[edges]]`, which `show_json_emits_an_empty_edge_array_when_none_are_declared`
    // reads, and no `[[rules]]`, which strict load refuses (STORY-259).
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

# the relationship the hierarchy runs on
[[relationships]]
name = "implements"
inverse = "implemented-by"
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

    #[test]
    fn show_json_emits_declared_edges_with_every_field() {
        let src = format!(
            "{SRC}{}",
            r#"
[[edges]]
name = "stories-implement-rfcs"
from = "story"
to = ["rfc", "story"]
via = "implements"
required = "error"
traversal = "chain"
"#
        );

        let edge = &show(&src)["edges"][0];

        assert_eq!(edge["name"], "stories-implement-rfcs");
        assert_eq!(edge["from"], "story");
        assert_eq!(edge["to"], serde_json::json!(["rfc", "story"]));
        assert_eq!(edge["via"], "implements");
        assert_eq!(edge["required"], "error");
        assert_eq!(edge["traversal"], "chain");
    }

    // Dictum 2: an agent reading `edges` should never have to branch on null.
    #[test]
    fn show_json_emits_an_empty_edge_array_when_none_are_declared() {
        assert_eq!(show(SRC)["edges"], serde_json::json!([]));
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

    // AC2: the relationships array serializes out. Guards against a future
    // #[serde(skip)].
    //
    // `rules` is not asserted here any more. Strict load refuses a config that
    // declares any (STORY-259), so every loaded config's is empty, and an
    // empty one is skipped by the serializer — there is no fixture that can
    // put a rule in it to name.
    #[test]
    fn show_json_emits_relationships() {
        let json = show(SRC);
        assert!(json["relationships"].is_array());
        assert_eq!(json["relationships"][0]["name"], "implements");
        assert_eq!(json["relationships"][0]["inverse"], "implemented-by");
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
            None,
            None,
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

    // The GitHub tag/type and ClickUp list-id string flags supplied to add-type
    // are written onto the TypeDef and surface in `config --json` after a reload.
    #[test]
    fn add_type_writes_remote_string_fields() {
        let (_dir, path, fs) = fixture(SRC);
        run_add_type(
            path.parent().unwrap(),
            &fs,
            "issue",
            "issues",
            "docs/issues",
            "ISSUE",
            None,
            None,
            false,
            None, // default store; the remote fields are written regardless
            None,
            None,
            None,
            Some("Bug"),     // github_issue_tag
            Some("Defect"),  // github_issue_type
            Some("list-42"), // clickup_list_id
            None,
            &[],
        )
        .unwrap();

        let json = show(&std::fs::read_to_string(&path).unwrap());
        let issue = type_named(&json, "issue");
        assert_eq!(issue["github_issue_tag"], "Bug");
        assert_eq!(issue["github_issue_type"], "Defect");
        assert_eq!(issue["clickup_list_id"], "list-42");
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
            None,
            None,
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

    fn add_stories_implement_rfcs(root: &Path, fs: &dyn FileSystem) -> Result<EdgeDef> {
        run_add_edge(
            root,
            fs,
            "stories-implement-rfcs",
            &["story".to_string()],
            &["rfc".to_string(), "story".to_string()],
            &["implements".to_string()],
            Some("error"),
            Some("chain"),
        )
    }

    // STORY-261 AC1: add-edge appends an `[[edges]]` row carrying every flag,
    // and the config it writes loads with that row in it.
    #[test]
    fn add_edge_writes_a_row_carrying_every_flag() {
        let (_dir, path, fs) = fixture(SRC);

        let written = add_stories_implement_rfcs(path.parent().unwrap(), &fs).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let loaded = Config::parse(&after).unwrap();
        assert_eq!(loaded.edges, vec![written]);
        let edge = &loaded.edges[0];
        assert_eq!(edge.name, "stories-implement-rfcs");
        assert_eq!(edge.from, TypeSelector::Types(vec!["story".to_string()]));
        assert_eq!(
            edge.to,
            TypeSelector::Types(vec!["rfc".to_string(), "story".to_string()])
        );
        assert_eq!(edge.via, RelSelector::Named(vec!["implements".to_string()]));
        assert_eq!(edge.required, Some(Severity::Error));
        assert_eq!(edge.traversal, Some(Traversal::Chain));
        assert!(
            after.contains("# filename template"),
            "the fixture's comments must survive: {after}"
        );
    }

    // A second row joins the table rather than replacing the first: the writer
    // reconciles rows by `name`, so a differently named row is an append.
    #[test]
    fn add_edge_appends_a_second_row_beside_the_first() {
        let (_dir, path, fs) = fixture(SRC);
        let root = path.parent().unwrap();
        add_stories_implement_rfcs(root, &fs).unwrap();

        run_add_edge(
            root,
            &fs,
            "rfcs-relate-to-stories",
            &["rfc".to_string()],
            &["story".to_string()],
            &["implements".to_string()],
            None,
            None,
        )
        .unwrap();

        let edges = Config::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap()
            .edges;
        let names: Vec<&str> = edges.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["stories-implement-rfcs", "rfcs-relate-to-stories"]);
        assert_eq!(edges[0].required, Some(Severity::Error));
        // Absent stays absent: the second row states no requiredness at all.
        assert_eq!(edges[1].required, None);
        assert_eq!(edges[1].traversal, None);
    }

    // A row is addressed by its name and the writer reconciles by it, so a
    // second row under a live name would silently rewrite the first.
    #[test]
    fn add_edge_rejects_a_duplicate_name_without_writing() {
        let (_dir, path, fs) = fixture(SRC);
        let root = path.parent().unwrap();
        add_stories_implement_rfcs(root, &fs).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let err = run_add_edge(
            root,
            &fs,
            "stories-implement-rfcs",
            &["story".to_string()],
            &["rfc".to_string()],
            &["implements".to_string()],
            None,
            None,
        )
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("stories-implement-rfcs") && err.contains("already"),
            "the refusal must name the live row: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "a refused add-edge must leave the config alone"
        );
    }

    // `--to '*'` is the wildcard position, which is written as a bare string:
    // `to = ["*"]` is a wildcard inside a list, which strict load refuses.
    #[test]
    fn add_edge_writes_a_wildcard_target_as_a_bare_string() {
        let (_dir, path, fs) = fixture(SRC);

        run_add_edge(
            path.parent().unwrap(),
            &fs,
            "rfcs-relate-to-anything",
            &["rfc".to_string()],
            &["*".to_string()],
            &["implements".to_string()],
            None,
            None,
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains(r#"to = "*""#), "got: {after}");
        assert_eq!(
            Config::parse(&after).unwrap().edges[0].to,
            TypeSelector::Any
        );
    }

    // Repeated flags are the one place `["story", "*"]` can be assembled, and
    // it is neither a wildcard nor a set of names. Refused with the config
    // untouched, rather than written for the next load to reject.
    #[test]
    fn add_edge_rejects_a_wildcard_mixed_with_type_names() {
        let (_dir, path, fs) = fixture(SRC);
        let before = std::fs::read_to_string(&path).unwrap();

        let err = run_add_edge(
            path.parent().unwrap(),
            &fs,
            "mixed-targets",
            &["rfc".to_string()],
            &["story".to_string(), "*".to_string()],
            &["implements".to_string()],
            None,
            None,
        )
        .unwrap_err();

        let rendered = format!("{err:#}");
        assert!(
            rendered.contains("--to") && rendered.contains('*'),
            "the refusal must name the flag that assembled it: {rendered}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "a refused add-edge must leave the config alone"
        );
    }

    // STORY-261 AC4: the result object carries the row itself, spelled exactly
    // as `config --json` spells it -- two spellings of one row is how the
    // command's answer and the config's answer drift.
    #[test]
    fn add_edge_json_carries_the_row_config_show_reports() {
        let (_dir, path, fs) = fixture(SRC);
        let written = add_stories_implement_rfcs(path.parent().unwrap(), &fs).unwrap();

        let envelope: Value = serde_json::from_str(&run_add_edge_json(&written).unwrap()).unwrap();

        assert_eq!(envelope["action"], "edge-added");
        assert_eq!(envelope["name"], "stories-implement-rfcs");
        let shown = show(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(envelope["edge"], shown["edges"][0]);
    }

    // The types the `[[edges]]` fixtures below name, beyond the `rfc` and
    // `story` `SRC` already declares.
    const EDGE_TYPES: &str = r#"
[[types]]
name = "spike"
plural = "spikes"
dir = "docs/spikes"
prefix = "SPIKE"
lifecycle = { states = ["draft"], edges = [] }

[[types]]
name = "bug"
plural = "bugs"
dir = "docs/bugs"
prefix = "BUG"
lifecycle = { states = ["draft"], edges = [] }
"#;

    // The row the edit and removal tests address, decorated both ways a block
    // can be: a standalone comment above it and an inline comment on a key
    // inside it, so a mutation has decor to lose.
    const EDGE_BLOCK: &str = r#"
# the edge the set-edge and remove-edge tests address
[[edges]]
name = "stories-implement-rfcs"
from = "story"
to = ["rfc", "spike", "bug"]  # the target set
via = "implements"
required = "error"
traversal = "chain"
"#;

    // Neighbours for [`EDGE_BLOCK`], each decorated too, so removing the middle
    // of three rows has comments either side of it to leave alone.
    const EDGE_BLOCK_BEFORE: &str = r#"
# the row above the one that goes
[[edges]]
name = "rfcs-implement-rfcs"
from = "rfc"
to = "rfc"  # a superseding RFC
via = "implements"
"#;

    const EDGE_BLOCK_AFTER: &str = r#"
# the row below the one that goes
[[edges]]
name = "bugs-implement-stories"
from = "bug"
to = "story"
via = "implements"
traversal = "related"
"#;

    // `SRC` plus the vocabulary and one decorated `[[edges]]` row.
    fn edged_src() -> String {
        format!("{SRC}{EDGE_TYPES}{EDGE_BLOCK}")
    }

    // [`edged_src`] with a decorated row either side of the addressed one.
    fn three_edged_src() -> String {
        format!("{SRC}{EDGE_TYPES}{EDGE_BLOCK_BEFORE}{EDGE_BLOCK}{EDGE_BLOCK_AFTER}")
    }

    fn set_edge(root: &Path, fs: &dyn FileSystem, edit: EdgeEdit) -> Result<EdgeDef> {
        run_set_edge(root, fs, "stories-implement-rfcs", &edit)
    }

    fn names(values: &[&str]) -> Option<Vec<String>> {
        Some(values.iter().map(|v| v.to_string()).collect())
    }

    // The lines that differ between two renderings of one file, paired
    // old-to-new. An edit that touches one key shows up here as one pair; a
    // dropped comment or a reordered block shows up as several.
    fn changed_lines<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        assert_eq!(
            before.lines().count(),
            after.lines().count(),
            "an edit that adds or removes a line cannot be compared line-for-line:\n{after}"
        );
        before
            .lines()
            .zip(after.lines())
            .filter(|(old, new)| old != new)
            .collect()
    }

    // STORY-261 AC2: `set-edge` merges rather than replaces, so a lone
    // `--required` leaves every other field of the row -- and every comment in
    // the file, which a reparse could not tell you had been dropped -- alone.
    #[test]
    fn set_edge_changes_only_the_field_it_was_given() {
        let src = edged_src();
        let (_dir, path, fs) = fixture(&src);

        let updated = set_edge(
            path.parent().unwrap(),
            &fs,
            EdgeEdit {
                required: FieldEdit::Set("warning".to_string()),
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        assert_eq!(updated.required, Some(Severity::Warning));
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            changed_lines(&src, &after),
            [(r#"required = "error""#, r#"required = "warning""#)],
            "got: {after}"
        );
        let loaded = Config::parse(&after).unwrap();
        assert_eq!(loaded.edges, vec![updated]);
    }

    // The target set replaces; it does not accumulate. Shrinking it to one name
    // also changes the TOML's shape, since a one-member set re-emits bare.
    #[test]
    fn set_edge_shrinking_the_target_set_drops_the_members_not_named() {
        let (_dir, path, fs) = fixture(&edged_src());

        set_edge(
            path.parent().unwrap(),
            &fs,
            EdgeEdit {
                to: names(&["rfc"]),
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains(r#"to = "rfc"  # the target set"#),
            "the set re-emits bare and keeps the key's own comment: {after}"
        );
        assert_eq!(
            Config::parse(&after).unwrap().edges[0].to,
            TypeSelector::Types(vec!["rfc".to_string()])
        );
    }

    #[test]
    fn set_edge_growing_the_target_set_re_emits_it_as_a_list() {
        let (_dir, path, fs) = fixture(&edged_src());
        let root = path.parent().unwrap();
        set_edge(
            root,
            &fs,
            EdgeEdit {
                to: names(&["rfc"]),
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        set_edge(
            root,
            &fs,
            EdgeEdit {
                to: names(&["rfc", "spike"]),
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains(r#"to = ["rfc", "spike"]"#), "got: {after}");
        assert_eq!(
            Config::parse(&after).unwrap().edges[0].to,
            TypeSelector::Types(vec!["rfc".to_string(), "spike".to_string()])
        );
    }

    // Unsetting is its own spelling, because omitting `--required` already
    // means "leave it". `required` is skipped when absent, so removing the key
    // is observable in the file rather than written back as a default.
    #[test]
    fn set_edge_no_required_removes_the_key_rather_than_defaulting_it() {
        let (_dir, path, fs) = fixture(&edged_src());

        let updated = set_edge(
            path.parent().unwrap(),
            &fs,
            EdgeEdit {
                required: FieldEdit::Unset,
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        assert_eq!(updated.required, None);
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(!after.contains("required ="), "got: {after}");
        assert_eq!(Config::parse(&after).unwrap().edges[0].required, None);
    }

    #[test]
    fn set_edge_no_traversal_removes_the_key_rather_than_defaulting_it() {
        let (_dir, path, fs) = fixture(&edged_src());

        set_edge(
            path.parent().unwrap(),
            &fs,
            EdgeEdit {
                traversal: FieldEdit::Unset,
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(!after.contains("traversal ="), "got: {after}");
        assert_eq!(Config::parse(&after).unwrap().edges[0].traversal, None);
    }

    // A name that addresses no row is a CLI-argument error, not a config-
    // validity one, so it reads like `set-lifecycle`'s unknown type.
    #[test]
    fn set_edge_rejects_an_unknown_name_without_writing() {
        let src = edged_src();
        let (_dir, path, fs) = fixture(&src);

        let err = run_set_edge(
            path.parent().unwrap(),
            &fs,
            "stories-implement-spikes",
            &EdgeEdit {
                required: FieldEdit::Unset,
                ..EdgeEdit::default()
            },
        )
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("unknown edge") && err.contains("stories-implement-spikes"),
            "the refusal must name the row asked for: {err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), src);
    }

    // A bad severity is refused before anything reaches disk, so the row keeps
    // the severity it had rather than half of the edit.
    #[test]
    fn set_edge_rejects_an_unknown_severity_without_writing() {
        let src = edged_src();
        let (_dir, path, fs) = fixture(&src);

        let err = set_edge(
            path.parent().unwrap(),
            &fs,
            EdgeEdit {
                to: names(&["rfc"]),
                required: FieldEdit::Set("nonsense".to_string()),
                ..EdgeEdit::default()
            },
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("nonsense"), "got: {err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), src);
    }

    // STORY-261 AC4: the envelope carries the row after the edit, spelled the
    // way `config --json` spells it.
    #[test]
    fn set_edge_json_carries_the_row_config_show_reports() {
        let (_dir, path, fs) = fixture(&edged_src());
        let updated = set_edge(
            path.parent().unwrap(),
            &fs,
            EdgeEdit {
                required: FieldEdit::Set("warning".to_string()),
                ..EdgeEdit::default()
            },
        )
        .unwrap();

        let envelope: Value = serde_json::from_str(&run_set_edge_json(&updated).unwrap()).unwrap();

        assert_eq!(envelope["action"], "edge-updated");
        assert_eq!(envelope["name"], "stories-implement-rfcs");
        let shown = show(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(envelope["edge"], shown["edges"][0]);
    }

    // The flag set is the contract (convention §"CLI Patterns"). A row's name
    // is its address, and renaming it through the writer is remove-old +
    // append-new -- so `set-edge` offers no `--name` and clap refuses it.
    #[test]
    fn set_edge_offers_no_rename_flag() {
        let parsed = Cli::try_parse_from([
            "lazyspec",
            "config",
            "set-edge",
            "stories-implement-rfcs",
            "--name",
            "stories-implement-anything",
        ]);

        assert!(parsed.is_err(), "set-edge must not accept a rename");
    }

    // An empty target set is a config the loader refuses, and it is unreachable
    // here: each `--to` occurrence takes a value, so `--to` alone is a parse
    // error and an absent `--to` means "leave the set alone". Confirmed rather
    // than guarded (ITERATION-393 §Out of scope).
    #[test]
    fn set_edge_cannot_be_given_an_empty_target_set() {
        let parsed = Cli::try_parse_from([
            "lazyspec",
            "config",
            "set-edge",
            "stories-implement-rfcs",
            "--to",
        ]);

        assert!(parsed.is_err(), "`--to` with no value must not parse");
        let absent =
            Cli::try_parse_from(["lazyspec", "config", "set-edge", "stories-implement-rfcs"])
                .unwrap();
        let Some(Commands::Config {
            command: Some(ConfigCommand::SetEdge { to, .. }),
            ..
        }) = absent.command
        else {
            panic!("expected config set-edge");
        };
        assert_eq!(to, None);
    }

    // STORY-261 AC3: the removed row goes and nothing else moves. Asserting the
    // whole file rather than a substring is the point -- a dropped comment on a
    // neighbour, or a blank line the writer invented, is invisible to a reparse
    // and shows up here.
    #[test]
    fn remove_edge_drops_the_middle_row_and_leaves_the_rest_byte_identical() {
        let (_dir, path, fs) = fixture(&three_edged_src());

        let removed =
            run_remove_edge(path.parent().unwrap(), &fs, "stories-implement-rfcs").unwrap();

        assert_eq!(removed.name, "stories-implement-rfcs");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{SRC}{EDGE_TYPES}{EDGE_BLOCK_BEFORE}{EDGE_BLOCK_AFTER}")
        );
    }

    // A config declaring no edges is legal, so the last row can go -- and the
    // table goes with it rather than staying behind as an empty
    // array-of-tables. The TOML loses the key; `config --json` keeps the field,
    // because an agent reading `edges` should never have to branch on null.
    #[test]
    fn remove_edge_removing_the_last_row_takes_the_edges_table_with_it() {
        let (_dir, path, fs) = fixture(&edged_src());

        run_remove_edge(path.parent().unwrap(), &fs, "stories-implement-rfcs").unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            after,
            format!("{SRC}{EDGE_TYPES}"),
            "the emptied table leaves no trace of itself"
        );
        assert!(Config::parse(&after).unwrap().edges.is_empty());
        assert_eq!(show(&after)["edges"], serde_json::json!([]));
    }

    // A name that addresses no row is a CLI-argument error, not a config one,
    // and a removal that matched nothing must not rewrite the file at all.
    #[test]
    fn remove_edge_rejects_an_unknown_name_without_writing() {
        let src = three_edged_src();
        let (_dir, path, fs) = fixture(&src);

        let err = run_remove_edge(path.parent().unwrap(), &fs, "stories-implement-spikes")
            .unwrap_err()
            .to_string();

        assert!(
            err.contains("unknown edge") && err.contains("stories-implement-spikes"),
            "the refusal must name the row asked for: {err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), src);
    }

    // STORY-261 AC4: the envelope carries the row as it stood, spelled the way
    // `config --json` spelled it while it was there. Dictum 2 -- an agent cannot
    // re-read a row that is gone, so removal is the one mutation whose result
    // has to carry the whole thing.
    #[test]
    fn remove_edge_json_carries_the_row_config_show_reported() {
        let src = edged_src();
        let (_dir, path, fs) = fixture(&src);
        let removed =
            run_remove_edge(path.parent().unwrap(), &fs, "stories-implement-rfcs").unwrap();

        let envelope: Value =
            serde_json::from_str(&run_remove_edge_json(&removed).unwrap()).unwrap();

        assert_eq!(envelope["action"], "edge-removed");
        assert_eq!(envelope["name"], "stories-implement-rfcs");
        assert_eq!(envelope["edge"], show(&src)["edges"][0]);
    }

    // Two rows under one name is a config that does not load, so `remove-edge`
    // reports the collision rather than choosing which of them to drop -- the
    // parse happens before the removal, and the file is left as it was. Were
    // the guard ever to loosen, `run_remove_edge` retains by name and both
    // would go.
    #[test]
    fn remove_edge_refuses_a_config_whose_rows_share_a_name() {
        let src = format!("{SRC}{EDGE_TYPES}{EDGE_BLOCK}{EDGE_BLOCK}");
        let (_dir, path, fs) = fixture(&src);

        let err = run_remove_edge(path.parent().unwrap(), &fs, "stories-implement-rfcs")
            .unwrap_err()
            .to_string();

        assert!(
            err.contains("both named") && err.contains("stories-implement-rfcs"),
            "the loader's own collision error must come through: {err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), src);
    }

    // The writer inserting or leaving whitespace a human would not is invisible
    // to every other assertion on this story, so the round trip is asserted on
    // the bytes: a config that declared no `[[edges]]` is returned to exactly
    // what it was, table header and all.
    #[test]
    fn add_edge_then_remove_edge_returns_the_file_to_what_it_was() {
        let src = format!("{SRC}{EDGE_TYPES}");
        let (_dir, path, fs) = fixture(&src);
        let root = path.parent().unwrap();

        add_stories_implement_rfcs(root, &fs).unwrap();
        run_remove_edge(root, &fs, "stories-implement-rfcs").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), src);
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
        assert!(after.contains("# the relationship the hierarchy runs on"));
        // The new block is appended to the [[types]] array (after the last type,
        // before [[relationships]]); the relationship comment still precedes it.
        let spike_at = after.find(r#"name = "spike""#).unwrap();
        let rels_at = after.find("[[relationships]]").unwrap();
        assert!(spike_at < rels_at, "new type sits inside the types array");
        // Untouched blocks keep their order: types -> relationships.
        assert!(after.find("# document types follow").unwrap() < rels_at);
        assert!(
            after
                .find("# the relationship the hierarchy runs on")
                .unwrap()
                < rels_at
        );
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
        assert!(after.contains("# the relationship the hierarchy runs on"));
        // The other type's lifecycle is untouched.
        assert!(after
            .contains(r#"states = ["draft", "done"], edges = [{ from = "draft", to = "done" }]"#));
        let json = show(&after);
        // story keeps its original lifecycle.
        assert_eq!(type_named(&json, "story")["lifecycle"]["states"][1], "done");
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

    // ITERATION-338: choosing clickup-tasks prompts for the store's defaults right
    // after the store select; both land on the collected type.
    #[test]
    fn collect_type_clickup_captures_list_id_and_task_type() {
        let config = Config::parse(SRC).unwrap();
        let mut prompter = scripted(&[
            "task",
            "tasks",
            "",
            "",
            "",              // name/plural/dir/prefix/icon
            "clickup-tasks", // store
            "list123",       // clickup list id
            "1001",          // clickup task type
            "",
            "",
            "", // numbering/singleton/authorship
            "n",
            "n",
            "n",
            "n", // attributes / parent / lifecycle / gate declined
        ]);
        let collected = collect_type_interactive(&config, &mut prompter).unwrap();
        assert_eq!(collected.clickup_list_id.as_deref(), Some("list123"));
        assert_eq!(collected.clickup_task_type, Some(1001));
        assert_eq!(collected.github_issue_tag, None);
        assert_eq!(collected.github_issue_type, None);
    }

    // ITERATION-338: a non-numeric clickup task type is rejected and re-asked in
    // place; a blank leaves it None.
    #[test]
    fn collect_type_clickup_task_type_reasks_on_non_numeric() {
        let config = Config::parse(SRC).unwrap();
        let mut prompter = scripted(&[
            "task",
            "tasks",
            "",
            "",
            "",
            "clickup-tasks",
            "",     // clickup list id (blank -> None)
            "nope", // not a number -> re-ask
            "42",   // valid
            "",
            "",
            "",
            "n",
            "n",
            "n",
            "n",
        ]);
        let collected = collect_type_interactive(&config, &mut prompter).unwrap();
        assert_eq!(collected.clickup_list_id, None);
        assert_eq!(collected.clickup_task_type, Some(42));
    }

    // ITERATION-338: choosing github-issues prompts for tag and issue type.
    #[test]
    fn collect_type_github_captures_tag_and_issue_type() {
        let config = Config::parse(SRC).unwrap();
        let mut prompter = scripted(&[
            "issue",
            "issues",
            "",
            "",
            "",
            "github-issues", // store
            "Bug",           // github issue tag
            "Bug",           // github issue type
            "",
            "",
            "",
            "n",
            "n",
            "n",
            "n",
        ]);
        let collected = collect_type_interactive(&config, &mut prompter).unwrap();
        assert_eq!(collected.github_issue_tag.as_deref(), Some("Bug"));
        assert_eq!(collected.github_issue_type.as_deref(), Some("Bug"));
        assert_eq!(collected.clickup_list_id, None);
        assert_eq!(collected.clickup_task_type, None);
    }

    // ITERATION-338: a filesystem type prompts for NONE of the remote defaults.
    // No answers are queued for them: a spurious remote prompt would consume a
    // later answer and the run would end short, so a clean run proves they skip.
    #[test]
    fn collect_type_filesystem_prompts_for_no_remote_fields() {
        let config = Config::parse(SRC).unwrap();
        let mut prompter = scripted(&[
            "doc",
            "docs",
            "",
            "",
            "",
            "filesystem", // store
            "",
            "",
            "", // numbering/singleton/authorship
            "n",
            "n",
            "n",
            "n",
        ]);
        let collected = collect_type_interactive(&config, &mut prompter).unwrap();
        assert_eq!(collected.github_issue_tag, None);
        assert_eq!(collected.github_issue_type, None);
        assert_eq!(collected.clickup_list_id, None);
        assert_eq!(collected.clickup_task_type, None);
    }

    // ITERATION-338: the interactive add-type path persists the GitHub remote
    // fields it collects, rather than discarding them before the disk write.
    #[test]
    fn interactive_add_type_persists_github_fields() {
        // A github-issues type reparses only with a [github] section present.
        let src = format!("[github]\nrepo = \"owner/repo\"\n\n{SRC}");
        let (_dir, path, fs) = fixture(&src);
        let mut prompter = scripted(&[
            "issue",
            "issues",
            "",
            "",
            "",              // name/plural/dir/prefix/icon
            "github-issues", // store
            "Bug",           // github issue tag
            "Defect",        // github issue type
            "",
            "",
            "", // numbering/singleton/authorship
            "n",
            "n",
            "n",
            "n", // attributes / parent / lifecycle / gate declined
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let issue = Config::parse(&after)
            .unwrap()
            .type_by_name("issue")
            .unwrap()
            .clone();
        assert_eq!(issue.github_issue_tag.as_deref(), Some("Bug"));
        assert_eq!(issue.github_issue_type.as_deref(), Some("Defect"));
    }

    // ITERATION-338: the interactive add-type path persists both ClickUp remote
    // fields, including clickup_task_type (which the pre-fix path dropped).
    #[test]
    fn interactive_add_type_persists_clickup_fields() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "task",
            "tasks",
            "",
            "",
            "",              // name/plural/dir/prefix/icon
            "clickup-tasks", // store
            "list-7",        // clickup list id
            "2002",          // clickup task type
            "",
            "",
            "", // numbering/singleton/authorship
            "n",
            "n",
            "n",
            "n", // attributes / parent / lifecycle / gate declined
        ]);
        run_add_type_interactive(path.parent().unwrap(), &fs, &mut prompter).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        let task = Config::parse(&after)
            .unwrap()
            .type_by_name("task")
            .unwrap()
            .clone();
        assert_eq!(task.clickup_list_id.as_deref(), Some("list-7"));
        assert_eq!(task.clickup_task_type, Some(2002));
    }

    // STORY-226 AC2: declining the custom-lifecycle prompt leaves the lifecycle
    // empty, so the type inherits the store preset via effective_lifecycle.
    #[test]
    fn interactive_add_type_declined_lifecycle_inherits_preset() {
        let (_dir, path, fs) = fixture(SRC);
        let mut prompter = scripted(&[
            "widget", "widgets", "", "", "", "", "", "", "", // core fields
            "n", "n", "n", // attributes / parent / lifecycle declined
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
            "n",     // lifecycle declined
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

    // STORY-226 AC5: the full interactive flow produces a byte-identical config to
    // the equivalent add-type(+attrs+parent) -> set-lifecycle flag chain, and the
    // result reparses cleanly.
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
            None,
            None,
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
        let flag_out = std::fs::read_to_string(&path_b).unwrap();

        assert_eq!(interactive_out, flag_out);
        Config::parse(&interactive_out).unwrap();
    }
}
