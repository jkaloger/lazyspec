use clap::Parser;
use lazyspec::cli::{Cli, Commands};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            let cwd = std::env::current_dir()?;
            lazyspec::cli::init::run(&cwd)?;
        }
        None => {
            // TODO: launch TUI
            println!("TUI not implemented yet");
        }
    }

    Ok(())
}
