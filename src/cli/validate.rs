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
