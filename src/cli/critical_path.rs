use crate::cli::sequencing_args::validate_scope_args;
use crate::engine::config::Config;
use crate::engine::sequencing::{Graph, Weights};
use crate::engine::store::Store;
use std::collections::HashMap;
use std::io::Write;

pub struct CriticalPathArgs {
    pub scope: Option<String>,
    pub after: Option<String>,
    pub json: bool,
}

pub fn run(store: &Store, config: &Config, args: CriticalPathArgs) -> i32 {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_writers(store, config, args, &mut stdout, &mut stderr)
}

pub fn run_with_writers(
    store: &Store,
    config: &Config,
    args: CriticalPathArgs,
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

    let weights = build_weights(store, config);

    let path: Vec<String> = graph
        .critical_path(scope, &weights)
        .into_iter()
        .map(|n| n.0)
        .collect();

    if args.json {
        match serde_json::to_string_pretty(&path) {
            Ok(s) => {
                let _ = writeln!(stdout, "{}", s);
            }
            Err(e) => {
                let _ = writeln!(stderr, "failed to serialize critical path: {}", e);
                return 1;
            }
        }
    } else {
        for id in &path {
            let _ = writeln!(stdout, "{}", id);
        }
    }

    0
}

/// Build per-node weights keyed by doc id from priority weights. Docs with
/// missing or unknown priorities fall back to the lowest configured weight,
/// so the critical-path algorithm still considers them.
pub fn build_weights(store: &Store, config: &Config) -> Weights {
    let priority_weights = config.priority_weights();
    let lowest = priority_weights
        .values()
        .min()
        .copied()
        .unwrap_or(1) as f64;

    let mut node_weights: HashMap<String, f64> = HashMap::new();
    for doc in store.all_docs() {
        let w = match doc.priority.as_deref() {
            Some(p) => priority_weights
                .get(p)
                .copied()
                .map(|v| v as f64)
                .unwrap_or(lowest),
            None => lowest,
        };
        node_weights.insert(doc.id.clone(), w);
    }
    Weights(node_weights)
}
