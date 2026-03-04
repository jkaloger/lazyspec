pub mod create;
pub mod delete;
pub mod init;
pub mod link;
pub mod list;
pub mod show;
pub mod update;
pub mod validate;

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
    /// Create a new document from template
    Create {
        /// Document type (rfc, adr, spec, plan)
        #[arg()]
        doc_type: String,
        /// Document title
        #[arg()]
        title: String,
        /// Author name
        #[arg(long, default_value = "unknown")]
        author: String,
    },
    /// List documents
    List {
        /// Filter by type (rfc, adr, spec, plan)
        #[arg()]
        doc_type: Option<String>,
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show a document
    Show {
        /// Document path or shorthand ID (e.g. RFC-001)
        #[arg()]
        id: String,
    },
    /// Update document frontmatter
    Update {
        /// Document path
        #[arg()]
        path: String,
        /// Set status
        #[arg(long)]
        status: Option<String>,
        /// Set title
        #[arg(long)]
        title: Option<String>,
    },
    /// Delete a document
    Delete {
        /// Document path
        #[arg()]
        path: String,
    },
    /// Add a relationship between documents
    Link {
        /// Source document path
        #[arg()]
        from: String,
        /// Relationship type (implements, supersedes, blocks, related-to)
        #[arg()]
        rel_type: String,
        /// Target document path
        #[arg()]
        to: String,
    },
    /// Remove a relationship between documents
    Unlink {
        /// Source document path
        #[arg()]
        from: String,
        /// Relationship type
        #[arg()]
        rel_type: String,
        /// Target document path
        #[arg()]
        to: String,
    },
    /// Validate all documents
    Validate {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
