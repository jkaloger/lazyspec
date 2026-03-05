pub mod context;
pub mod create;
pub mod status;
pub mod delete;
pub mod init;
pub mod json;
pub mod link;
pub mod list;
pub mod show;
pub mod update;
pub mod search;
pub mod validate;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lazyspec", about = "Manage project stories, RFCs, ADRs, and iterations")]
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
        /// Document type (rfc, adr, story, iteration)
        #[arg()]
        doc_type: String,
        /// Document title
        #[arg()]
        title: String,
        /// Author name
        #[arg(long, default_value = "unknown")]
        author: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List documents
    List {
        /// Filter by type (rfc, adr, story, iteration)
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
        /// Output as JSON
        #[arg(long)]
        json: bool,
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
    /// Search across all documents
    Search {
        /// Search query
        #[arg()]
        query: String,
        /// Filter by type (rfc, adr, story, iteration)
        #[arg(long, name = "type")]
        doc_type: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show full project status with all documents and validation
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show the full document chain (RFC -> Story -> Iteration)
    Context {
        /// Document path or shorthand ID (e.g. ITERATION-001)
        #[arg()]
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Validate all documents
    Validate {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Show warnings in addition to errors
        #[arg(long)]
        warnings: bool,
    },
}
