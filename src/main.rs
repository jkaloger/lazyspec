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
        Some(Commands::Create { doc_type, title, author }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let path = lazyspec::cli::create::run(&cwd, &config, &doc_type, &title, &author)?;
            println!("{}", path.display());
        }
        Some(Commands::List { doc_type, status, json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::list::run(&store, doc_type.as_deref(), status.as_deref(), json);
        }
        Some(Commands::Show { id }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            lazyspec::cli::show::run(&store, &id)?;
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
        Some(Commands::Validate { json }) => {
            let cwd = std::env::current_dir()?;
            let config = Config::load(&cwd)?;
            let store = Store::load(&cwd, &config)?;
            let exit_code = lazyspec::cli::validate::run(&store, json);
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
