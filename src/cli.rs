pub mod completions;
pub mod context;
pub mod convention;
pub mod create;
pub mod delete;
pub mod fetch;
pub mod fix;
pub mod ignore;
pub mod init;
pub mod json;
pub mod link;
pub mod list;
pub mod pin;
pub mod reservations;
pub mod resolve;
pub mod search;
pub mod setup;
pub mod show;
pub mod status;
pub mod style;
pub mod update;
pub mod validate;

use crate::cli::reservations::ReservationsCommand;
use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::engine::ArgValueCompleter;

#[derive(Debug, Clone, ValueEnum)]
pub enum RenumberFormat {
    Sqids,
    Incremental,
}

#[derive(Parser)]
#[command(
    name = "lazyspec",
    about = "Manage project stories, RFCs, ADRs, and iterations"
)]
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
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Expand @ref directives into fenced code blocks
        #[arg(short = 'e', long = "expand-references")]
        expand_references: bool,
        /// Maximum lines per expanded @ref block
        #[arg(long, default_value_t = 25)]
        max_ref_lines: usize,
    },
    /// Update document frontmatter
    Update {
        /// Document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
        /// Set status
        #[arg(long)]
        status: Option<String>,
        /// Set title
        #[arg(long)]
        title: Option<String>,
        /// Set body content inline
        #[arg(long)]
        body: Option<String>,
        /// Read body from file (use `-` for stdin)
        #[arg(long)]
        body_file: Option<String>,
    },
    /// Delete a document
    Delete {
        /// Document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
    },
    /// Add a relationship between documents
    Link {
        /// Source document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        from: String,
        /// Relationship type (implements, supersedes, blocks, related-to)
        #[arg(add = ArgValueCompleter::new(completions::complete_rel_type))]
        rel_type: String,
        /// Target document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        to: String,
    },
    /// Remove a relationship between documents
    Unlink {
        /// Source document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        from: String,
        /// Relationship type
        #[arg(add = ArgValueCompleter::new(completions::complete_rel_type))]
        rel_type: String,
        /// Target document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
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
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Mark a document to skip validation
    Ignore {
        /// Document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
    },
    /// Remove validation skip from a document
    Unignore {
        /// Document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
    },
    /// Fix documents with broken or incomplete frontmatter
    Fix {
        /// Document paths to fix (fixes all broken docs if none given)
        #[arg()]
        paths: Vec<String>,
        /// Show what would change without writing
        #[arg(long)]
        dry_run: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Renumber all documents to the given format
        #[arg(long)]
        renumber: Option<RenumberFormat>,
        /// Filter to a single document type (e.g. rfc, story)
        #[arg(long = "type")]
        doc_type: Option<String>,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
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
    /// Pin blob hashes onto @ref directives in a document
    Pin {
        /// Document path or shorthand ID (e.g. ITERATION-114)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Fetch all github-issues documents from the API
    Fetch {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Filter to a single document type
        #[arg(long = "type")]
        doc_type: Option<String>,
    },
    /// Set up github-issues backend (validate auth, fetch issues)
    Setup,
    /// Show convention and dictum content
    Convention {
        /// Show only the convention preamble (no dictum)
        #[arg(long)]
        preamble: bool,
        /// Filter dictum by tags (comma-separated, OR logic)
        #[arg(long)]
        tags: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Manage reservation refs
    Reservations {
        #[command(subcommand)]
        command: ReservationsCommand,
    },
}
