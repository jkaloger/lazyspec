use clap::Parser;
use lazyspec::cli::{Cli, Commands};
use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    if matches!(cli.command, Some(Commands::Init)) {
        lazyspec::cli::init::run(&cwd)?;
        return Ok(());
    }

    let config = Config::load(&cwd)?;

    match cli.command {
        Some(Commands::Init) => unreachable!(),
        Some(Commands::Create { doc_type, title, author, json }) => {
            if json {
                let output = lazyspec::cli::create::run_json(&cwd, &config, &doc_type, &title, &author)?;
                println!("{}", output);
            } else {
                let path = lazyspec::cli::create::run(&cwd, &config, &doc_type, &title, &author)?;
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
                let output = lazyspec::cli::show::run_json(&store, &id, expand_references, max_ref_lines)?;
                println!("{}", output);
            } else {
                lazyspec::cli::show::run(&store, &id, expand_references, max_ref_lines)?;
            }
        }
        Some(Commands::Update { path, status, title }) => {
            let store = Store::load(&cwd, &config)?;
            let mut updates = Vec::new();
            if let Some(ref s) = status {
                updates.push(("status", s.as_str()));
            }
            if let Some(ref t) = title {
                updates.push(("title", t.as_str()));
            }
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::update::run(&cwd, &store, &path, &updates)?;
            println!("Updated {}", resolved.display());
        }
        Some(Commands::Delete { path }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::delete::run(&cwd, &store, &path)?;
            println!("Deleted {}", resolved.display());
        }
        Some(Commands::Link { from, rel_type, to }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::link::link(&cwd, &store, &from, &rel_type, &to)?;
            let resolved_from = lazyspec::cli::resolve::resolve_to_path(&store, &from)?;
            let resolved_to = lazyspec::cli::resolve::resolve_to_path(&store, &to)?;
            println!("Linked {} --{}--> {}", resolved_from.display(), rel_type, resolved_to.display());
        }
        Some(Commands::Unlink { from, rel_type, to }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::link::unlink(&cwd, &store, &from, &rel_type, &to)?;
            let resolved_from = lazyspec::cli::resolve::resolve_to_path(&store, &from)?;
            let resolved_to = lazyspec::cli::resolve::resolve_to_path(&store, &to)?;
            println!("Unlinked {} --{}--> {}", resolved_from.display(), rel_type, resolved_to.display());
        }
        Some(Commands::Ignore { path }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::ignore::ignore(&cwd, &store, &path)?;
            println!("Ignoring {}", resolved.display());
        }
        Some(Commands::Unignore { path }) => {
            let store = Store::load(&cwd, &config)?;
            let resolved = lazyspec::cli::resolve::resolve_to_path(&store, &path)?;
            lazyspec::cli::ignore::unignore(&cwd, &store, &path)?;
            println!("Unignoring {}", resolved.display());
        }
        Some(Commands::Search { query, doc_type, json }) => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::search::run(&store, &query, doc_type.as_deref(), json);
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
            if let Some(format) = renumber {
                let exit_code = lazyspec::cli::fix::run_renumber(&cwd, &store, &config, &format, doc_type.as_deref(), dry_run, json);
                if exit_code != 0 {
                    std::process::exit(exit_code);
                }
            } else {
                let exit_code = lazyspec::cli::fix::run(&cwd, &store, &config, &paths, dry_run, json);
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
        None => {
            let store = Store::load(&cwd, &config)?;
            lazyspec::tui::run(store, &config)?;
        }
    }

    Ok(())
}
