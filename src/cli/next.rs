use crate::cli::sequencing_args::validate_scope_args;
use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::git_ref::GitCli;
use crate::engine::lease::LeaseEngine;
use crate::engine::sequencing::{
    next_ready, Bottleneck, Graph, GraphWarning, LeaseView, NextOpts, NextResult, ReadyCandidate,
    ReadyKind, Scope,
};
use crate::engine::store::Store;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;

pub struct NextArgs {
    pub scope: Option<String>,
    pub after: Option<String>,
    pub type_filter: Option<String>,
    pub include_leased: bool,
    pub json: bool,
}

#[derive(Serialize)]
struct ReadyJson {
    id: String,
    kind: &'static str,
    leased_by: Option<String>,
}

#[derive(Serialize)]
struct BottleneckJson {
    id: String,
    gates: usize,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum WarningJson {
    #[serde(rename = "cycle")]
    Cycle { ids: Vec<String> },
}

#[derive(Serialize)]
struct NextJson {
    ready: Vec<ReadyJson>,
    bottlenecks: Vec<BottleneckJson>,
    warnings: Vec<WarningJson>,
}

fn ready_kind_str(kind: ReadyKind) -> &'static str {
    match kind {
        ReadyKind::Claimable => "claimable",
        ReadyKind::NeedsChildren => "needs-children",
        ReadyKind::NeedsStatusUpdate => "needs-status-update",
    }
}

fn build_lease_view(config: &Config, store: &Store) -> LeaseView {
    let coord = match config.coordination.as_ref() {
        Some(c) => c.clone(),
        None => return LeaseView::default(),
    };
    let engine = LeaseEngine::new(GitCli, coord);
    let leases = match engine.query(store.root()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: could not query leases: {}", e);
            return LeaseView::default();
        }
    };
    let mut held: HashMap<String, String> = HashMap::new();
    for (refname, lease) in leases {
        if let Some(id) = refname.rsplit('/').next() {
            held.insert(id.to_string(), lease.agent);
        }
    }
    LeaseView { held }
}

pub fn run(store: &Store, config: &Config, args: NextArgs) -> i32 {
    let lease_view = build_lease_view(config, store);
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_lease_view(store, config, args, lease_view, &mut stdout, &mut stderr)
}

pub fn run_with_lease_view(
    store: &Store,
    config: &Config,
    args: NextArgs,
    lease_view: LeaseView,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let graph = Graph::from_store(store);

    let scope = match validate_scope_args(
        args.scope.as_deref(),
        args.after.as_deref(),
        &graph,
        stderr,
    ) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let docs: Vec<DocMeta> = store.all_docs().into_iter().cloned().collect();

    let scope_id = match &scope {
        Scope::All => None,
        Scope::Under(id) | Scope::After(id) => Some(id.clone()),
    };

    let opts = NextOpts {
        include_leased: args.include_leased,
        scope: scope_id,
    };

    let result = next_ready(&graph, &docs, &opts, &lease_view, config);

    let filtered_ready: Vec<ReadyCandidate> = match args.type_filter.as_deref() {
        Some(want) => {
            let types_by_id: HashMap<&str, &str> = docs
                .iter()
                .map(|d| (d.id.as_str(), d.doc_type.as_str()))
                .collect();
            result
                .ready
                .into_iter()
                .filter(|c| types_by_id.get(c.id.as_str()).is_some_and(|t| *t == want))
                .collect()
        }
        None => result.ready,
    };

    let final_result = NextResult {
        ready: filtered_ready,
        bottlenecks: result.bottlenecks,
        warnings: result.warnings,
    };

    if args.json {
        let payload = to_json(&final_result);
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => {
                let _ = writeln!(stdout, "{}", s);
            }
            Err(e) => {
                let _ = writeln!(stderr, "failed to serialize next result: {}", e);
                return 1;
            }
        }
    } else {
        print_human(&final_result, stdout);
    }

    0
}

fn to_json(r: &NextResult) -> NextJson {
    NextJson {
        ready: r
            .ready
            .iter()
            .map(|c| ReadyJson {
                id: c.id.clone(),
                kind: ready_kind_str(c.kind),
                leased_by: c.lessee.clone(),
            })
            .collect(),
        bottlenecks: r
            .bottlenecks
            .iter()
            .map(|b: &Bottleneck| BottleneckJson {
                id: b.id.clone(),
                gates: b.gates,
            })
            .collect(),
        warnings: r
            .warnings
            .iter()
            .map(|w| match w {
                GraphWarning::Cycle { ids } => WarningJson::Cycle { ids: ids.clone() },
            })
            .collect(),
    }
}

fn print_human(r: &NextResult, out: &mut dyn Write) {
    if r.ready.is_empty() {
        let _ = writeln!(out, "No ready work.");
    } else {
        let _ = writeln!(out, "Ready:");
        for c in &r.ready {
            let suffix = match &c.lessee {
                Some(agent) => format!(" [leased by {}]", agent),
                None => String::new(),
            };
            let _ = writeln!(out, "  {} ({}){}", c.id, ready_kind_str(c.kind), suffix);
        }
    }

    if !r.bottlenecks.is_empty() {
        let _ = writeln!(out, "\nBottlenecks:");
        for b in &r.bottlenecks {
            let _ = writeln!(out, "  {} (gates {})", b.id, b.gates);
        }
    }

    if !r.warnings.is_empty() {
        let _ = writeln!(out, "\nWarnings:");
        for w in &r.warnings {
            match w {
                GraphWarning::Cycle { ids } => {
                    let _ = writeln!(out, "  cycle: {}", ids.join(", "));
                }
            }
        }
    }
}
