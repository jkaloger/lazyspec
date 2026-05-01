use crate::engine::sequencing::{Graph, Scope};
use std::io::Write;

/// Validate `--scope` / `--after` flags shared by `next`, `graph`, and
/// `critical-path`. On error writes the same stderr messages that
/// `cli::next` shipped first, and returns the exit code (2) the caller
/// should propagate.
pub fn validate_scope_args(
    scope: Option<&str>,
    after: Option<&str>,
    graph: &Graph,
    stderr: &mut dyn Write,
) -> Result<Scope, i32> {
    if scope.is_some() && after.is_some() {
        let _ = writeln!(stderr, "--scope and --after are mutually exclusive");
        return Err(2);
    }

    if let Some(id) = scope {
        if graph.is_iteration(id) {
            let _ = writeln!(
                stderr,
                "--scope rejects iteration id '{}'; --scope only accepts RFC or Story ids",
                id
            );
            return Err(2);
        }
        return Ok(Scope::Under(id.to_string()));
    }

    if let Some(id) = after {
        return Ok(Scope::After(id.to_string()));
    }

    Ok(Scope::All)
}
