use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use lazyspec::cli::provenance::ProvenanceCommand;
use lazyspec::cli::reservations::ReservationsCommand;
use lazyspec::cli::setup::SetupCommand;
use lazyspec::cli::skills::SkillsCommand;
use lazyspec::cli::{Cli, Commands, TagAction};
use lazyspec::engine::clickup::ClickupHttpClient;
use lazyspec::engine::config::{Config, StoreBackend};
use lazyspec::engine::credentials::{CredentialStore, LayeredCredentialStore};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::gh::GhCli;
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::github::resolve_repo;
use lazyspec::engine::issue_cache::IssueCache;
use lazyspec::engine::issue_map::IssueMap;
use lazyspec::engine::store::Store;

fn main() -> anyhow::Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    if let Some(Commands::Init {
        non_interactive,
        json,
        template,
    }) = &cli.command
    {
        use std::io::IsTerminal;
        let interactive = lazyspec::cli::init::init_is_interactive(
            *non_interactive,
            *json,
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
        );
        if interactive {
            let mut prompter = lazyspec::cli::wizard::StdinPrompter::new();
            lazyspec::cli::init::run_init_interactive(&cwd, &mut prompter, template.as_deref())?;
        } else {
            lazyspec::cli::init::run(&cwd)?;
        }
        return Ok(());
    }

    if let Some(Commands::Completions { shell }) = &cli.command {
        let bin = "lazyspec";
        let shell_name = match shell {
            clap_complete::Shell::Bash => "bash",
            clap_complete::Shell::Zsh => "zsh",
            clap_complete::Shell::Fish => "fish",
            clap_complete::Shell::Elvish => "elvish",
            clap_complete::Shell::PowerShell => "powershell",
            _ => {
                eprintln!("Unsupported shell for dynamic completions");
                std::process::exit(1);
            }
        };
        use clap_complete::env::EnvCompleter;
        let shells: &[&dyn EnvCompleter] = &[
            &clap_complete::env::Zsh,
            &clap_complete::env::Bash,
            &clap_complete::env::Fish,
        ];
        let env_shell = shells.iter().find(|s| s.is(shell_name));
        match env_shell {
            Some(s) => {
                s.write_registration("COMPLETE", "lazyspec", bin, bin, &mut std::io::stdout())?;
            }
            None => {
                // Fallback to static generation for shells without dynamic support
                clap_complete::generate(
                    *shell,
                    &mut Cli::command(),
                    "lazyspec",
                    &mut std::io::stdout(),
                );
            }
        }
        return Ok(());
    }

    let fs = RealFileSystem;

    // `fix --config` migrates a legacy config that strict load would reject, so
    // it must dispatch before `Config::load` via the lenient read (mirroring how
    // Init/Completions are special-cased above).
    if let Some(Commands::Fix {
        config: true,
        dry_run,
        json,
        ..
    }) = &cli.command
    {
        let exit_code = lazyspec::cli::fix::run_config(&cwd, *dry_run, *json, &fs);
        std::process::exit(exit_code);
    }

    // `skills install` must work in a project with no `.lazyspec.toml` (it
    // never creates one), so it dispatches before `Config::load` like Init and
    // `fix --config`.
    if let Some(Commands::Skills {
        command: SkillsCommand::Install { runtime },
    }) = &cli.command
    {
        lazyspec::cli::skills::run_install(&cwd, *runtime)?;
        return Ok(());
    }

    // `config schema` describes the shape of any .lazyspec.toml and is a property
    // of the binary, not of a project, so it must dispatch before `Config::load`
    // (which would otherwise fail when no .lazyspec.toml exists).
    if let Some(Commands::Config {
        command: Some(lazyspec::cli::config::ConfigCommand::Schema { .. }),
        ..
    }) = &cli.command
    {
        println!("{}", lazyspec::cli::config::run_schema_json()?);
        return Ok(());
    }

    let config = Config::load(&cwd, &fs)?;

    match cli.command {
        Some(Commands::Init { .. })
        | Some(Commands::Completions { .. })
        | Some(Commands::Skills { .. }) => {
            unreachable!()
        }
        Some(Commands::Fetch { json, doc_type }) => {
            let gh = GhCli::new();
            let git_ref_ops = GitCli;
            let clickup = ClickupHttpClient::new();
            // Only touch the credential store when a clickup-tasks type is
            // actually configured, so github-only projects never trigger
            // keychain access on `fetch`.
            let clickup_token = if config
                .documents
                .types
                .iter()
                .any(|t| t.store == lazyspec::engine::config::StoreBackend::ClickupTasks)
            {
                LayeredCredentialStore::global()
                    .load_clickup_token()
                    .ok()
                    .flatten()
            } else {
                None
            };
            lazyspec::cli::fetch::run(
                &cwd,
                &config,
                &gh,
                &git_ref_ops,
                &clickup,
                clickup_token.as_ref(),
                doc_type.as_deref(),
                json,
            )?;
        }
        Some(Commands::Setup { command }) => match command {
            None => {
                let gh = GhCli::new();
                lazyspec::cli::setup::run(&cwd, &config, &gh)?;
            }
            Some(SetupCommand::Clickup { token, json }) => {
                let client = ClickupHttpClient::new();
                let store = LayeredCredentialStore::global();
                lazyspec::cli::setup::run_clickup(&client, &store, token, json)?;
            }
        },
        Some(Commands::Create {
            doc_type,
            title,
            author,
            parent,
            body,
            body_file,
            json,
        }) => {
            let body_content = lazyspec::cli::resolve_body(&body, &body_file)?;
            let store = Store::load(&cwd, &config)?;
            let pb = lazyspec::cli::spinner::op_spinner(format!("creating {}", doc_type), json);
            if json {
                let result = lazyspec::cli::create::run_json_with_body(
                    &cwd,
                    &config,
                    &store,
                    &doc_type,
                    &title,
                    &author,
                    parent.as_deref(),
                    body_content.as_deref(),
                    |p| {
                        if let Some(pb) = &pb {
                            pb.set_message(lazyspec::cli::spinner::reservation_message(&p));
                        }
                    },
                );
                match result {
                    Ok(output) => {
                        lazyspec::cli::spinner::finish_ok(pb, "created");
                        println!("{}", output);
                    }
                    Err(e) => {
                        lazyspec::cli::spinner::finish_err(pb, "create failed");
                        return Err(e);
                    }
                }
            } else {
                let result = lazyspec::cli::create::run_with_body(
                    &cwd,
                    &config,
                    &store,
                    &doc_type,
                    &title,
                    &author,
                    parent.as_deref(),
                    body_content.as_deref(),
                    |p| {
                        if let Some(pb) = &pb {
                            pb.set_message(lazyspec::cli::spinner::reservation_message(&p));
                        }
                    },
                );
                match result {
                    Ok((path, push_outcome)) => {
                        lazyspec::cli::spinner::finish_ok(pb, "created");
                        println!("{}", path.display());
                        if let Some(warning) = push_outcome.warning() {
                            eprintln!("{}", warning);
                        }
                    }
                    Err(e) => {
                        lazyspec::cli::spinner::finish_err(pb, "create failed");
                        return Err(e);
                    }
                }
            }
        }
        Some(Commands::List {
            doc_type,
            status,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::list::run(&store, doc_type.as_deref(), status.as_deref(), json);
        }
        Some(Commands::Show {
            id,
            json,
            expand_references,
            max_ref_lines,
            open,
        }) => {
            refresh_github_cache(&cwd, &config);
            let store = Store::load(&cwd, &config)?;
            if open {
                lazyspec::cli::show::run_open(&store, &id, &config, &cwd, json)?;
            } else if json {
                let gh = GhCli::new();
                let output = lazyspec::cli::show::run_json(
                    &store,
                    &id,
                    expand_references,
                    max_ref_lines,
                    &fs,
                    &config,
                    &cwd,
                    &gh,
                )?;
                println!("{}", output);
            } else {
                lazyspec::cli::show::run(&store, &id, expand_references, max_ref_lines, &fs)?;
            }
        }
        Some(Commands::Update {
            path,
            status,
            title,
            assignee,
            body,
            body_file,
            attr,
            json,
        }) => {
            let body_content = lazyspec::cli::resolve_body(&body, &body_file)?;
            let store = Store::load(&cwd, &config)?;
            let attr_pairs = lazyspec::cli::update::parse_attr_pairs(&attr)?;
            let mut updates = Vec::new();
            if let Some(ref s) = status {
                updates.push(("status", s.as_str()));
            }
            if let Some(ref t) = title {
                updates.push(("title", t.as_str()));
            }
            if let Some(ref a) = assignee {
                updates.push(("assignee", a.as_str()));
            }
            if let Some(ref b) = body_content {
                updates.push(("body", b.as_str()));
            }
            for (key, value) in &attr_pairs {
                updates.push((key.as_str(), value.as_str()));
            }
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            let push_outcome = lazyspec::cli::update::run_with_config(
                &cwd,
                &store,
                &path,
                &updates,
                Some(&config),
            )?;
            if json {
                let store = Store::load(&cwd, &config)?;
                let doc = lazyspec::cli::resolve::resolve_shorthand_or_path(&store, &path)?;
                let mut json_val = lazyspec::cli::json::doc_to_json(doc);
                lazyspec::cli::json::merge_push_outcome(&mut json_val, &push_outcome);
                println!("{}", serde_json::to_string_pretty(&json_val)?);
            } else {
                println!("Updated {}", resolved.display());
                if let Some(warning) = push_outcome.warning() {
                    eprintln!("{}", warning);
                }
            }
        }
        Some(Commands::Delete { path, json }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            let push_outcome =
                lazyspec::cli::delete::run_with_config(&cwd, &store, &path, Some(&config))?;
            if json {
                let id = lazyspec::cli::resolve::resolve_to_id(&store, &path)?;
                let mut out = serde_json::json!({
                    "action": "deleted",
                    "id": id,
                    "path": resolved.to_string_lossy(),
                });
                lazyspec::cli::json::merge_push_outcome(&mut out, &push_outcome);
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("Deleted {}", resolved.display());
                if let Some(warning) = push_outcome.warning() {
                    eprintln!("{}", warning);
                }
            }
        }
        Some(Commands::Link {
            from,
            rel_type,
            to,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            let outcome = lazyspec::cli::link::link_with_config(
                &cwd,
                &store,
                &from,
                &rel_type,
                &to,
                &fs,
                Some(&config),
            )?;
            if json {
                let mut out = serde_json::json!({
                    "action": "linked",
                    "source": outcome.source.to_string_lossy(),
                    "rel_type": outcome.rel_type.to_string(),
                    "target": outcome.target,
                });
                lazyspec::cli::json::merge_push_outcome(&mut out, &outcome.push_outcome);
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "Linked {} --{}--> {}",
                    outcome.source.display(),
                    outcome.rel_type,
                    outcome.target
                );
                if let Some(warning) = outcome.push_outcome.warning() {
                    eprintln!("{}", warning);
                }
            }
        }
        Some(Commands::Unlink {
            from,
            rel_type,
            to,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            let outcome = lazyspec::cli::link::unlink_with_config(
                &cwd,
                &store,
                &from,
                &rel_type,
                &to,
                &fs,
                Some(&config),
            )?;
            if json {
                let mut out = serde_json::json!({
                    "action": "unlinked",
                    "source": outcome.source.to_string_lossy(),
                    "rel_type": outcome.rel_type.to_string(),
                    "target": outcome.target,
                });
                lazyspec::cli::json::merge_push_outcome(&mut out, &outcome.push_outcome);
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "Unlinked {} --{}--> {}",
                    outcome.source.display(),
                    outcome.rel_type,
                    outcome.target
                );
                if let Some(warning) = outcome.push_outcome.warning() {
                    eprintln!("{}", warning);
                }
            }
        }
        Some(Commands::Tag { action }) => match action {
            TagAction::Add { id, tags, json } => {
                let store = Store::load(&cwd, &config)?;
                let push_outcome = lazyspec::cli::tag::tag_add_with_config(
                    &cwd,
                    &store,
                    &id,
                    &tags,
                    &fs,
                    Some(&config),
                )?;
                if json {
                    let store = Store::load(&cwd, &config)?;
                    let doc = lazyspec::cli::resolve::resolve_shorthand_or_path(&store, &id)?;
                    let mut json_val = lazyspec::cli::json::doc_to_json(doc);
                    lazyspec::cli::json::merge_push_outcome(&mut json_val, &push_outcome);
                    println!("{}", serde_json::to_string_pretty(&json_val)?);
                } else {
                    println!("Tagged {}", id);
                    if let Some(warning) = push_outcome.warning() {
                        eprintln!("{}", warning);
                    }
                }
            }
            TagAction::Remove { id, tags, json } => {
                let store = Store::load(&cwd, &config)?;
                let push_outcome = lazyspec::cli::tag::tag_remove_with_config(
                    &cwd,
                    &store,
                    &id,
                    &tags,
                    &fs,
                    Some(&config),
                )?;
                if json {
                    let store = Store::load(&cwd, &config)?;
                    let doc = lazyspec::cli::resolve::resolve_shorthand_or_path(&store, &id)?;
                    let mut json_val = lazyspec::cli::json::doc_to_json(doc);
                    lazyspec::cli::json::merge_push_outcome(&mut json_val, &push_outcome);
                    println!("{}", serde_json::to_string_pretty(&json_val)?);
                } else {
                    println!("Untagged {}", id);
                    if let Some(warning) = push_outcome.warning() {
                        eprintln!("{}", warning);
                    }
                }
            }
        },
        Some(Commands::Ignore { path, json }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::ignore::ignore(&cwd, &store, &path, &fs)?;
            if json {
                let id = lazyspec::cli::resolve::resolve_to_id(&store, &path)?;
                let out = serde_json::json!({
                    "action": "ignored",
                    "id": id,
                    "path": resolved.to_string_lossy(),
                    "validate_ignore": true,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("Ignoring {}", resolved.display());
            }
        }
        Some(Commands::Unignore { path, json }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::ignore::unignore(&cwd, &store, &path, &fs)?;
            if json {
                let id = lazyspec::cli::resolve::resolve_to_id(&store, &path)?;
                let out = serde_json::json!({
                    "action": "unignored",
                    "id": id,
                    "path": resolved.to_string_lossy(),
                    "validate_ignore": false,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("Unignoring {}", resolved.display());
            }
        }
        Some(Commands::Search {
            query,
            doc_type,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::search::run(&store, &query, doc_type.as_deref(), json, &fs);
        }
        Some(Commands::Status { json }) => {
            let store = Store::load(&cwd, &config)?;
            if json {
                let gh = GhCli::new();
                println!(
                    "{}",
                    lazyspec::cli::status::run_json(&store, &config, &cwd, &gh)
                );
            } else {
                let output = lazyspec::cli::status::run_human(&store);
                if output.is_empty() {
                    println!("No documents found.");
                } else {
                    print!("{}", output);
                }
            }
        }
        Some(Commands::Context {
            id,
            anchor,
            depth,
            json,
        }) => {
            refresh_github_cache(&cwd, &config);
            let store = Store::load(&cwd, &config)?;
            match id {
                Some(id) => {
                    if json {
                        let output = lazyspec::cli::context::run_json(&store, &id, depth)?;
                        println!("{}", output);
                    } else {
                        let output = lazyspec::cli::context::run_human(&store, &id, depth)?;
                        print!("{}", output);
                    }
                }
                None => {
                    if json {
                        let output =
                            lazyspec::cli::context::run_forest_json(&store, anchor.as_deref())?;
                        println!("{}", output);
                    } else {
                        let output =
                            lazyspec::cli::context::run_forest_human(&store, anchor.as_deref())?;
                        print!("{}", output);
                    }
                }
            }
        }
        Some(Commands::Convention {
            preamble,
            tags,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::convention::run_json(
                    &store,
                    &config,
                    preamble,
                    tags.as_deref(),
                    &fs,
                )?;
                println!("{}", output);
            } else {
                let output = lazyspec::cli::convention::run_human(
                    &store,
                    &config,
                    preamble,
                    tags.as_deref(),
                    &fs,
                )?;
                print!("{}", output);
            }
        }
        Some(Commands::Fix {
            paths,
            dry_run,
            json,
            renumber,
            doc_type,
            config: _,
        }) => {
            let store = Store::load(&cwd, &config)?;
            let fs = lazyspec::engine::fs::RealFileSystem;
            if let Some(format) = renumber {
                let exit_code = lazyspec::cli::fix::run_renumber(
                    &cwd,
                    &store,
                    &config,
                    &format,
                    doc_type.as_deref(),
                    dry_run,
                    json,
                    &fs,
                );
                if exit_code != 0 {
                    std::process::exit(exit_code);
                }
            } else {
                let exit_code =
                    lazyspec::cli::fix::run(&cwd, &store, &config, &paths, dry_run, json, &fs);
                if exit_code != 0 {
                    std::process::exit(exit_code);
                }
            }
        }
        Some(Commands::Validate { json, warnings }) => {
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::validate::run_full(&store, &config, json, warnings);
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Some(Commands::Pin { id, json }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::pin::run(&store, &config, &id, json)?;
        }
        Some(Commands::Reservations { command }) => match command {
            ReservationsCommand::List { json } => {
                lazyspec::cli::reservations::run_list(&cwd, &config, json)?;
            }
            ReservationsCommand::Prune { dry_run, json } => {
                let store = Store::load(&cwd, &config)?;
                let pb = lazyspec::cli::spinner::op_spinner("pruning reservations", json);
                let result = lazyspec::cli::reservations::run_prune(
                    &cwd,
                    &config,
                    &store,
                    dry_run,
                    json,
                    pb.as_ref(),
                );
                match result {
                    Ok(()) => lazyspec::cli::spinner::finish_ok(pb, "prune complete"),
                    Err(e) => {
                        lazyspec::cli::spinner::finish_err(pb, "prune failed");
                        return Err(e);
                    }
                }
            }
        },
        Some(Commands::Config { command, json }) => {
            use lazyspec::cli::config::ConfigCommand;
            match command {
                None | Some(ConfigCommand::Show { .. }) => {
                    println!("{}", lazyspec::cli::config::run_show_json(&config)?);
                }
                Some(ConfigCommand::Schema { .. }) => {
                    // Dispatched before `Config::load` above; kept for exhaustiveness.
                    println!("{}", lazyspec::cli::config::run_schema_json()?);
                }
                Some(ConfigCommand::AddType {
                    name,
                    plural,
                    dir,
                    prefix,
                    icon,
                    parent_type,
                    singleton,
                    store,
                    numbering,
                    intent,
                    authorship,
                    github_issue_tag,
                    github_issue_type,
                    clickup_list_id,
                    clickup_task_type,
                    attributes,
                }) => {
                    use lazyspec::cli::config::{classify_add_type_args, AddTypeInvocation};
                    use std::io::IsTerminal;
                    match classify_add_type_args([&name, &plural, &dir, &prefix])? {
                        AddTypeInvocation::Positional => {
                            let type_name = name.as_deref().unwrap();
                            let pb = lazyspec::cli::spinner::op_spinner(
                                format!("adding type {type_name}"),
                                json,
                            );
                            let result = lazyspec::cli::config::run_add_type(
                                &cwd,
                                &fs,
                                type_name,
                                plural.as_deref().unwrap(),
                                dir.as_deref().unwrap(),
                                prefix.as_deref().unwrap(),
                                icon.as_deref(),
                                parent_type.as_deref(),
                                singleton,
                                store.as_deref(),
                                numbering.as_deref(),
                                intent.as_deref(),
                                authorship.as_deref(),
                                github_issue_tag.as_deref(),
                                github_issue_type.as_deref(),
                                clickup_list_id.as_deref(),
                                clickup_task_type,
                                &attributes,
                            );
                            match result {
                                Ok(()) => lazyspec::cli::spinner::finish_ok(pb, "type added"),
                                Err(e) => {
                                    lazyspec::cli::spinner::finish_err(pb, "add-type failed");
                                    return Err(e);
                                }
                            }
                        }
                        AddTypeInvocation::Prompt => {
                            let interactive = !json
                                && std::io::stdin().is_terminal()
                                && std::io::stdout().is_terminal();
                            if !interactive {
                                anyhow::bail!(
                                    "config add-type requires name, plural, dir, and prefix (or run interactively on a TTY)"
                                );
                            }
                            if lazyspec::cli::spinner::should_greet(
                                json,
                                std::io::stdout().is_terminal(),
                                console::colors_enabled(),
                            ) {
                                lazyspec::cli::spinner::say("let's add a new document type");
                            }
                            let mut prompter = lazyspec::cli::wizard::StdinPrompter::new();
                            lazyspec::cli::config::run_add_type_interactive(
                                &cwd,
                                &fs,
                                &mut prompter,
                            )?;
                            // The wizard's prompts are the feedback while it runs;
                            // settle on the happy face once the write lands.
                            let pb = lazyspec::cli::spinner::op_spinner("type added", json);
                            lazyspec::cli::spinner::finish_ok(pb, "type added");
                        }
                    }
                }
                Some(ConfigCommand::SetLifecycle {
                    name,
                    states,
                    edges,
                }) => {
                    let pb = lazyspec::cli::spinner::op_spinner(
                        format!("setting lifecycle on {name}"),
                        json,
                    );
                    match lazyspec::cli::config::run_set_lifecycle(
                        &cwd, &fs, &name, &states, &edges,
                    ) {
                        Ok(()) => lazyspec::cli::spinner::finish_ok(pb, "lifecycle set"),
                        Err(e) => {
                            lazyspec::cli::spinner::finish_err(pb, "set-lifecycle failed");
                            return Err(e);
                        }
                    }
                }
                Some(ConfigCommand::AddGate { name, status }) => {
                    let pb =
                        lazyspec::cli::spinner::op_spinner(format!("gating rule {name}"), json);
                    match lazyspec::cli::config::run_add_gate(&cwd, &fs, &name, &status) {
                        Ok(()) => lazyspec::cli::spinner::finish_ok(pb, "gate added"),
                        Err(e) => {
                            lazyspec::cli::spinner::finish_err(pb, "add-gate failed");
                            return Err(e);
                        }
                    }
                }
            }
        }
        Some(Commands::Provenance { command }) => {
            let store = Store::load(&cwd, &config)?;
            let mut stdout = std::io::stdout();
            match command {
                ProvenanceCommand::Add { id, citation, json } => {
                    lazyspec::cli::provenance::run_add(
                        &cwd,
                        &store,
                        &config,
                        &id,
                        &citation,
                        json,
                        &mut stdout,
                    )?;
                }
                ProvenanceCommand::Remove { id, citation, json } => {
                    lazyspec::cli::provenance::run_remove(
                        &cwd,
                        &store,
                        &config,
                        &id,
                        &citation,
                        json,
                        &mut stdout,
                    )?;
                }
                ProvenanceCommand::List { id, json } => {
                    lazyspec::cli::provenance::run_list(&store, id.as_deref(), json, &mut stdout)?;
                }
            }
        }
        #[cfg(feature = "web")]
        Some(Commands::Serve { port }) => {
            let store = Store::load(&cwd, &config)?;
            let coords = lazyspec::engine::github_url::resolve_repo_coords(&config, &cwd);
            if coords.is_none() {
                eprintln!("lazyspec serve: repo coordinates unresolved (no origin remote or [web] override); GitHub deep-links disabled");
            }
            let issue_map = std::sync::Arc::new(IssueMap::load(&cwd).unwrap_or_default());
            let repo_name = store
                .root()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let branch = lazyspec::engine::git_status::query_git_branch(store.root());
            let state = lazyspec::web::server::AppState {
                store: lazyspec::web::server::SharedStore::new(store),
                config: std::sync::Arc::new(config),
                coords,
                issue_map,
                repo_name,
                branch,
            };
            lazyspec::web::serve(state, port)?;
        }
        None => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::tui::run(store, &config)?;
        }
    }

    Ok(())
}

/// Refreshes stale github-issues cache entries. Failures are non-fatal and print warnings to stderr.
fn refresh_github_cache(cwd: &std::path::Path, config: &Config) {
    let gh_config = match config.documents.github.as_ref() {
        Some(gh) => gh,
        None => return,
    };

    let gh_types: Vec<_> = config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GithubIssues)
        .collect();

    if gh_types.is_empty() {
        return;
    }

    let repo = match resolve_repo(config, cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "warning: could not resolve github repo, skipping refresh: {}",
                e
            );
            return;
        }
    };

    let gh = GhCli::new();
    let cache = IssueCache::new(cwd);
    let ttl = chrono::Duration::seconds(gh_config.cache_ttl as i64);

    let mut issue_map = match IssueMap::load(cwd) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("warning: could not load issue map, skipping refresh: {}", e);
            return;
        }
    };

    let all_type_rules: Vec<lazyspec::engine::issue_body::TypeMatchRule> = config
        .documents
        .types
        .iter()
        .map(lazyspec::engine::issue_body::TypeMatchRule::from)
        .collect();
    let result = match cache.refresh_stale(
        cwd,
        &gh_types,
        &gh,
        &repo,
        &mut issue_map,
        ttl,
        &all_type_rules,
        config,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("warning: could not refresh github cache: {}", e);
            return;
        }
    };
    for warning in &result.warnings {
        eprintln!("warning: {}", warning.message);
    }

    if result.refreshed > 0 {
        if let Err(e) = issue_map.save(cwd) {
            eprintln!("warning: could not save issue map after refresh: {}", e);
        }
    }
}
