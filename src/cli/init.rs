use crate::cli::config::{
    apply_collected_type, collect_parent_child_rule, collect_type_interactive,
};
use crate::cli::wizard::Prompter;
use crate::engine::config::{
    default_rules, starter_relationships, starter_types, CertificationConfig, Config,
    DocumentConfig, FilesystemConfig, Naming, Templates, UiConfig, ValidationRule,
};
use crate::engine::fs_ops::default_template;
use crate::engine::gh::{deterministic_color, GhCli, GhError, GhIssueWriter};
use crate::engine::github::resolve_repo;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// The starter config `init` writes into a fresh project. Per ADR-011 this is the
/// sole home for default types and rules; the engine load path carries none.
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
        rules: default_rules(),
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

    println!("Initialized lazyspec in {}", root.display());
    Ok(())
}

/// Interactive `init`: bail if a config already exists (before prompting), then
/// offer the two authoring paths -- tweak the starter DAG (STORY-227) or design a
/// DAG from scratch (STORY-228) -- and scaffold whichever config the chosen
/// designer returns. The single interactive dispatch; `--json`/`--non-interactive`
/// /non-TTY never reach it (they take the `starter_config()` path in `run`).
pub fn run_init_interactive(root: &Path, prompter: &mut dyn Prompter) -> Result<()> {
    ensure_no_config(root)?;
    let choice = prompter.select("Start from", &["starter", "scratch"], "starter")?;
    let config = if choice == "scratch" {
        design_config_from_scratch(prompter)?
    } else {
        design_config_interactive(starter_config(), prompter)?
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

        while prompter.confirm("Add another type", false)? {
            let collected = collect_type_interactive(&config, prompter)?;
            apply_collected_type(&mut config, &collected)?;
        }

        if prompter.confirm("Write this config", true)? {
            return Ok(config);
        }
        println!("discarded; starting over");
    }
}

/// The empty base the from-scratch designer builds on: the starter config's
/// non-type scaffolding (naming pattern, filesystem, ui, ceiling, certification)
/// and the type-agnostic starter relationship vocabulary, but with NO types and
/// NO rules. Rules must start empty so the from-scratch DAG never inherits a rule
/// referencing a type it does not define (the dangling-rule trap); every rule is
/// built from the user's own parent-child steps, whose endpoints are types they
/// just defined. Relationships are safe to keep because they name no types.
pub fn blank_config() -> Config {
    Config {
        documents: DocumentConfig {
            types: vec![],
            ..starter_config().documents
        },
        rules: vec![],
        ..starter_config()
    }
}

/// Design a whole type DAG from nothing, interactively: author (prompted, not
/// persisted), naming pattern, a types loop (at least one type required), then --
/// once two or more types exist -- a parent-child rules loop with severity and an
/// optional parent-status gate. Renders a DAG summary and asks to write; a `no`
/// discards the session and starts over. Pure -- no disk IO -- so it is fully
/// driveable by a `ScriptedPrompter`. The returned `Config` owes nothing to
/// `starter_config()`'s types or rules and validates clean via `write_project`.
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
                println!("at least one type is required");
            } else {
                break;
            }
        }

        // Parent-child rules only make sense once at least two types exist. Each
        // rule's endpoints are chosen from the defined types, so no rule can
        // dangle. Gates attach here (after rules exist), not in the types loop.
        if config.documents.types.len() >= 2 {
            while prompter.confirm("Add a parent-child rule", false)? {
                let rule = collect_parent_child_rule(&config, prompter)?;
                config.rules.push(rule);
            }
        }

        print!("{}", render_dag_summary(&config));
        if prompter.confirm("Write this config", true)? {
            return Ok(config);
        }
        println!("discarded; starting over");
    }
}

/// A human-readable summary of the designed DAG: every type with its plural,
/// directory, prefix, store, and effective lifecycle (states and edges); every
/// parent-child rule with its child, parent, severity, and gate status; and the
/// relation vocabulary. Rendered before the final write confirmation.
fn render_dag_summary(config: &Config) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    out.push_str("\nTypes:\n");
    for type_def in &config.documents.types {
        let _ = writeln!(
            out,
            "  {} (plural: {}, dir: {}, prefix: {}, store: {})",
            type_def.name, type_def.plural, type_def.dir, type_def.prefix, type_def.store,
        );
        let lifecycle = type_def.effective_lifecycle();
        let _ = writeln!(out, "    lifecycle: {}", lifecycle.states.join(", "));
        for edge in &lifecycle.edges {
            let _ = writeln!(out, "      edge: {} -> {}", edge.from, edge.to);
        }
    }

    out.push_str("Parent-child rules:\n");
    let mut any_rule = false;
    for rule in &config.rules {
        if let ValidationRule::ParentChild {
            name,
            child,
            parent,
            severity,
            require_parent_status,
        } = rule
        {
            any_rule = true;
            let gate = match require_parent_status {
                Some(status) => format!(", gate: parent status = {status}"),
                None => String::new(),
            };
            let severity = match severity {
                crate::engine::config::Severity::Error => "error",
                crate::engine::config::Severity::Warning => "warning",
            };
            let _ = writeln!(
                out,
                "  {name}: {child} -> {parent} (severity: {severity}{gate})",
            );
        }
    }
    if !any_rule {
        out.push_str("  (none)\n");
    }

    out.push_str("Relation vocabulary:\n");
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
        let label = match config.type_by_name(type_name) {
            Some(type_def) => type_def.github_label(),
            None => continue,
        };
        let color = deterministic_color(type_name);
        let description = format!("lazyspec document type: {}", type_name);
        match client.label_ensure(&repo, &label, &description, &color) {
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
    use crate::engine::config::{Severity, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::store::Store;
    use crate::engine::validation::validate_full;

    fn scripted(answers: &[&str]) -> ScriptedPrompter {
        ScriptedPrompter::new(answers.iter().map(|s| s.to_string()).collect())
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
        let interactive = run_init_interactive(dir.path(), &mut prompter);
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
            "n",      // gate a parent-child rule? no
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

        let store = Store::load(root, &loaded).unwrap();
        let result = validate_full(&store, &loaded);
        assert!(
            result.errors.is_empty(),
            "scaffolded project should validate clean: {:?}",
            result.errors
        );
    }

    // --- STORY-228: from-scratch (blank slate) DAG designer ---

    // Blank base parity: no types, no rules, but the type-agnostic starter
    // relationship vocabulary and the `.md`-suffixed starter naming pattern.
    #[test]
    fn scratch_blank_config_parity() {
        let blank = blank_config();
        assert!(blank.documents.types.is_empty(), "types start empty");
        assert!(
            blank.rules.is_empty(),
            "rules start empty (no dangling rules)"
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
    // accepted), story (inherited lifecycle, parent rfc), and a story -> rfc
    // parent-child rule with severity=error gated on parent status `accepted`.
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
            "draft",          // state
            "accepted",       // state
            "",               // finish states
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
            "n",              // set a parent type? no (the DAG edge is the rule below)
            "n",              // design a custom lifecycle? no (inherits preset)
            "n",              // add another type? no
            "y",              // add a parent-child rule? yes
            "story",          // child
            "rfc",            // parent
            "error",          // severity
            "y",              // gate on a parent status? yes
            "accepted",       // required parent status
            "n",              // add another rule? no
            "y",              // write this config? yes
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    // AC1: a custom-lifecycle edge and a rule gate that each name an undefined
    // state re-prompt (reusing the type collector) rather than aborting; only
    // defined states/statuses are accepted.
    #[test]
    fn scratch_lifecycle_and_gate_reject_unknown_states() {
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
            "",  // core fields
            "n", // no attribute
            "y", // custom lifecycle
            "draft",
            "done",
            "",           // states
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
            "n", // set a parent type? no (the DAG edge is the rule below)
            "n", // no custom lifecycle
            "n", // add another type? no
            "y", // add a parent-child rule
            "beta",
            "alpha", // child, parent
            "",      // severity -> warning
            "y",     // gate on a parent status
            "bogus", // not in alpha's lifecycle -> re-ask
            "done",  // valid
            "n",     // add another rule? no
            "y",     // write
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

        let rule = config
            .rules
            .iter()
            .find_map(|r| match r {
                ValidationRule::ParentChild {
                    require_parent_status,
                    ..
                } => require_parent_status.clone(),
                _ => None,
            })
            .expect("a gated parent-child rule exists");
        assert_eq!(rule, "done", "only a defined parent status is accepted");
    }

    // AC2: a parent-child rule draws child and parent from the defined types
    // (an unknown child re-asks) and records the chosen severity.
    #[test]
    fn scratch_parent_child_rule_defined_types_and_severity() {
        let answers = [
            "", "",  // author, naming
            "y", // add a type -> alpha
            "alpha", "alphas", "", "", "", "", "", "", "",  // core fields
            "n", // no attribute
            "n", // no custom lifecycle
            "y", // add a type -> beta
            "beta", "betas", "", "", "", "", "", "", "",      // core fields
            "n",     // no attribute
            "n",     // no parent
            "n",     // no custom lifecycle
            "n",     // add another type? no
            "y",     // add a parent-child rule
            "ghost", // not a defined type -> re-ask child
            "beta",  // child
            "alpha", // parent
            "error", // severity
            "n",     // no gate
            "n",     // add another rule? no
            "y",     // write
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let mut prompter = ScriptedPrompter::new(answers);
        let config = design_config_from_scratch(&mut prompter).unwrap();

        let rule = config
            .rules
            .iter()
            .find_map(|r| match r {
                ValidationRule::ParentChild {
                    child,
                    parent,
                    severity,
                    ..
                } => Some((child.clone(), parent.clone(), severity.clone())),
                _ => None,
            })
            .expect("a parent-child rule exists");
        assert_eq!(rule.0, "beta", "child from defined types");
        assert_eq!(rule.1, "alpha", "parent from defined types");
        assert_eq!(rule.2, Severity::Error, "chosen severity recorded");
    }

    // AC3: the DAG summary names every type, its lifecycle, every rule and gate,
    // and the relation vocabulary. (The write confirmation is exercised by the
    // decline test below and by every happy-path script ending in `write? yes`.)
    #[test]
    fn scratch_summary_lists_dag() {
        let mut prompter = ScriptedPrompter::new(full_scratch_answers());
        let config = design_config_from_scratch(&mut prompter).unwrap();

        let summary = render_dag_summary(&config);
        assert!(summary.contains("rfc"), "type rfc: {summary}");
        assert!(summary.contains("story"), "type story: {summary}");
        assert!(
            summary.contains("draft -> accepted"),
            "rfc lifecycle edge: {summary}"
        );
        assert!(
            summary.contains("stories-need-rfcs"),
            "rule name: {summary}"
        );
        assert!(
            summary.contains("story -> rfc"),
            "rule endpoints: {summary}"
        );
        assert!(
            summary.contains("parent status = accepted"),
            "gate: {summary}"
        );
        assert!(summary.contains("implements"), "relation vocab: {summary}");
    }

    // AC4: a full from-scratch design scaffolds into a temp dir that loads and
    // validates with zero errors -- including no dangling rule (every rule
    // endpoint is a defined type).
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

        // Explicit no-dangling-rule check: every rule endpoint is a defined type.
        let defined: Vec<&str> = loaded
            .documents
            .types
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        for rule in &loaded.rules {
            if let ValidationRule::ParentChild { child, parent, .. } = rule {
                assert!(defined.contains(&child.as_str()), "child {child} defined");
                assert!(
                    defined.contains(&parent.as_str()),
                    "parent {parent} defined"
                );
            }
        }

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
            "scratch", // first-screen select
            "", "",  // author, naming
            "y", // add a type -> solo
            "solo", "solos", "", "", "", "", "", "", "",  // core fields
            "n", // no attribute
            "n", // no custom lifecycle
            "n", // add another type? no (one type is enough; no rule loop)
            "n", // write this config? no -> discard and reloop
                 // reloop asks for author again; the queue is empty -> Err (abort)
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut prompter = ScriptedPrompter::new(answers);
        let result = run_init_interactive(root, &mut prompter);

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
