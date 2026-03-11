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
            let mut updates = Vec::new();
            if let Some(ref s) = status {
                updates.push(("status", s.as_str()));
            }
            if let Some(ref t) = title {
                updates.push(("title", t.as_str()));
            }
            lazyspec::cli::update::run(&cwd, &path, &updates)?;
            println!("Updated {}", path);
        }
        Some(Commands::Delete { path }) => {
            lazyspec::cli::delete::run(&cwd, &path)?;
            println!("Deleted {}", path);
        }
        Some(Commands::Link { from, rel_type, to }) => {
            lazyspec::cli::link::link(&cwd, &from, &rel_type, &to)?;
            println!("Linked {} --{}--> {}", from, rel_type, to);
        }
        Some(Commands::Unlink { from, rel_type, to }) => {
            lazyspec::cli::link::unlink(&cwd, &from, &rel_type, &to)?;
            println!("Unlinked {} --{}--> {}", from, rel_type, to);
        }
        Some(Commands::Ignore { path }) => {
            lazyspec::cli::ignore::ignore(&cwd, &path)?;
            println!("Ignoring {}", path);
        }
        Some(Commands::Unignore { path }) => {
            lazyspec::cli::ignore::unignore(&cwd, &path)?;
            println!("Unignoring {}", path);
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
        Some(Commands::Fix { paths, dry_run, json }) => {
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::fix::run(&cwd, &store, &config, &paths, dry_run, json);
            if exit_code != 0 {
                std::process::exit(exit_code);
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
