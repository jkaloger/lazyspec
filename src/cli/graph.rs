use crate::cli::sequencing_args::validate_scope_args;
use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::sequencing::Graph;
use crate::engine::sequencing_render::{render_d2, render_dot, render_json};
use crate::engine::store::Store;
use clap::ValueEnum;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GraphFormat {
    D2,
    Json,
    Dot,
}

pub struct GraphArgs {
    pub scope: Option<String>,
    pub after: Option<String>,
    pub format: GraphFormat,
}

pub fn run(store: &Store, config: &Config, args: GraphArgs) -> i32 {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_writers(store, config, args, &mut stdout, &mut stderr)
}

pub fn run_with_writers(
    store: &Store,
    _config: &Config,
    args: GraphArgs,
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

    let rendered = match args.format {
        GraphFormat::D2 => render_d2(&graph, &scope, &docs),
        GraphFormat::Dot => render_dot(&graph, &scope, &docs),
        GraphFormat::Json => render_json(&graph, &scope, &docs),
    };

    let _ = writeln!(stdout, "{}", rendered);
    0
}
