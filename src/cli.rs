pub mod completions;
pub mod config;
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
pub mod provenance;
pub mod reservations;
pub mod resolve;
pub mod search;
pub mod setup;
pub mod show;
pub mod skills;
pub mod status;
pub mod style;
pub mod tag;
pub mod update;
pub mod validate;
pub mod wizard;

use crate::cli::config::ConfigCommand;
use crate::cli::provenance::ProvenanceCommand;
use crate::cli::reservations::ReservationsCommand;
use crate::cli::setup::SetupCommand;
use crate::cli::skills::SkillsCommand;
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
#[command(name = "lazyspec", about = "Manage project documentation")]
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
        /// Place the new document under a parent doc, as a subdir child.
        #[arg(long)]
        parent: Option<String>,
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
        /// Open the document externally: a browser on its web URL, else the
        /// `[tui] viewer` command on its file. With --json, print the resolved
        /// target and spawn nothing.
        #[arg(long)]
        open: bool,
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
        /// Set assignee (empty string clears)
        #[arg(long)]
        assignee: Option<String>,
        /// Set body content inline
        #[arg(long)]
        body: Option<String>,
        /// Read body from file (use `-` for stdin)
        #[arg(long)]
        body_file: Option<String>,
        /// Set a custom attribute (repeatable): --attr key=value
        #[arg(long = "attr", value_name = "KEY=VALUE")]
        attr: Vec<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a document
    Delete {
        /// Document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a relationship between documents
    Link {
        /// Source document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        from: String,
        /// Relationship type: canonical (implements, supersedes, blocks, related-to) or inverse alias (implemented-by, superseded-by, blocked-by)
        #[arg(add = ArgValueCompleter::new(completions::complete_rel_type))]
        rel_type: String,
        /// Target document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        to: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a relationship between documents
    Unlink {
        /// Source document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        from: String,
        /// Relationship type: canonical or inverse alias (mirrors `link`)
        #[arg(add = ArgValueCompleter::new(completions::complete_rel_type))]
        rel_type: String,
        /// Target document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        to: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add or remove tags on a document
    Tag {
        #[command(subcommand)]
        action: TagAction,
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
    ///
    /// With an ID, shows that document's chain. With no ID, emits the
    /// whole-store context forest; pass `--anchor <type>` to re-root the forest
    /// on documents of that type, emitting each anchor plus its descendants.
    Context {
        /// Document path or shorthand ID (e.g. ITERATION-001). Omit to emit the context forest.
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: Option<String>,
        /// Re-root the forest on documents of this type (forest mode only; ignored when an ID is given)
        #[arg(long, conflicts_with = "id")]
        anchor: Option<String>,
        /// Maximum hops to follow `related-to` links when collecting related records
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Mark a document to skip validation
    Ignore {
        /// Document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove validation skip from a document
    Unignore {
        /// Document path
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        path: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
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
        /// Repair `.lazyspec.toml` instead of documents (injects missing standard relationships/rules)
        #[arg(long)]
        config: bool,
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
    /// Fetch remote documents (github-issues, git-ref, and clickup-tasks types)
    Fetch {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Filter to a single document type
        #[arg(long = "type")]
        doc_type: Option<String>,
    },
    /// Set up a store backend. Bare `setup` runs github-issues auth + fetch;
    /// `setup clickup` captures and stores a ClickUp personal API token.
    Setup {
        #[command(subcommand)]
        command: Option<SetupCommand>,
    },
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
    /// Install and manage agent skills
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
    /// Inspect and edit .lazyspec.toml
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
        /// Print the resolved configuration as JSON (the default when no subcommand is given)
        #[arg(long)]
        json: bool,
    },
    /// Serve a read-only web view of the documents (loopback only)
    #[cfg(feature = "web")]
    Serve {
        /// Port to bind on 127.0.0.1 (default 8787)
        #[arg(long)]
        port: Option<u16>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TagAction {
    /// Add tags to a document
    Add {
        /// Document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        /// Tags to add
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove tags from a document
    Remove {
        /// Document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        /// Tags to remove
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
