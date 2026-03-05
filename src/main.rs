use clap::Parser;
use lazyspec::cli::{Cli, Commands};
use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            let cwd = std::env::current_dir()?;
            lazyspec::cli::init::run(&cwd)?;
        }
        Some(Commands::Create { doc_type, title, author, json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            if json {
                let output = lazyspec::cli::create::run_json(&cwd, &config, &doc_type, &title, &author)?;
                println!("{}", output);
            } else {
                let path = lazyspec::cli::create::run(&cwd, &config, &doc_type, &title, &author)?;
                println!("{}", path.display());
            }
        }
        Some(Commands::List { doc_type, status, json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::list::run(&store, doc_type.as_deref(), status.as_deref(), json);
        }
        Some(Commands::Show { id, json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::show::run_json(&store, &id)?;
                println!("{}", output);
            } else {
                lazyspec::cli::show::run(&store, &id)?;
            }
        }
        Some(Commands::Update { path, status, title }) => {
            let cwd = std::env::current_dir()?;
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
            let cwd = std::env::current_dir()?;
            lazyspec::cli::delete::run(&cwd, &path)?;
            println!("Deleted {}", path);
        }
        Some(Commands::Link { from, rel_type, to }) => {
            let cwd = std::env::current_dir()?;
            lazyspec::cli::link::link(&cwd, &from, &rel_type, &to)?;
            println!("Linked {} --{}--> {}", from, rel_type, to);
        }
        Some(Commands::Unlink { from, rel_type, to }) => {
            let cwd = std::env::current_dir()?;
            lazyspec::cli::link::unlink(&cwd, &from, &rel_type, &to)?;
            println!("Unlinked {} --{}--> {}", from, rel_type, to);
        }
        Some(Commands::Search { query, doc_type, json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::search::run(&store, &query, doc_type.as_deref(), json);
        }
        Some(Commands::Status { json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            if json {
                println!("{}", lazyspec::cli::status::run_json(&store));
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
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            if json {
                let output = lazyspec::cli::context::run_json(&store, &id)?;
                println!("{}", output);
            } else {
                let output = lazyspec::cli::context::run_human(&store, &id)?;
                print!("{}", output);
            }
        }
        Some(Commands::Validate { json, warnings }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::validate::run_full(&store, json, warnings);
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        None => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            lazyspec::tui::run(store, &config)?;
        }
    }

    Ok(())
}
