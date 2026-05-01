use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use lazyspec::cli::provenance::ProvenanceCommand;
use lazyspec::cli::reservations::ReservationsCommand;
use lazyspec::cli::{Cli, Commands};
use lazyspec::engine::config::{Config, StoreBackend};
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

    if matches!(cli.command, Some(Commands::Init)) {
        lazyspec::cli::init::run(&cwd)?;
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
    let config = Config::load(&cwd, &fs)?;

    match cli.command {
        Some(Commands::Init) | Some(Commands::Completions { .. }) => unreachable!(),
        Some(Commands::Fetch { json, doc_type }) => {
            let gh = GhCli::new();
            let git_ref_ops = GitCli;
            lazyspec::cli::fetch::run(
                &cwd,
                &config,
                &gh,
                &git_ref_ops,
                "origin",
                doc_type.as_deref(),
                json,
            )?;
        }
        Some(Commands::Setup) => {
            let gh = GhCli::new();
            lazyspec::cli::setup::run(&cwd, &config, &gh)?;
        }
        Some(Commands::Create {
            doc_type,
            title,
            author,
            body,
            body_file,
            json,
        }) => {
            let body_content = lazyspec::cli::resolve_body(&body, &body_file)?;
            lazyspec::cli::lease::check_lease_gate(&cwd, &config, None)?;
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::create::run_json_with_body(
                    &cwd,
                    &config,
                    &store,
                    &doc_type,
                    &title,
                    &author,
                    body_content.as_deref(),
                    |_| {},
                )?;
                println!("{}", output);
            } else {
                let path = lazyspec::cli::create::run_with_body(
                    &cwd,
                    &config,
                    &store,
                    &doc_type,
                    &title,
                    &author,
                    body_content.as_deref(),
                    |_| {},
                )?;
                println!("{}", path.display());
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
        Some(Commands::Next {
            scope,
            after,
            type_filter,
            include_leased,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::next::run(
                &store,
                &config,
                lazyspec::cli::next::NextArgs {
                    scope,
                    after,
                    type_filter,
                    include_leased,
                    json,
                },
            );
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Some(Commands::CriticalPath {
            scope,
            after,
            json,
        }) => {
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::critical_path::run(
                &store,
                &config,
                lazyspec::cli::critical_path::CriticalPathArgs {
                    scope,
                    after,
                    json,
                },
            );
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Some(Commands::Graph {
            scope,
            after,
            format,
        }) => {
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::graph::run(
                &store,
                &config,
                lazyspec::cli::graph::GraphArgs {
                    scope,
                    after,
                    format,
                },
            );
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Some(Commands::Show {
            id,
            json,
            expand_references,
            max_ref_lines,
        }) => {
            refresh_github_cache(&cwd, &config);
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::show::run_json(
                    &store,
                    &id,
                    expand_references,
                    max_ref_lines,
                    &fs,
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
            body,
            body_file,
            json,
        }) => {
            lazyspec::cli::lease::check_lease_gate(&cwd, &config, Some(&path))?;
            let body_content = lazyspec::cli::resolve_body(&body, &body_file)?;
            let store = Store::load(&cwd, &config)?;
            let mut updates = Vec::new();
            if let Some(ref s) = status {
                updates.push(("status", s.as_str()));
            }
            if let Some(ref t) = title {
                updates.push(("title", t.as_str()));
            }
            if let Some(ref b) = body_content {
                updates.push(("body", b.as_str()));
            }
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::update::run_with_config(&cwd, &store, &path, &updates, Some(&config))?;
            if json {
                let store = Store::load(&cwd, &config)?;
                let doc = lazyspec::cli::resolve::resolve_shorthand_or_path(&store, &path)?;
                let json_val = lazyspec::cli::json::doc_to_json(doc);
                println!("{}", serde_json::to_string_pretty(&json_val)?);
            } else {
                println!("Updated {}", resolved.display());
            }
        }
        Some(Commands::Delete { path }) => {
            lazyspec::cli::lease::check_lease_gate(&cwd, &config, Some(&path))?;
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::delete::run_with_config(&cwd, &store, &path, Some(&config))?;
            println!("Deleted {}", resolved.display());
        }
        Some(Commands::Link { from, rel_type, to }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::link::link_with_config(
                &cwd,
                &store,
                &from,
                &rel_type,
                &to,
                &fs,
                Some(&config),
            )?;
            let resolved_from = lazyspec::cli::resolve::resolve_to_path(&store, &from)?;
            let resolved_to = lazyspec::cli::resolve::resolve_to_path(&store, &to)?;
            println!(
                "Linked {} --{}--> {}",
                resolved_from.display(),
                rel_type,
                resolved_to.display()
            );
        }
        Some(Commands::Unlink { from, rel_type, to }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::link::unlink_with_config(
                &cwd,
                &store,
                &from,
                &rel_type,
                &to,
                &fs,
                Some(&config),
            )?;
            let resolved_from = lazyspec::cli::resolve::resolve_to_path(&store, &from)?;
            let resolved_to = lazyspec::cli::resolve::resolve_to_path(&store, &to)?;
            println!(
                "Unlinked {} --{}--> {}",
                resolved_from.display(),
                rel_type,
                resolved_to.display()
            );
        }
        Some(Commands::Ignore { path }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::ignore::ignore(&cwd, &store, &path, &fs)?;
            println!("Ignoring {}", resolved.display());
        }
        Some(Commands::Unignore { path }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::ignore::unignore(&cwd, &store, &path, &fs)?;
            println!("Unignoring {}", resolved.display());
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
                println!("{}", lazyspec::cli::status::run_json(&store, &config));
            } else {
                let output = lazyspec::cli::status::run_human(&store);
                if output.is_empty() {
                    println!("No documents found.");
                } else {
                    print!("{}", output);
                }
            }
        }
        Some(Commands::Context { id, json }) => {
            refresh_github_cache(&cwd, &config);
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::context::run_json(&store, &id)?;
                println!("{}", output);
            } else {
                let output = lazyspec::cli::context::run_human(&store, &id)?;
                print!("{}", output);
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
                lazyspec::cli::reservations::run_prune(
                    &cwd,
                    &config,
                    &store,
                    dry_run,
                    json,
                    |_| {},
                )?;
            }
        },
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
        Some(Commands::Claim {
            doc_id,
            agent_id,
            force,
            json,
        }) => {
            if let Err(e) = lazyspec::cli::lease::run_claim(
                &cwd,
                &config,
                &doc_id,
                agent_id.as_deref(),
                force,
                json,
            ) {
                if json {
                    println!("{}", serde_json::json!({"error": e.to_string()}));
                    std::process::exit(1);
                } else {
                    return Err(e);
                }
            }
        }
        Some(Commands::Release {
            doc_id,
            agent_id,
            expected_holder,
            json,
        }) => {
            if let Err(e) = lazyspec::cli::lease::run_release(
                &cwd,
                &config,
                &doc_id,
                agent_id.as_deref(),
                expected_holder.as_deref(),
                json,
            ) {
                if json {
                    println!("{}", serde_json::json!({"error": e.to_string()}));
                    std::process::exit(1);
                } else {
                    return Err(e);
                }
            }
        }
        Some(Commands::Leases { json }) => {
            if let Err(e) = lazyspec::cli::lease::run_leases(&cwd, &config, json) {
                if json {
                    println!("{}", serde_json::json!({"error": e.to_string()}));
                    std::process::exit(1);
                } else {
                    return Err(e);
                }
            }
        }
        Some(Commands::Heartbeat {
            doc_id,
            agent_id,
            json,
        }) => {
            if let Err(e) = lazyspec::cli::lease::run_heartbeat(
                &cwd,
                &config,
                &doc_id,
                agent_id.as_deref(),
                json,
            ) {
                if json {
                    println!("{}", serde_json::json!({"error": e.to_string()}));
                    std::process::exit(1);
                } else {
                    return Err(e);
                }
            }
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

    let mut map_changed = false;
    for type_def in &gh_types {
        let all_type_names: Vec<String> = config
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let result = cache.refresh_stale(
            cwd,
            type_def,
            &gh,
            &repo,
            &mut issue_map,
            ttl,
            &all_type_names,
        );
        for warning in &result.warnings {
            eprintln!("warning: {}", warning.message);
        }
        if result.refreshed > 0 {
            map_changed = true;
        }
    }

    if map_changed {
        if let Err(e) = issue_map.save(cwd) {
            eprintln!("warning: could not save issue map after refresh: {}", e);
        }
    }
}
