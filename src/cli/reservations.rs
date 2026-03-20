use crate::engine::config::{Config, ReservedFormat};
use crate::engine::reservation::{self, Reservation};
use crate::engine::store::Store;
use crate::engine::template::shuffle_alphabet;
use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Subcommand)]
pub enum ReservationsCommand {
    /// List all reservation refs on the remote
    List {
        #[arg(long)]
        json: bool,
    },
    /// Remove reservation refs for documents that exist locally
    Prune {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
}

pub fn run_list(repo_root: &Path, config: &Config, json: bool) -> Result<()> {
    let Some(reserved_config) = config.reserved.as_ref() else {
        bail!("reserved numbering is not configured");
    };

    let reservations = reservation::list_reservations(repo_root, &reserved_config.remote)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&reservations)?);
    } else {
        for r in &reservations {
            println!("{}\t{}\t{}", r.prefix, r.number, r.ref_path);
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct PruneOutput {
    pruned: Vec<PruneEntry>,
    orphaned: Vec<PruneEntry>,
    errors: Vec<PruneError>,
}

#[derive(Serialize)]
struct PruneEntry {
    prefix: String,
    number: u32,
    ref_path: String,
}

#[derive(Serialize)]
struct PruneError {
    prefix: String,
    number: u32,
    ref_path: String,
    error: String,
}

impl From<&Reservation> for PruneEntry {
    fn from(r: &Reservation) -> Self {
        PruneEntry {
            prefix: r.prefix.clone(),
            number: r.number,
            ref_path: r.ref_path.clone(),
        }
    }
}

fn format_number(number: u32, config: &Config) -> String {
    let reserved_config = config.reserved.as_ref().unwrap();
    match reserved_config.format {
        ReservedFormat::Incremental => format!("{:03}", number),
        ReservedFormat::Sqids => {
            let sqids_config = config.sqids.as_ref().unwrap();
            let alphabet = shuffle_alphabet(&sqids_config.salt);
            let sqids = sqids::Sqids::builder()
                .alphabet(alphabet)
                .min_length(sqids_config.min_length)
                .blocklist(HashSet::new())
                .build()
                .expect("valid sqids config");
            sqids
                .encode(&[number as u64])
                .expect("sqids encode")
                .to_lowercase()
        }
    }
}

fn has_local_document(store: &Store, prefix: &str, formatted_number: &str) -> bool {
    let needle = format!("{}-{}", prefix, formatted_number);
    store.all_docs().iter().any(|doc| {
        doc.path
            .to_string_lossy()
            .contains(&needle)
    })
}

pub fn run_prune(
    repo_root: &Path,
    config: &Config,
    store: &Store,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let Some(reserved_config) = config.reserved.as_ref() else {
        bail!("reserved numbering is not configured");
    };

    let reservations = reservation::list_reservations(repo_root, &reserved_config.remote)?;

    let mut pruned = Vec::new();
    let mut orphaned = Vec::new();
    let mut errors = Vec::new();

    for r in &reservations {
        let formatted = format_number(r.number, config);
        if has_local_document(store, &r.prefix, &formatted) {
            if dry_run {
                if !json {
                    println!("would prune\t{}\t{}\t{}", r.prefix, r.number, r.ref_path);
                }
                pruned.push(PruneEntry::from(r));
            } else {
                match reservation::delete_remote_ref(repo_root, &reserved_config.remote, &r.ref_path) {
                    Ok(()) => {
                        if !json {
                            println!("pruned\t{}\t{}\t{}", r.prefix, r.number, r.ref_path);
                        }
                        pruned.push(PruneEntry::from(r));
                    }
                    Err(e) => {
                        if !json {
                            println!("error\t{}\t{}\t{}\t{}", r.prefix, r.number, r.ref_path, e);
                        }
                        errors.push(PruneError {
                            prefix: r.prefix.clone(),
                            number: r.number,
                            ref_path: r.ref_path.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
        } else {
            if !json {
                println!("orphan\t{}\t{}\t{}", r.prefix, r.number, r.ref_path);
            }
            orphaned.push(PruneEntry::from(r));
        }
    }

    if json {
        let output = PruneOutput {
            pruned,
            orphaned,
            errors,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    }

    Ok(())
}
