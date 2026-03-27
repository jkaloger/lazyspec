use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use lazyspec::cli::reservations::ReservationsCommand;
use lazyspec::cli::{Cli, Commands};
use lazyspec::engine::config::Config;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::gh::GhCli;
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
                s.write_registration("COMPLETE", "lazyspec", &bin, &bin, &mut std::io::stdout())?;
            }
            None => {
                // Fallback to static generation for shells without dynamic support
                clap_complete::generate(*shell, &mut Cli::command(), "lazyspec", &mut std::io::stdout());
            }
        }
        return Ok(());
    }

    let fs = RealFileSystem;
    let config = Config::load(&cwd, &fs)?;

    match cli.command {
        Some(Commands::Init) | Some(Commands::Completions { .. }) => unreachable!(),
        Some(Commands::Setup) => {
            let gh = GhCli::new();
            lazyspec::cli::setup::run(&cwd, &config, &gh)?;
        }
        Some(Commands::Create { doc_type, title, author, json }) => {
            if json {
                let output = lazyspec::cli::create::run_json(&cwd, &config, &doc_type, &title, &author, |_| {})?;
                println!("{}", output);
            } else {
                let path = lazyspec::cli::create::run(&cwd, &config, &doc_type, &title, &author, |_| {})?;
                println!("{}", path.display());
            }
        }
        Some(Commands::List { doc_type, status, json }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::list::run(&store, doc_type.as_deref(), status.as_deref(), json);
        }
        Some(Commands::Show { id, json, expand_references, max_ref_lines }) => {
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::show::run_json(&store, &id, expand_references, max_ref_lines, &fs)?;
                println!("{}", output);
            } else {
                lazyspec::cli::show::run(&store, &id, expand_references, max_ref_lines, &fs)?;
            }
        }
        Some(Commands::Update { path, status, title, body, body_file }) => {
            if body.is_some() && body_file.is_some() {
                anyhow::bail!("cannot use both --body and --body-file");
            }
            let body_content = if let Some(ref b) = body {
                Some(b.clone())
            } else if let Some(ref bf) = body_file {
                if bf == "-" {
                    let mut buf = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
                    Some(buf)
                } else {
                    Some(std::fs::read_to_string(bf)?)
                }
            } else {
                None
            };
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
            println!("Updated {}", resolved.display());
        }
        Some(Commands::Delete { path }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::delete::run_with_config(&cwd, &store, &path, Some(&config))?;
            println!("Deleted {}", resolved.display());
        }
        Some(Commands::Link { from, rel_type, to }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::link::link(&cwd, &store, &from, &rel_type, &to, &fs)?;
            let resolved_from = lazyspec::cli::resolve::resolve_to_path(&store, &from)?;
            let resolved_to = lazyspec::cli::resolve::resolve_to_path(&store, &to)?;
            println!("Linked {} --{}--> {}", resolved_from.display(), rel_type, resolved_to.display());
        }
        Some(Commands::Unlink { from, rel_type, to }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::link::unlink(&cwd, &store, &from, &rel_type, &to, &fs)?;
            let resolved_from = lazyspec::cli::resolve::resolve_to_path(&store, &from)?;
            let resolved_to = lazyspec::cli::resolve::resolve_to_path(&store, &to)?;
            println!("Unlinked {} --{}--> {}", resolved_from.display(), rel_type, resolved_to.display());
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
        Some(Commands::Search { query, doc_type, json }) => {
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
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::context::run_json(&store, &id)?;
                println!("{}", output);
            } else {
                let output = lazyspec::cli::context::run_human(&store, &id)?;
                print!("{}", output);
            }
        }
        Some(Commands::Fix { paths, dry_run, json, renumber, doc_type }) => {
            let store = Store::load(&cwd, &config)?;
            let fs = lazyspec::engine::fs::RealFileSystem;
            if let Some(format) = renumber {
                let exit_code = lazyspec::cli::fix::run_renumber(&cwd, &store, &config, &format, doc_type.as_deref(), dry_run, json, &fs);
                if exit_code != 0 {
                    std::process::exit(exit_code);
                }
            } else {
                let exit_code = lazyspec::cli::fix::run(&cwd, &store, &config, &paths, dry_run, json, &fs);
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
        Some(Commands::Reservations { command }) => {
            match command {
                ReservationsCommand::List { json } => {
                    lazyspec::cli::reservations::run_list(&cwd, &config, json)?;
                }
                ReservationsCommand::Prune { dry_run, json } => {
                    let store = Store::load(&cwd, &config)?;
                    lazyspec::cli::reservations::run_prune(&cwd, &config, &store, dry_run, json, |_| {})?;
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
