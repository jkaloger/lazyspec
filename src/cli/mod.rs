pub mod init;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lazyspec", about = "Manage project specs, RFCs, ADRs, and plans")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize lazyspec in the current project
    Init,
}
