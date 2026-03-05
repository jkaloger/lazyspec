use crate::cli::style::{error_prefix, warning_prefix};
use crate::engine::store::Store;
use console::{colors_enabled, Style};

fn success_message() -> String {
    if colors_enabled() {
        format!(
            "{} All documents valid.",
            Style::new().green().bold().apply_to("\u{2713}")
        )
    } else {
        "All documents valid.".to_string()
    }
}

pub fn run_full(store: &Store, json: bool, warnings: bool) -> i32 {
    let result = store.validate_full();

    if json {
        let output = run_json(store);
        println!("{}", output);
    } else {
        let output = run_human(store, warnings);
        if output.is_empty() {
            println!("{}", success_message());
        } else {
            eprint!("{}", output);
        }
    }

    if result.errors.is_empty() { 0 } else { 2 }
}

pub fn run_json(store: &Store) -> String {
    let result = store.validate_full();
    let errors: Vec<_> = result.errors.iter().map(|e| format!("{}", e)).collect();
    let warnings: Vec<_> = result.warnings.iter().map(|w| format!("{}", w)).collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "errors": errors,
        "warnings": warnings,
    }))
    .unwrap()
}

pub fn run_human(store: &Store, show_warnings: bool) -> String {
    let result = store.validate_full();
    let mut output = String::new();

    for error in &result.errors {
        output.push_str(&format!("  {} {}\n", error_prefix(), error));
    }
    if show_warnings {
        for warning in &result.warnings {
            output.push_str(&format!("  {} {}\n", warning_prefix(), warning));
        }
    }

    output
}
