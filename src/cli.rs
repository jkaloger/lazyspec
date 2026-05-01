pub mod completions;
pub mod context;
pub mod convention;
pub mod create;
pub mod critical_path;
pub mod delete;
pub mod fetch;
pub mod fix;
pub mod graph;
pub mod ignore;
pub mod init;
pub mod json;
pub mod lease;
pub mod link;
pub mod list;
pub mod next;
pub mod pin;
pub mod provenance;
pub mod reservations;
pub mod resolve;
pub mod search;
pub mod sequencing_args;
pub mod setup;
pub mod show;
pub mod status;
pub mod style;
pub mod update;
pub mod validate;

use crate::cli::provenance::ProvenanceCommand;
use crate::cli::reservations::ReservationsCommand;
use clap::{Parser, Subcommand, ValueEnum};

pub fn resolve_body(
    body: &Option<String>,
    body_file: &Option<String>,
) -> anyhow::Result<Option<String>> {
    if body.is_some() && body_file.is_some() {
        anyhow::bail!("cannot use both --body and --body-file");
    }
    if let Some(b) = body {
        Ok(Some(b.clone()))
    } else if let Some(bf) = body_file {
        if bf == "-" {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
            Ok(Some(buf))
        } else {
            Ok(Some(std::fs::read_to_string(bf)?))
        }
    } else {
        Ok(None)
    }
}
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
        /// Set body content inline
        #[arg(long)]
        body: Option<String>,
        /// Read body from file (use `-` for stdin)
        #[arg(long)]
        body_file: Option<String>,
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
    /// Show the next ready work items based on the dependency graph
    Next {
        /// Restrict the ready set to a scope anchor (RFC or Story id). Mutually exclusive with --after.
        #[arg(long, add = ArgValueCompleter::new(completions::complete_doc_id))]
        scope: Option<String>,
        /// Restrict the ready set to documents downstream of an anchor (transitive blocks). Mutually exclusive with --scope.
        #[arg(long, add = ArgValueCompleter::new(completions::complete_doc_id))]
        after: Option<String>,
        /// Filter ready[] by document type (e.g. story, iteration, rfc)
        #[arg(long = "type", name = "type")]
        type_filter: Option<String>,
        /// Include candidates that are currently leased (default: hide them)
        #[arg(long)]
        include_leased: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show the longest weighted path through the dependency graph
    #[command(name = "critical-path")]
    CriticalPath {
        /// Restrict the path search to the implements-subtree of an anchor (RFC or Story id). Mutually exclusive with --after.
        #[arg(long, add = ArgValueCompleter::new(completions::complete_doc_id))]
        scope: Option<String>,
        /// Restrict the path search to documents downstream of an anchor (transitive blocks). Mutually exclusive with --scope.
        #[arg(long, add = ArgValueCompleter::new(completions::complete_doc_id))]
        after: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Render the document dependency graph as d2, dot, or JSON
    Graph {
        /// Restrict the graph to the implements-subtree of an anchor (RFC or Story id). Mutually exclusive with --after.
        #[arg(long, add = ArgValueCompleter::new(completions::complete_doc_id))]
        scope: Option<String>,
        /// Restrict the graph to documents downstream of an anchor (transitive blocks). Mutually exclusive with --scope.
        #[arg(long, add = ArgValueCompleter::new(completions::complete_doc_id))]
        after: Option<String>,
        /// Output format
        #[arg(long, value_enum, default_value_t = graph::GraphFormat::D2)]
        format: graph::GraphFormat,
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
        /// Document path or shorthand ID (e.g. RFC-001)
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
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a document
    Delete {
        /// Document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
    },
    /// Add a relationship between documents
    Link {
        /// Source document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        from: String,
        /// Relationship type (implements, supersedes, blocks, related-to)
        #[arg(add = ArgValueCompleter::new(completions::complete_rel_type))]
        rel_type: String,
        /// Target document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        to: String,
    },
    /// Remove a relationship between documents
    Unlink {
        /// Source document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        from: String,
        /// Relationship type
        #[arg(add = ArgValueCompleter::new(completions::complete_rel_type))]
        rel_type: String,
        /// Target document path or shorthand ID (e.g. RFC-001)
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
    /// Fetch remote documents (github-issues and git-ref types)
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
    /// Manage document provenance citations
    Provenance {
        #[command(subcommand)]
        command: ProvenanceCommand,
    },
    /// Acquire a lease on a document
    Claim {
        /// Document ID (e.g. STORY-108, RFC-035)
        #[arg()]
        doc_id: String,
        /// Agent identity (defaults to auto-resolved agent ID)
        #[arg(long)]
        agent_id: Option<String>,
        /// Force-acquire an expired lease held by another agent
        #[arg(long)]
        force: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Release a lease on a document
    Release {
        /// Document ID (e.g. STORY-108, RFC-035)
        #[arg()]
        doc_id: String,
        /// Agent identity (defaults to auto-resolved agent ID)
        #[arg(long)]
        agent_id: Option<String>,
        /// Admin release: verify the current holder matches this ID
        #[arg(long)]
        expected_holder: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List all active leases
    Leases {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Extend the expiry of a held lease
    Heartbeat {
        /// Document ID (e.g. STORY-108, RFC-035)
        #[arg()]
        doc_id: String,
        /// Agent identity (defaults to auto-resolved agent ID)
        #[arg(long)]
        agent_id: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
