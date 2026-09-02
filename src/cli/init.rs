use crate::cli::config::{apply_collected_type, collect_edge, collect_type_interactive};
use crate::cli::style::{bold, dim, section_header, success_line, warning_prefix};
use crate::cli::wizard::Prompter;
use crate::engine::config::{
    starter_edges, starter_relationships, starter_types, CertificationConfig, Config,
    DocumentConfig, EdgeDef, FilesystemConfig, Naming, Templates, UiConfig,
};
use crate::engine::fs_ops::default_template;
use crate::engine::gh::{deterministic_color, GhCli, GhError, GhIssueWriter};
use crate::engine::github::resolve_repo;
use anyhow::{bail, Result};
use console::colors_enabled;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;

/// The starter config `init` writes into a fresh project. Per ADR-011 this is the
/// sole home for default types and for the starter DAG; the engine load path
/// carries none. The DAG is stated as `[[edges]]` and nothing else (STORY-259).
pub fn starter_config() -> Config {
    Config {
        documents: DocumentConfig {
            types: starter_types(),
            naming: Naming {
                pattern: "{type}-{n:03}-{title}.md".to_string(),
            },
            sqids: None,
            reserved: None,
            github: None,
        },
        filesystem: FilesystemConfig {
            templates: Templates {
                dir: ".lazyspec/templates".to_string(),
            },
        },
        relationships: starter_relationships(),
        ui: UiConfig::default(),
        edges: starter_edges(),
        ref_count_ceiling: 15,
        certification: CertificationConfig::default(),
        agents: Default::default(),
        skills: Default::default(),
        web: None,
        git_ref: Default::default(),
    }
}

/// Whether `init` should run the interactive wizard: neither opt-out flag set
/// and both stdin and stdout are TTYs. `--json` implies `--non-interactive`.
pub fn init_is_interactive(
    non_interactive: bool,
    json: bool,
    stdin_tty: bool,
    stdout_tty: bool,
) -> bool {
    !non_interactive && !json && stdin_tty && stdout_tty
}

fn ensure_no_config(root: &Path) -> Result<()> {
    if root.join(".lazyspec.toml").exists() {
        bail!(".lazyspec.toml already exists");
    }
    Ok(())
}

pub fn run(root: &Path) -> Result<()> {
    ensure_no_config(root)?;
    write_project(root, &starter_config())
}

/// Scaffold a fresh project from `config`: per-type directories, the templates
/// dir and default template, type skeletons, the serialized `.lazyspec.toml`,
/// and (for github-issues types) labels and gitignore entries. The single write
/// path shared by both the non-interactive and interactive `init` entry points.
pub fn write_project(root: &Path, config: &Config) -> Result<()> {
    let config_path = root.join(".lazyspec.toml");

    for type_def in &config.documents.types {
        fs::create_dir_all(root.join(&type_def.dir))?;
    }
    let templates_dir = root.join(&config.filesystem.templates.dir);
    fs::create_dir_all(&templates_dir)?;
    write_if_absent(&templates_dir.join("template.md"), default_template())?;

    scaffold_skeleton_files(root, config)?;

    fs::write(&config_path, config.to_toml()?)?;

    ensure_github_labels(config, root);
    ensure_gitignore(config, root)?;

    println!(
        "{}",
        success_line(&format!("Initialized lazyspec in {}", root.display()))
    );
    Ok(())
}

/// Interactive `init`: bail if a config already exists (before prompting), then
/// scaffold whichever config the chosen designer returns. The wizard defaults to a
/// blank DAG (STORY-228); passing `template == Some("starter")` pre-selects the
/// starter designer (STORY-227) and skips the first "Start from" screen entirely.
/// With no template the first screen offers `blank`/`starter`, defaulting to
/// `blank`. The single interactive dispatch; `--json`/`--non-interactive`/non-TTY
/// never reach it (they take the `starter_config()` path in `run`, which never
/// consults `template`).
pub fn run_init_interactive(
    root: &Path,
    prompter: &mut dyn Prompter,
    template: Option<&str>,
) -> Result<()> {
    ensure_no_config(root)?;
    // This path is only reached interactively (main.rs routes `--json`/non-TTY to
    // `run`), so json is false here; the guard still honours colours-off / non-TTY.
    if crate::cli::spinner::should_greet(false, std::io::stdout().is_terminal(), colors_enabled()) {
        crate::cli::spinner::say("a new project begins......");
    }
    let config = if template == Some("starter") {
        design_config_interactive(starter_config(), prompter)?
    } else {
        println!("{}", section_header("Start from"));
        let choice = prompter.select("Start from", &["blank", "starter"], "blank")?;
        if choice == "starter" {
            design_config_interactive(starter_config(), prompter)?
        } else {
            design_config_from_scratch(prompter)?
        }
    };
    write_project(root, &config)
}

/// Read `git config user.name` as the author prompt's default, or `None` when
/// git is unavailable or unconfigured. Purely cosmetic: this slice prompts for
/// an author but does not persist it (Config has no author field).
fn git_user_name() -> Option<String> {
    let output = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Walk the starter config with the user: prompt author (not persisted), naming
/// pattern, keep/drop each starter type, then an add-type loop. Pure -- no disk
/// IO -- so it is fully driveable by a `ScriptedPrompter`. Accepting every
/// default returns `base` unchanged (byte-for-byte parity with `init`). A `no`
/// at the final write confirmation discards the session and starts over.
///
/// This designer prunes the edge table and never adds to it: the starter set is
/// already five rows, and a sixth is `config add-edge`. The from-scratch
/// designer, which starts with no rows at all, is the one that prompts for them.
/// Both render the DAG summary before the write confirmation, so a prune the
/// keep/drop walk forces is shown rather than applied silently.
pub fn design_config_interactive(base: Config, prompter: &mut dyn Prompter) -> Result<Config> {
    let author_default = git_user_name();
    loop {
        let mut config = base.clone();

        // Prompted for the wizard's sake but intentionally discarded: there is
        // no author field to persist it to in this slice.
        let _author = prompter.ask("Author", author_default.as_deref())?;

        let pattern = prompter.ask("Naming pattern", Some(&base.documents.naming.pattern))?;
        config.documents.naming.pattern = pattern;

        let mut kept = Vec::new();
        for type_def in &base.documents.types {
            if prompter.confirm(&format!("Keep type {}", type_def.name), true)? {
                kept.push(type_def.clone());
            }
        }
        config.documents.types = kept;
        let pruned = drop_edges_naming_undefined_types(&mut config);

        while prompter.confirm("Add another type", false)? {
            let collected = collect_type_interactive(&config, prompter)?;
            apply_collected_type(&mut config, &collected)?;
        }

        print!("{}", render_dag_summary(&config, &pruned));
        if prompter.confirm("Write this config", true)? {
            return Ok(config);
        }
        println!("{} {}", warning_prefix(), dim("discarded; starting over"));
    }
}

/// Drop every `[[edges]]` row that names a type `config` no longer defines, and
/// return the rows that went. The keep/drop walk can retire a starter type an
/// edge row names, and such a row fails strict load outright, so the config the
/// wizard writes would not load back. A wildcard position names no type and
/// keeps its row.
///
/// Pruning beats refusing the drop, because the drop is the feature; the rows
/// come back so the summary can report them, because a DAG edit the user is not
/// shown is one they cannot correct.
fn drop_edges_naming_undefined_types(config: &mut Config) -> Vec<EdgeDef> {
    let defined = |name: &String| config.documents.types.iter().any(|t| &t.name == name);
    let (retained, pruned): (Vec<EdgeDef>, Vec<EdgeDef>) =
        config.edges.iter().cloned().partition(|edge| {
            edge.from.names().iter().all(defined) && edge.to.names().iter().all(defined)
        });
    config.edges = retained;
    pruned
}

/// The empty base the from-scratch designer builds on: the starter config's
/// non-type scaffolding (naming pattern, filesystem, ui, ceiling, certification)
/// and the type-agnostic starter relationship vocabulary, but with NO types and
/// NO edges. Edges must start empty so the from-scratch DAG never inherits a row
/// naming a type it does not define -- the dangling-edge trap, which strict load
/// rejects outright. Relationships are safe to keep because they name no types.
pub fn blank_config() -> Config {
    Config {
        documents: DocumentConfig {
            types: vec![],
            ..starter_config().documents
        },
        edges: vec![],
        ..starter_config()
    }
}

/// Design a whole type DAG from nothing, interactively: author (prompted, not
/// persisted), naming pattern, a types loop (at least one type required), then
/// -- once two or more types exist -- an edge loop. Renders a DAG summary and
/// asks to write; a `no` discards the session and starts over. Pure -- no disk
/// IO -- so it is fully driveable by a `ScriptedPrompter`. The returned `Config`
/// owes nothing to `starter_config()`'s types or edges and validates clean via
/// `write_project`.
pub fn design_config_from_scratch(prompter: &mut dyn Prompter) -> Result<Config> {
    let author_default = git_user_name();
    let base = blank_config();
    loop {
        let mut config = base.clone();

        // Prompted for the wizard's sake but intentionally discarded: there is
        // no author field to persist it to in this slice.
        let _author = prompter.ask("Author", author_default.as_deref())?;

        let pattern = prompter.ask("Naming pattern", Some(&base.documents.naming.pattern))?;
        config.documents.naming.pattern = pattern;

        // Types loop: at least one type is required. The "Add a type" default is
        // yes while no type exists yet and no afterwards, so accepting defaults
        // adds one type then stops; declining while still empty re-asks.
        loop {
            let want_type = prompter.confirm("Add a type", config.documents.types.is_empty())?;
            if want_type {
                let collected = collect_type_interactive(&config, prompter)?;
                apply_collected_type(&mut config, &collected)?;
            } else if config.documents.types.is_empty() {
                println!("{} at least one type is required", warning_prefix());
            } else {
                break;
            }
        }

        // Edges only make sense once at least two types exist, for the reason
        // the retired rules loop was gated the same way: a row's endpoints are
        // chosen from the defined types, so no row can dangle, and a lone type
        // has nothing but itself to point at. `to = "*"` would be legal with one
        // type declared; a project that wants it has `config add-edge`.
        //
        // The first row is offered by default, because a DAG with no rows walks
        // nothing: `context` on any document in such a project shows that
        // document alone.
        if config.documents.types.len() >= 2 {
            while prompter.confirm("Add an edge", config.edges.is_empty())? {
                let edge = collect_edge(&config, prompter)?;
                config.edges.push(edge);
            }
        }

        print!("{}", render_dag_summary(&config, &[]));
        if prompter.confirm("Write this config", true)? {
            return Ok(config);
        }
        println!("{} {}", warning_prefix(), dim("discarded; starting over"));
    }
}

/// The parenthesised tail of an edge line: whichever of `required` and
/// `traversal` the row states, or nothing at all when it states neither.
fn edge_qualifiers(edge: &EdgeDef) -> String {
    let mut parts = Vec::new();
    if let Some(required) = &edge.required {
        parts.push(format!("required: {}", required.as_str()));
    }
    if let Some(traversal) = &edge.traversal {
        parts.push(format!("traversal: {}", traversal.as_str()));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!(" ({})", parts.join(", "))
}

/// One `[[edges]]` row as the summary reads it: name, endpoints, the
/// relationships that realize it, and whichever qualifiers it states.
fn edge_line(edge: &EdgeDef) -> String {
    format!(
        "  {}: {} via {}{}",
        bold(&edge.name),
        dim(&format!(
            "{} -> {}",
            edge.from.spelling(),
            edge.to.spelling()
        )),
        dim(&edge.via.spelling()),
        dim(&edge_qualifiers(edge)),
    )
}

/// A human-readable summary of the designed DAG: every type with its plural,
/// directory, prefix, store, and effective lifecycle (states and transitions);
/// every `[[edges]]` row; the rows in `pruned`, which the design dropped rather
/// than declared; and the relation vocabulary. Rendered before the final write
/// confirmation.
///
/// Two unrelated things are called an edge here -- a lifecycle transition and a
/// row of the document DAG -- so the lifecycle lines read `transition:` and only
/// the DAG rows are called edges.
fn render_dag_summary(config: &Config, pruned: &[EdgeDef]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "\n{}", section_header("Types:"));
    for type_def in &config.documents.types {
        let _ = writeln!(
            out,
            "  {} (plural: {}, dir: {}, prefix: {}, store: {})",
            bold(&type_def.name),
            type_def.plural,
            dim(&type_def.dir),
            dim(&type_def.prefix),
            type_def.store,
        );
        let lifecycle = type_def.effective_lifecycle();
        let _ = writeln!(out, "    lifecycle: {}", lifecycle.states.join(", "));
        for transition in &lifecycle.edges {
            let _ = writeln!(
                out,
                "      transition: {}",
                dim(&format!("{} -> {}", transition.from, transition.to))
            );
        }
    }

    let _ = writeln!(out, "{}", section_header("DAG edges:"));
    for edge in &config.edges {
        let _ = writeln!(out, "{}", edge_line(edge));
    }
    if config.edges.is_empty() {
        out.push_str("  (none)\n");
    }

    if !pruned.is_empty() {
        let _ = writeln!(
            out,
            "{}",
            section_header("Dropped edges (they named a type you dropped):")
        );
        for edge in pruned {
            let _ = writeln!(out, "{}", edge_line(edge));
        }
    }

    let _ = writeln!(out, "{}", section_header("Relation vocabulary:"));
    for rel in &config.relationships {
        let _ = writeln!(out, "  {}", rel.name);
    }

    out
}

fn ensure_github_labels(config: &Config, root: &Path) {
    let gh_types = config.documents.github_issues_types();

    if gh_types.is_empty() {
        return;
    }

    let repo = match resolve_repo(config, root).ok() {
        Some(r) => r,
        None => {
            eprintln!("warning: could not resolve GitHub repo; skipping label creation");
            return;
        }
    };

    let client = GhCli::new();
    for type_name in &gh_types {
        let labels = match config.type_by_name(type_name) {
            Some(type_def) => type_def.github_create_labels(),
            None => continue,
        };
        let color = deterministic_color(type_name);
        let description = format!("lazyspec document type: {}", type_name);
        for label in &labels {
            match client.label_ensure(&repo, label, &description, &color) {
                Ok(()) => println!("  created label: {}", label),
                Err(e) => {
                    if let Some(gh_err) = e.downcast_ref::<GhError>() {
                        if matches!(gh_err, GhError::NotInstalled) {
                            eprintln!(
                                "warning: gh CLI not found; skipping label creation for github-issues types"
                            );
                            return;
                        }
                    }
                    eprintln!("warning: failed to create label {}: {}", label, e);
                }
            }
        }
    }
}

const GITIGNORE_ENTRIES: &[&str] = &[".lazyspec/cache/", ".lazyspec/issue-map.json"];

fn ensure_gitignore(config: &Config, root: &Path) -> Result<()> {
    if !config.documents.has_github_issues_types() {
        return Ok(());
    }

    let gitignore_path = root.join(".gitignore");
    let existing = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    let existing_lines: Vec<&str> = existing.lines().collect();
    let mut to_append: Vec<&str> = GITIGNORE_ENTRIES
        .iter()
        .filter(|entry| !existing_lines.contains(entry))
        .copied()
        .collect();

    if to_append.is_empty() {
        return Ok(());
    }

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    to_append.push(""); // trailing newline
    content.push_str(&to_append.join("\n"));

    fs::write(&gitignore_path, content)?;
    Ok(())
}

fn scaffold_skeleton_files(root: &Path, config: &Config) -> Result<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    for type_def in &config.documents.types {
        if type_def.singleton && type_def.name == "convention" {
            let conv_dir = root.join(&type_def.dir).join("convention");
            fs::create_dir_all(&conv_dir)?;
            write_if_absent(&conv_dir.join("index.md"), &convention_skeleton(&today))?;
        }

        if type_def.parent_type.as_deref() == Some("convention") && type_def.name == "dictum" {
            let parent = config
                .documents
                .types
                .iter()
                .find(|t| t.name == "convention");
            if let Some(parent) = parent {
                let conv_dir = root.join(&parent.dir).join("convention");
                write_if_absent(&conv_dir.join("example.md"), &dictum_skeleton(&today))?;
            }
        }
    }

    Ok(())
}

fn write_if_absent(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, content)?;
    Ok(())
}

fn convention_skeleton(date: &str) -> String {
    format!(
        r#"---
title: "Convention"
type: convention
status: draft
author: "unknown"
date: {date}
tags: []
---

This is your project's convention. It captures the values, constraints, and
principles that should inform all work in this repository.

Edit this document to describe your project's constitution. Keep it short.
Dictum (child documents in this folder) capture specific principles.
"#
    )
}

fn dictum_skeleton(date: &str) -> String {
    format!(
        r#"---
title: "Example Dictum"
type: dictum
status: draft
author: "unknown"
date: {date}
tags: [example]
---

This is an example dictum. Replace it with a principle that matters to your project.

Each dictum should cover a single topic and be tagged for selective retrieval
by agent skills. For example, a dictum about testing philosophy would have
`tags: [testing]`.
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::wizard::ScriptedPrompter;
    use crate::engine::config::{Severity, StoreBackend, Traversal, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::store::Store;
    use crate::engine::validation::validate_full;

    fn scripted(answers: &[&str]) -> ScriptedPrompter {
        ScriptedPrompter::new(answers.iter().map(|s| s.to_string()).collect())
    }

    fn plain_summary(config: &Config, pruned: &[EdgeDef]) -> String {
        console::strip_ansi_codes(&render_dag_summary(config, pruned)).to_string()
    }

    // Enough blank answers to accept every default in the wizard: author, naming,
    // one keep-confirm per starter type, the add-another decline, and the final
    // write confirmation.
    fn all_default_answers() -> Vec<String> {
        let n = starter_config().documents.types.len();
        vec![String::new(); n + 4]
    }

    // AC1: the wizard prompts author then naming then keep/drop, so a queued
    // non-blank naming pattern lands on the config and every starter type
    // survives a keep-all walk. (Misaligning the author prompt would divert the
    // pattern answer and fail the assertion.)
    #[test]
    fn design_prompts_author_naming_types() {
        let mut answers = vec![
            "Ada Lovelace".to_string(), // author (prompted, discarded)
            "custom-{type}-{n:03}-{title}.md".to_string(), // naming pattern
        ];
        let starter = starter_config();
        for _ in &starter.documents.types {
            answers.push("y".to_string()); // keep each starter type
        }
        answers.push("n".to_string()); // add another? no
        answers.push("y".to_string()); // write? yes

        let mut prompter = ScriptedPrompter::new(answers);
        let config = design_config_interactive(starter_config(), &mut prompter).unwrap();

        assert_eq!(
            config.documents.naming.pattern,
            "custom-{type}-{n:03}-{title}.md"
        );
        for type_def in &starter.documents.types {
            assert!(
                config.type_by_name(&type_def.name).is_some(),
                "starter type {} should survive keep-all",
                type_def.name
            );
        }
    }

    // AC2: accepting every default returns the starter config byte-for-byte, so a
    // non-interactive `init` and an accept-all interactive `init` are identical.
    #[test]
    fn design_all_defaults_equals_starter() {
        let mut prompter = ScriptedPrompter::new(all_default_answers());
        let config = design_config_interactive(starter_config(), &mut prompter).unwrap();
        assert_eq!(
            config.to_toml().unwrap(),
            starter_config().to_toml().unwrap()
        );
    }

    // AC3: the non-interactive scaffold writer emits exactly the starter config.
    #[test]
    fn init_noninteractive_writes_starter() {
        let dir = tempfile::tempdir().unwrap();
        write_project(dir.path(), &starter_config()).unwrap();
        let written = fs::read_to_string(dir.path().join(".lazyspec.toml")).unwrap();
        assert_eq!(written, starter_config().to_toml().unwrap());
    }

    // AC3: `--json` and `--non-interactive` and a non-TTY each suppress the wizard;
    // only a plain TTY invocation runs it.
    #[test]
    fn json_suppresses_interactive() {
        assert!(!init_is_interactive(false, true, true, true), "--json");
        assert!(
            !init_is_interactive(true, false, true, true),
            "--non-interactive"
        );
        assert!(
            !init_is_interactive(false, false, false, true),
            "no stdin tty"
        );
        assert!(
            !init_is_interactive(false, false, true, false),
            "no stdout tty"
        );
        assert!(init_is_interactive(false, false, true, true), "plain tty");
    }

    // AC4: an existing .lazyspec.toml bails both paths, and the interactive path
    // bails before consuming any prompt answer (empty queue never errors).
    #[test]
    fn init_bails_when_config_exists() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".lazyspec.toml"), "existing").unwrap();

        let non_interactive = run(dir.path());
        assert!(non_interactive.is_err(), "run should bail");

        let mut prompter = scripted(&[]);
        let interactive = run_init_interactive(dir.path(), &mut prompter, None);
        assert!(interactive.is_err(), "interactive run should bail");
        assert!(
            interactive
                .unwrap_err()
                .to_string()
                .contains("already exists"),
            "bail message names the conflict"
        );
    }

    // Drop one starter type and add a new one; the designed config reflects both.
    fn drop_spec_add_spike_answers() -> Vec<String> {
        [
            "",       // author
            "",       // naming (default)
            "y",      // keep rfc
            "y",      // keep story
            "y",      // keep iteration
            "y",      // keep adr
            "n",      // DROP spec
            "y",      // keep convention
            "y",      // keep dictum
            "y",      // add another type? yes
            "spike",  // name
            "spikes", // plural
            "",       // dir -> docs/spikes
            "",       // prefix -> SPIKE
            "",       // icon (none)
            "",       // store -> filesystem
            "",       // numbering -> incremental
            "",       // singleton -> false
            "",       // authorship -> default
            "n",      // add an attribute? no
            "n",      // set a parent type? no
            "n",      // design custom lifecycle? no
            "n",      // add another type? no
            "y",      // write this config? yes
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    // AC5: dropping a starter type removes it and adding a new one appends it.
    #[test]
    fn design_drop_and_add() {
        let mut prompter = ScriptedPrompter::new(drop_spec_add_spike_answers());
        let config = design_config_interactive(starter_config(), &mut prompter).unwrap();

        assert!(config.type_by_name("spec").is_none(), "dropped type absent");
        assert!(config.type_by_name("spike").is_some(), "added type present");
        let spike = config.type_by_name("spike").unwrap();
        assert_eq!(spike.dir, "docs/spikes");
        assert_eq!(spike.prefix, "SPIKE");
        assert_eq!(
            config.edges,
            starter_edges(),
            "dropping a type no edge names leaves the starter edges alone"
        );
    }

    // Dropping a type a starter edge names takes the row with it: such a row
    // names an undeclared type, which strict load refuses, so the wizard would
    // otherwise write a config it cannot read back.
    #[test]
    fn design_dropping_a_type_drops_the_edges_naming_it() {
        let mut answers = vec![String::new(), String::new()]; // author, naming
        for type_def in &starter_config().documents.types {
            let keep = if type_def.name == "story" { "n" } else { "y" };
            answers.push(keep.to_string());
        }
        answers.push("n".to_string()); // add another type? no
        answers.push("y".to_string()); // write? yes

        let mut prompter = ScriptedPrompter::new(answers);
        let config = design_config_interactive(starter_config(), &mut prompter).unwrap();

        let names: Vec<&str> = config.edges.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "adrs-need-relations",
                "implements-traversal",
                "related-to-traversal"
            ],
            "only the rows naming no dropped type survive"
        );
        assert!(
            Config::parse(&config.to_toml().unwrap()).is_ok(),
            "the config the wizard hands back must parse: the types list shrinking is what \
             breaks it"
        );

        let dir = tempfile::tempdir().unwrap();
        write_project(dir.path(), &config).unwrap();
        let loaded = Config::load(dir.path(), &RealFileSystem);
        assert!(
            loaded.is_ok(),
            "the written config must load back, got: {:?}",
            loaded.err()
        );
    }

    // AC5: scaffolding a designed config round-trips to a project that validates
    // clean, with per-type dirs, the template, and skeletons matching the design.
    #[test]
    fn write_project_scaffold_validates() {
        let mut prompter = ScriptedPrompter::new(drop_spec_add_spike_answers());
        let config = design_config_interactive(starter_config(), &mut prompter).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_project(root, &config).unwrap();

        // Per-type directories track the designed set: dropped type absent, new
        // type present.
        assert!(root.join("docs/spikes").is_dir(), "new type dir created");
        assert!(!root.join("docs/specs").exists(), "dropped type dir absent");
        assert!(root.join("docs/rfcs").is_dir());
        assert!(
            root.join(".lazyspec/templates/template.md").is_file(),
            "default template written"
        );
        assert!(
            root.join("docs/convention/convention/index.md").is_file(),
            "convention skeleton written"
        );

        let fs = RealFileSystem;
        let loaded = Config::load(root, &fs).unwrap();
        assert!(loaded.type_by_name("spike").is_some());
        assert!(loaded.type_by_name("spec").is_none());
        assert_eq!(
            loaded.edges,
            starter_edges(),
            "the starter DAG round-trips through the written config"
        );
        // Named, not counted: a count survives a row being silently swapped for
        // another. Three constraints and the two blanket walks (STORY-261 AC6).
        let names: Vec<&str> = loaded.edges.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "stories-need-rfcs",
                "iterations-need-stories",
                "adrs-need-relations",
                "implements-traversal",
                "related-to-traversal",
            ]
        );

        let store = Store::load(root, &loaded).unwrap();
        let result = validate_full(&store, &loaded);
        assert!(
            result.errors.is_empty(),
            "scaffolded project should validate clean: {:?}",
            result.errors
        );
    }

    // --- STORY-228: from-scratch (blank slate) DAG designer ---

    // Blank base parity: no types, no edges, but the type-agnostic starter
    // relationship vocabulary and the `.md`-suffixed starter naming pattern.
    #[test]
    fn scratch_blank_config_parity() {
        let blank = blank_config();
        assert!(blank.documents.types.is_empty(), "types start empty");
        assert!(
            blank.edges.is_empty(),
            "edges start empty (no dangling edges)"
        );
        assert_eq!(
            blank.relationships,
            crate::engine::config::starter_relationships()
        );
        assert_eq!(
            blank.documents.naming.pattern, "{type}-{n:03}-{title}.md",
            "naming default keeps the .md suffix"
        );
    }

    // A full from-scratch happy-path script: rfc (custom lifecycle draft ->
    // accepted) and story (inherited lifecycle). Two types open the edge loop,
    // which this script declines, so the design ends with an empty edge table.
    fn full_scratch_answers() -> Vec<String> {
        [
            "",               // author
            "",               // naming pattern (default)
            "y",              // add a type? -> rfc
            "rfc",            // name
            "rfcs",           // plural
            "",               // dir -> docs/rfcs
            "",               // prefix -> RFC
            "",               // icon (none)
            "",               // store -> filesystem
            "",               // numbering -> incremental
            "",               // singleton -> false
            "",               // authorship -> default
            "n",              // add an attribute? no
            "y",              // design a custom lifecycle? yes
            "draft,accepted", // lifecycle states (comma-separated)
            "draft:accepted", // edge
            "",               // finish edges
            "y",              // add a type? -> story
            "story",          // name
            "stories",        // plural
            "",               // dir -> docs/stories
            "",               // prefix -> STORY
            "",               // icon (none)
            "",               // store -> filesystem
            "",               // numbering -> incremental
            "",               // singleton -> false
            "",               // authorship -> default
            "n",              // add an attribute? no
            "n",              // set a parent type? no
            "n",              // design a custom lifecycle? no (inherits preset)
            "n",              // add another type? no
            "n",              // add an edge? no
            "y",              // write this config? yes
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    // AC1: a custom-lifecycle edge that names an undefined state re-prompts
    // (reusing the type collector) rather than aborting; only defined states are
    // accepted.
    #[test]
    fn scratch_lifecycle_edge_rejects_unknown_states() {
        let answers = [
            "",
            "",  // author, naming
            "y", // add a type -> alpha
            "alpha",
            "alphas",
            "",
            "",
            "",
            "",
            "",
            "",
            "",           // core fields
            "n",          // no attribute
            "y",          // custom lifecycle
            "draft,done", // lifecycle states (comma-separated)
            "draft:nope", // edge names an undefined state -> re-ask
            "draft:done", // valid
            "",           // finish edges
            "y",          // add a type -> beta
            "beta",
            "betas",
            "",
            "",
            "",
            "",
            "",
            "",
            "",  // core fields
            "n", // no attribute
            "n", // set a parent type? no
            "n", // no custom lifecycle
            "n", // add another type? no
            "n", // add an edge? no
            "y", // write
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let mut prompter = ScriptedPrompter::new(answers);
        let config = design_config_from_scratch(&mut prompter).unwrap();

        let alpha = config.type_by_name("alpha").unwrap();
        assert_eq!(alpha.lifecycle.states, vec!["draft", "done"]);
        assert_eq!(alpha.lifecycle.edges.len(), 1, "bad edge was not kept");
        assert_eq!(alpha.lifecycle.edges[0].from, "draft");
        assert_eq!(alpha.lifecycle.edges[0].to, "done");
    }

    // A from-scratch script through the end of the types loop -- alpha then
    // beta, both on inherited lifecycles -- followed by `tail`. Two types are
    // what opens the edge loop, so every edge script starts here.
    fn two_type_scratch(tail: &[&str]) -> Vec<String> {
        [
            "", "",  // author, naming
            "y", // add a type -> alpha
            "alpha", "alphas", "", "", "", "", "", "", "",  // core fields
            "n", // no attribute
            "n", // no custom lifecycle
            "y", // add a type -> beta
            "beta", "betas", "", "", "", "", "", "", "",  // core fields
            "n", // no attribute
            "n", // no parent
            "n", // no custom lifecycle
            "n", // add another type? no
        ]
        .iter()
        .chain(tail)
        .map(|s| s.to_string())
        .collect()
    }

    // STORY-261 AC7: two types open the edge loop, and the row the answers
    // describe is the row the design carries. The config it hands back parses,
    // which is what choosing both endpoints from the defined types buys: a row
    // naming an undeclared type fails strict load outright.
    #[test]
    fn scratch_one_edge_lands_as_a_row_the_config_parses() {
        let mut prompter = ScriptedPrompter::new(two_type_scratch(&[
            "y",          // add an edge? yes
            "alpha",      // from type
            "beta",       // to types
            "implements", // via relationships
            "error",      // requiredness
            "chain",      // traversal
            "n",          // add another edge? no
            "y",          // write
        ]));

        let config = design_config_from_scratch(&mut prompter).unwrap();

        assert_eq!(config.edges.len(), 1, "one row: {:?}", config.edges);
        let edge = &config.edges[0];
        assert_eq!(edge.name, "alphas-to-betas");
        assert_eq!(edge.from.spelling(), "alpha");
        assert_eq!(edge.to.spelling(), "beta");
        assert_eq!(edge.via.spelling(), "implements");
        assert_eq!(edge.required, Some(Severity::Error));
        assert_eq!(edge.traversal, Some(Traversal::Chain));

        let rendered = config.to_toml().unwrap();
        assert!(
            Config::parse(&rendered).is_ok(),
            "the designed config must parse: {rendered}"
        );
    }

    // Nothing at the wizard's own layer stops it assembling two rows under one
    // name, and such a config does not load, so the generated name steps aside
    // for the row already there.
    #[test]
    fn scratch_two_edges_between_the_same_types_get_distinct_names() {
        let mut prompter = ScriptedPrompter::new(two_type_scratch(&[
            "y",
            "alpha",
            "beta",
            "implements",
            "none",
            "chain", // the first row
            "y",
            "alpha",
            "beta",
            "related-to",
            "none",
            "none", // the second
            "n",    // add another edge? no
            "y",    // write
        ]));

        let config = design_config_from_scratch(&mut prompter).unwrap();

        let names: Vec<&str> = config.edges.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alphas-to-betas", "alphas-to-betas-2"]);
        let rendered = config.to_toml().unwrap();
        assert!(
            Config::parse(&rendered).is_ok(),
            "two rows one name would not parse: {rendered}"
        );
    }

    // Declining the loop declares nothing: the wizard writes no row the session
    // did not ask for.
    #[test]
    fn scratch_declining_the_edge_loop_leaves_no_edges() {
        let mut prompter = ScriptedPrompter::new(two_type_scratch(&["n", "y"]));

        let config = design_config_from_scratch(&mut prompter).unwrap();

        assert!(config.type_by_name("alpha").is_some());
        assert!(config.type_by_name("beta").is_some());
        assert!(config.edges.is_empty(), "no edge is declared unasked");
    }

    // AC3: the DAG summary names every type, its lifecycle transitions, its
    // edges and the relation vocabulary. A from-scratch design declares no
    // edges, so its edge section says so rather than going missing. (The write
    // confirmation is exercised by the decline test below and by every
    // happy-path script ending in `write? yes`.)
    #[test]
    fn scratch_summary_lists_dag() {
        let mut prompter = ScriptedPrompter::new(full_scratch_answers());
        let config = design_config_from_scratch(&mut prompter).unwrap();

        let summary = render_dag_summary(&config, &[]);
        assert!(summary.contains("rfc"), "type rfc: {summary}");
        assert!(summary.contains("story"), "type story: {summary}");
        assert!(
            summary.contains("transition: draft -> accepted"),
            "rfc lifecycle transition, named as a transition: {summary}"
        );
        assert!(summary.contains("DAG edges:"), "edge section: {summary}");
        assert!(
            summary.contains("DAG edges:\n  (none)"),
            "an edgeless design says so: {summary}"
        );
        assert!(summary.contains("implements"), "relation vocab: {summary}");
    }

    // STORY-259 AC4: the summary reads the edge table -- each row's name, its
    // endpoints, the relationships that realize it, and its requiredness --
    // rather than a rules table. The wildcard positions read back as `*`.
    #[test]
    fn summary_lists_edges_with_positions_and_requiredness() {
        // Styling wraps individual tokens, and whether it is on at all is a
        // process-global the colour-parity test toggles; strip it so the
        // assertion is about the line rather than the palette.
        let summary = plain_summary(&starter_config(), &[]);

        assert!(
            summary.contains(
                "stories-need-rfcs: story -> rfc via implements (required: warning, traversal: \
                 chain)"
            ),
            "chain edge row: {summary}"
        );
        assert!(
            summary.contains(
                "iterations-need-stories: iteration -> story via implements (required: error, \
                 traversal: chain)"
            ),
            "second chain edge row: {summary}"
        );
        assert!(
            summary.contains("adrs-need-relations: adr -> * via * (required: error)"),
            "wildcard positions read back as `*`: {summary}"
        );
        assert!(
            summary.contains("implements-traversal: * -> * via implements (traversal: chain)"),
            "the blanket hierarchy row, which states no requiredness: {summary}"
        );
        assert!(
            summary.contains("related-to-traversal: * -> * via related-to (traversal: related)"),
            "the blanket neighbourhood row: {summary}"
        );
        assert!(
            !summary.contains("Parent-child rules:"),
            "the rules section is gone: {summary}"
        );
        assert!(
            !summary.contains("Dropped edges"),
            "a design that pruned nothing has no dropped section: {summary}"
        );
    }

    // A prune the user is not shown is a silent DAG edit, so the rows a dropped
    // type took with it are reported beside the rows that survived.
    #[test]
    fn summary_names_the_edges_a_dropped_type_took_with_it() {
        let mut config = starter_config();
        config.documents.types.retain(|t| t.name != "story");

        let pruned = drop_edges_naming_undefined_types(&mut config);

        let summary = plain_summary(&config, &pruned);
        assert!(
            summary.contains("Dropped edges"),
            "the pruned rows are reported: {summary}"
        );
        assert!(
            summary.contains(
                "stories-need-rfcs: story -> rfc via implements (required: warning, traversal: \
                 chain)"
            ),
            "a pruned row reads the way a surviving one does: {summary}"
        );
        assert!(
            summary.contains("iterations-need-stories: iteration -> story via implements"),
            "both rows naming the dropped type are reported: {summary}"
        );
    }

    // ITERATION-331: colour parity. With colours forced off the summary carries
    // zero ANSI escapes; with them forced on it does, yet every load-bearing
    // substring survives because styling wraps whole tokens (never splits them).
    #[test]
    fn dag_summary_colour_parity() {
        let config = starter_config();

        console::set_colors_enabled(false);
        let plain = render_dag_summary(&config, &[]);
        assert!(
            !plain.contains('\u{1b}'),
            "colours-off summary must be free of ANSI: {plain:?}"
        );

        console::set_colors_enabled(true);
        let colored = render_dag_summary(&config, &[]);
        console::set_colors_enabled(false);

        assert!(
            colored.contains('\u{1b}'),
            "colours-on summary should carry ANSI"
        );
        for needle in [
            "rfc",
            "draft -> review",
            "stories-need-rfcs",
            "story -> rfc",
            "implements",
        ] {
            assert!(
                colored.contains(needle),
                "styled summary lost contiguous substring {needle:?}: {colored:?}"
            );
        }
    }

    // STORY-261 AC7: a from-scratch design carrying a prompted row scaffolds
    // into a project that strict-loads with that row in it and validates clean.
    // Strict load, not `validate`, is what refuses a dangling row, so the load
    // is the assertion the endpoint prompts exist for.
    #[test]
    fn scratch_scaffold_with_an_edge_loads_and_validates_clean() {
        let mut prompter = ScriptedPrompter::new(two_type_scratch(&[
            "y",
            "alpha",
            "beta",
            "implements",
            "warning",
            "chain",
            "n",
            "y",
        ]));
        let config = design_config_from_scratch(&mut prompter).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_project(root, &config).unwrap();

        let loaded = Config::load(root, &RealFileSystem).unwrap();
        let names: Vec<&str> = loaded.edges.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alphas-to-betas"]);
        assert_eq!(loaded.edges[0].traversal, Some(Traversal::Chain));

        let store = Store::load(root, &loaded).unwrap();
        let result = validate_full(&store, &loaded);
        assert!(
            result.errors.is_empty(),
            "a designed edge must not make a fresh project invalid: {:?}",
            result.errors
        );
    }

    // AC4: a full from-scratch design scaffolds into a temp dir that loads and
    // validates with zero errors. Its DAG is empty by construction (this script
    // declines the edge loop), so there is no row that could dangle -- which
    // strict load, not `validate`, is what would reject.
    #[test]
    fn scratch_scaffold_validates_clean() {
        let mut prompter = ScriptedPrompter::new(full_scratch_answers());
        let config = design_config_from_scratch(&mut prompter).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_project(root, &config).unwrap();

        assert!(root.join("docs/rfcs").is_dir(), "rfc dir created");
        assert!(root.join("docs/stories").is_dir(), "story dir created");
        assert!(
            root.join(".lazyspec/templates/template.md").is_file(),
            "template written"
        );

        let fs = RealFileSystem;
        let loaded = Config::load(root, &fs).unwrap();

        assert!(
            loaded.edges.is_empty(),
            "the from-scratch DAG declares no edges: {:?}",
            loaded.edges
        );

        let store = Store::load(root, &loaded).unwrap();
        let result = validate_full(&store, &loaded);
        assert!(
            result.errors.is_empty(),
            "scaffolded from-scratch project should validate clean: {:?}",
            result.errors
        );
    }

    // AC5: declining at the write confirmation on the scratch path (then aborting
    // the reloop) writes nothing -- no config file, no per-type directories.
    #[test]
    fn scratch_decline_writes_nothing() {
        let answers = [
            "blank", // first-screen select
            "", "",  // author, naming
            "y", // add a type -> solo
            "solo", "solos", "", "", "", "", "", "", "",  // core fields
            "n", // no attribute
            "n", // no custom lifecycle
            "n", // add another type? no (one type: the edge loop is not offered)
            "n", // write this config? no -> discard and reloop
                 // reloop asks for author again; the queue is empty -> Err (abort)
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut prompter = ScriptedPrompter::new(answers);
        let result = run_init_interactive(root, &mut prompter, None);

        assert!(result.is_err(), "aborting the reloop propagates an error");
        assert!(
            !root.join(".lazyspec.toml").exists(),
            "no config written on decline"
        );
        assert!(
            !root.join("docs/solos").exists(),
            "no per-type dir written on decline"
        );
    }

    // ITERATION-330: `--template starter` pre-selects the starter designer and
    // SKIPS the first-screen select. The script starts at the author prompt (no
    // queued "Start from" answer); accepting every default yields the starter
    // config, proving no first-screen answer was consumed (a consumed answer would
    // divert the author prompt and desync the keep-all walk).
    #[test]
    fn template_starter_skips_first_screen() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut prompter = ScriptedPrompter::new(all_default_answers());

        run_init_interactive(root, &mut prompter, Some("starter")).unwrap();

        let written = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert_eq!(
            written,
            starter_config().to_toml().unwrap(),
            "starter template routes straight to the starter designer"
        );
    }

    // ITERATION-330: with no template the first-screen default is `blank`, so a
    // blank first answer routes to the from-scratch designer. A minimal scratch
    // script (one type, no rule loop) then produces a config with only that type
    // and none of the starter types.
    #[test]
    fn no_template_default_routes_to_scratch() {
        let answers = [
            "", // first-screen select -> default "blank"
            "", "",  // author, naming
            "y", // add a type -> solo
            "solo", "solos", "", "", "", "", "", "", "",  // core fields
            "n", // no attribute
            "n", // no custom lifecycle
            "n", // add another type? no
            "y", // write this config? yes
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut prompter = ScriptedPrompter::new(answers);

        run_init_interactive(root, &mut prompter, None).unwrap();

        let fs = RealFileSystem;
        let loaded = Config::load(root, &fs).unwrap();
        assert!(
            loaded.type_by_name("solo").is_some(),
            "from-scratch type present"
        );
        assert!(
            loaded.type_by_name("rfc").is_none(),
            "no starter types on the from-scratch path"
        );
    }

    fn gh_issues_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![
            TypeDef::test_fixture("rfc", StoreBackend::Filesystem),
            TypeDef::test_fixture("story", StoreBackend::GithubIssues),
        ];
        config
    }

    #[test]
    fn gitignore_created_when_github_issues_type_exists() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();

        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(contents.contains(".lazyspec/cache/"));
        assert!(contents.contains(".lazyspec/issue-map.json"));
    }

    #[test]
    fn gitignore_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();

        ensure_gitignore(&config, dir.path()).unwrap();
        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            contents.matches(".lazyspec/cache/").count(),
            1,
            "cache entry duplicated"
        );
        assert_eq!(
            contents.matches(".lazyspec/issue-map.json").count(),
            1,
            "issue-map entry duplicated"
        );
    }

    #[test]
    fn gitignore_appends_to_existing() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        fs::write(&gitignore, "node_modules/\n").unwrap();

        let config = gh_issues_config();
        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(&gitignore).unwrap();
        assert!(contents.starts_with("node_modules/\n"));
        assert!(contents.contains(".lazyspec/cache/"));
        assert!(contents.contains(".lazyspec/issue-map.json"));
    }

    #[test]
    fn gitignore_skips_already_present_entries() {
        let dir = tempfile::tempdir().unwrap();
        let gitignore = dir.path().join(".gitignore");
        fs::write(&gitignore, ".lazyspec/cache/\n").unwrap();

        let config = gh_issues_config();
        ensure_gitignore(&config, dir.path()).unwrap();

        let contents = fs::read_to_string(&gitignore).unwrap();
        assert_eq!(contents.matches(".lazyspec/cache/").count(), 1);
        assert!(contents.contains(".lazyspec/issue-map.json"));
    }

    #[test]
    fn gitignore_not_created_for_filesystem_only() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();

        ensure_gitignore(&config, dir.path()).unwrap();

        assert!(!dir.path().join(".gitignore").exists());
    }
}
