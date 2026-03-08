use crate::cli::style::{error_prefix, warning_prefix};
use crate::engine::config::Config;
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

pub fn run_full(store: &Store, config: &Config, json: bool, warnings: bool) -> i32 {
    let result = store.validate_full(config);

    if json {
        let output = run_json(store, config);
        println!("{}", output);
    } else {
        let output = run_human(store, config, warnings);
        if output.is_empty() {
            println!("{}", success_message());
        } else {
            eprint!("{}", output);
        }
    }

    if result.errors.is_empty() && store.parse_errors().is_empty() { 0 } else { 2 }
}

pub fn run_json(store: &Store, config: &Config) -> String {
    let result = store.validate_full(config);
    let errors: Vec<_> = result.errors.iter().map(|e| format!("{}", e)).collect();
    let warnings: Vec<_> = result.warnings.iter().map(|w| format!("{}", w)).collect();
    let parse_errors: Vec<_> = store.parse_errors().iter().map(|pe| {
        serde_json::json!({ "path": pe.path.display().to_string(), "error": pe.error })
    }).collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "errors": errors,
        "warnings": warnings,
        "parse_errors": parse_errors,
    }))
    .unwrap()
}

pub fn run_human(store: &Store, config: &Config, show_warnings: bool) -> String {
    let result = store.validate_full(config);
    let mut output = String::new();

    for pe in store.parse_errors() {
        output.push_str(&format!("  {} parse error in {}: {}\n", error_prefix(), pe.path.display(), pe.error));
    }
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
