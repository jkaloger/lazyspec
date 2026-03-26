use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::certification::compute_blob_hash_for_spec;
use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::refs::{parse_refs, Ref};
use crate::engine::store::{ResolveError, Store};
use anyhow::{Context, Result};
use std::path::Path;

/// Result of pinning a single ref.
#[derive(Debug, Clone)]
pub struct PinnedRef {
    pub target: String,
    pub hash: String,
}

/// Error for a single ref that could not be resolved.
#[derive(Debug, Clone)]
pub struct PinError {
    pub target: String,
    pub message: String,
}

/// Result of pinning all refs in a document.
#[derive(Debug)]
pub struct PinResult {
    pub pinned: Vec<PinnedRef>,
    pub errors: Vec<PinError>,
    pub new_body: String,
}

fn ref_target(r: &Ref) -> String {
    match &r.symbol {
        Some(sym) => format!("{}#{}", r.path, sym),
        None => r.path.clone(),
    }
}

/// Core pin logic: parse refs, compute hashes, rewrite body.
pub fn pin_document(
    root: &Path,
    config: &Config,
    spec_path: &str,
    body: &str,
) -> PinResult {
    let refs = parse_refs(body);
    let mut pinned = Vec::new();
    let mut errors = Vec::new();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for r in &refs {
        let target = ref_target(r);
        match compute_blob_hash_for_spec(
            root,
            config,
            spec_path,
            &r.path,
            r.symbol.as_deref(),
        ) {
            Ok(hash) => {
                // Build the new ref string
                let new_ref = match &r.symbol {
                    Some(sym) => format!("@ref {}#{}@{{blob:{}}}", r.path, sym, hash),
                    None => format!("@ref {}@{{blob:{}}}", r.path, hash),
                };
                replacements.push((r.span.0, r.span.1, new_ref));
                pinned.push(PinnedRef { target, hash });
            }
            Err(e) => {
                // Leave the ref unchanged
                errors.push(PinError {
                    target,
                    message: format!("{:#}", e),
                });
            }
        }
    }

    // Apply replacements in reverse order to preserve offsets
    let mut new_body = body.to_string();
    for (start, end, replacement) in replacements.into_iter().rev() {
        new_body.replace_range(start..end, &replacement);
    }

    PinResult {
        pinned,
        errors,
        new_body,
    }
}

pub fn run(store: &Store, config: &Config, id: &str, json: bool) -> Result<()> {
    let doc = match resolve_shorthand_or_path(store, id) {
        Ok(doc) => doc,
        Err(ResolveError::Ambiguous { id, matches }) => {
            if json {
                let paths: Vec<String> =
                    matches.iter().map(|m| m.to_string_lossy().to_string()).collect();
                let output = serde_json::json!({
                    "error": "ambiguous_id",
                    "id": id,
                    "ambiguous_matches": paths,
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                eprintln!("Ambiguous ID '{}' matches multiple documents:", id);
                for m in &matches {
                    eprintln!("  {}", m.display());
                }
            }
            return Ok(());
        }
        Err(ResolveError::NotFound(id)) => {
            return Err(anyhow::anyhow!("document not found: {}", id));
        }
    };

    let root = store.root();
    let full_path = root.join(&doc.path);
    let spec_path = doc.path.to_string_lossy();

    // Read the full file content
    let content = std::fs::read_to_string(&full_path)
        .with_context(|| format!("failed to read {}", full_path.display()))?;

    // Extract body from frontmatter
    let body = DocMeta::extract_body(&content)
        .with_context(|| format!("failed to parse frontmatter in {}", full_path.display()))?;

    // Pin the refs in the body
    let result = pin_document(root, config, &spec_path, &body);

    // Rewrite the file: replace the body portion in the original content
    if !result.pinned.is_empty() {
        let frontmatter_end = find_body_start(&content)?;
        let prefix = &content[..frontmatter_end];
        let new_content = format!("{}{}", prefix, result.new_body);
        std::fs::write(&full_path, new_content)
            .with_context(|| format!("failed to write {}", full_path.display()))?;
    }

    // Output results
    if json {
        let output = serde_json::json!({
            "pinned": result.pinned.iter().map(|p| serde_json::json!({
                "target": p.target,
                "hash": p.hash,
            })).collect::<Vec<_>>(),
            "errors": result.errors.iter().map(|e| serde_json::json!({
                "target": e.target,
                "message": e.message,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let pinned_count = result.pinned.len();
        let error_count = result.errors.len();
        if pinned_count > 0 || error_count > 0 {
            eprintln!(
                "Pinned {} ref{}, {} error{}",
                pinned_count,
                if pinned_count == 1 { "" } else { "s" },
                error_count,
                if error_count == 1 { "" } else { "s" },
            );
        } else {
            eprintln!("No @ref directives found");
        }
        for e in &result.errors {
            eprintln!("  error: {}: {}", e.target, e.message);
        }
    }

    Ok(())
}

/// Find the byte offset where the body starts (after the frontmatter closing delimiter and its newline).
fn find_body_start(content: &str) -> Result<usize> {
    let trimmed = content.trim_start();
    let leading_ws = content.len() - trimmed.len();
    if !trimmed.starts_with("---") {
        anyhow::bail!("no frontmatter found");
    }
    let after_first = &trimmed[3..];
    let end = after_first
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("no closing frontmatter delimiter"))?;
    // Position after the closing "---" plus its trailing newline
    let close_pos = leading_ws + 3 + end + 4; // skip "\n---"
    // extract_body does: body = after_first[end + 4..], then trim_start_matches('\n')
    // We need to include the newlines that extract_body trims, so the prefix ends right
    // at the point where the trimmed body starts.
    let remainder = &content[close_pos..];
    let trimmed_start = remainder.len() - remainder.trim_start_matches('\n').len();
    Ok(close_pos + trimmed_start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use crate::engine::hashing;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Set up a temp dir as a git repo with an initial commit.
    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let path = dir.path();
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        // Create an initial commit so HEAD exists
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init"])
            .current_dir(path)
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn test_pin_writes_hash_for_file_ref() {
        let dir = setup_git_repo();
        let root = dir.path();
        let config = Config::default();

        // Create a file to reference
        let file_content = "hello world\n";
        fs::write(root.join("hello.txt"), file_content).unwrap();

        let body = "Some text\n\n@ref hello.txt\n\nMore text\n";
        let result = pin_document(root, &config, "docs/specs/SPEC-001", body);

        assert_eq!(result.pinned.len(), 1);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.pinned[0].target, "hello.txt");

        // Verify the hash matches what hash_file produces
        let expected_hash = hashing::hash_file(&root.join("hello.txt")).unwrap();
        assert_eq!(result.pinned[0].hash, expected_hash);

        // Verify the body was rewritten correctly
        let expected_ref = format!("@ref hello.txt@{{blob:{}}}", expected_hash);
        assert!(
            result.new_body.contains(&expected_ref),
            "Expected body to contain '{}', got: {}",
            expected_ref,
            result.new_body
        );
    }

    #[test]
    fn test_pin_writes_hash_for_symbol_ref() {
        let dir = setup_git_repo();
        let root = dir.path();
        let config = Config::default();

        let rust_source = "pub struct MyStruct {\n    pub field: i32,\n}\n";
        fs::write(root.join("foo.rs"), rust_source).unwrap();

        let body = "Spec body\n\n@ref foo.rs#MyStruct\n";
        let result = pin_document(root, &config, "docs/specs/SPEC-001", body);

        assert_eq!(result.pinned.len(), 1);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.pinned[0].target, "foo.rs#MyStruct");
        assert_eq!(result.pinned[0].hash.len(), 40);

        let expected_ref = format!("@ref foo.rs#MyStruct@{{blob:{}}}", result.pinned[0].hash);
        assert!(
            result.new_body.contains(&expected_ref),
            "Expected body to contain '{}', got: {}",
            expected_ref,
            result.new_body
        );
    }

    #[test]
    fn test_pin_updates_existing_hash() {
        let dir = setup_git_repo();
        let root = dir.path();
        let config = Config::default();

        let file_content = "some content\n";
        fs::write(root.join("data.txt"), file_content).unwrap();

        let body = "Text before\n\n@ref data.txt@{blob:aabb0011}\n\nText after\n";
        let result = pin_document(root, &config, "docs/specs/SPEC-001", body);

        assert_eq!(result.pinned.len(), 1);
        assert_eq!(result.errors.len(), 0);

        let fresh_hash = hashing::hash_file(&root.join("data.txt")).unwrap();
        assert_eq!(result.pinned[0].hash, fresh_hash);
        assert_ne!(fresh_hash, "aabb0011");

        // Old hash should be gone
        assert!(!result.new_body.contains("aabb0011"));
        // New hash should be present
        let expected_ref = format!("@ref data.txt@{{blob:{}}}", fresh_hash);
        assert!(result.new_body.contains(&expected_ref));
    }

    #[test]
    fn test_pin_errors_on_nonexistent_file() {
        let dir = setup_git_repo();
        let root = dir.path();
        let config = Config::default();

        let body = "See @ref nonexistent.rs for details\n";
        let result = pin_document(root, &config, "docs/specs/SPEC-001", body);

        assert_eq!(result.pinned.len(), 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].target, "nonexistent.rs");

        // The ref should be unchanged
        assert!(result.new_body.contains("@ref nonexistent.rs"));
    }

    #[test]
    fn test_pin_errors_on_nonexistent_symbol() {
        let dir = setup_git_repo();
        let root = dir.path();
        let config = Config::default();

        let rust_source = "pub fn real_fn() {}\n";
        fs::write(root.join("real_file.rs"), rust_source).unwrap();

        let body = "See @ref real_file.rs#NoSuchSymbol\n";
        let result = pin_document(root, &config, "docs/specs/SPEC-001", body);

        assert_eq!(result.pinned.len(), 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].target, "real_file.rs#NoSuchSymbol");

        // The ref should be unchanged
        assert!(result.new_body.contains("@ref real_file.rs#NoSuchSymbol"));
    }

    #[test]
    fn test_pin_mixed_valid_and_invalid() {
        let dir = setup_git_repo();
        let root = dir.path();
        let config = Config::default();

        let file_content = "valid content\n";
        fs::write(root.join("valid.txt"), file_content).unwrap();

        let body = "First: @ref valid.txt\nSecond: @ref missing.txt\n";
        let result = pin_document(root, &config, "docs/specs/SPEC-001", body);

        assert_eq!(result.pinned.len(), 1);
        assert_eq!(result.errors.len(), 1);

        assert_eq!(result.pinned[0].target, "valid.txt");
        assert_eq!(result.errors[0].target, "missing.txt");

        // Valid ref should have hash
        let expected_hash = hashing::hash_file(&root.join("valid.txt")).unwrap();
        let expected_ref = format!("@ref valid.txt@{{blob:{}}}", expected_hash);
        assert!(result.new_body.contains(&expected_ref));

        // Invalid ref should be unchanged
        assert!(result.new_body.contains("@ref missing.txt"));
    }
}
