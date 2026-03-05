use crate::engine::store::Store;

pub fn run(store: &Store, json: bool) -> i32 {
    let errors = store.validate();
    if errors.is_empty() {
        if !json {
            println!("All documents valid.");
        }
        return 0;
    }

    if json {
        let items: Vec<_> = errors.iter().map(|e| format!("{}", e)).collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        for error in &errors {
            eprintln!("  {}", error);
        }
    }
    2
}

pub fn run_full(store: &Store, json: bool, warnings: bool) -> i32 {
    let result = store.validate_full();

    if json {
        let output = run_json(store);
        println!("{}", output);
    } else {
        let output = run_human(store, warnings);
        if output.is_empty() {
            println!("All documents valid.");
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
        output.push_str(&format!("  error: {}\n", error));
    }
    if show_warnings {
        for warning in &result.warnings {
            output.push_str(&format!("  warning: {}\n", warning));
        }
    }

    output
}
