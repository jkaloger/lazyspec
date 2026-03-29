use anyhow::{bail, Context, Result};
use std::path::Path;

use super::config::Config;
use super::hashing;
use super::symbols::{
    normalize_ast, RustSymbolExtractor, SymbolExtractor, TypeScriptSymbolExtractor,
};

use tree_sitter_rust::LANGUAGE as LANGUAGE_RUST;
use tree_sitter_typescript::LANGUAGE_TYPESCRIPT;

/// Compute a blob hash for a ref target.
///
/// - If `symbol` is `None` (whole-file ref): hashes raw file content via `git hash-object`.
/// - If `symbol` is `Some(name)`: extracts the symbol, optionally normalises (strip comments,
///   collapse whitespace), then hashes the result.
pub fn compute_blob_hash(
    root: &Path,
    file_path: &str,
    symbol: Option<&str>,
    normalize: bool,
) -> Result<String> {
    let full_path = root.join(file_path);

    match symbol {
        None => hashing::hash_file(&full_path),
        Some(name) => {
            let source = std::fs::read_to_string(&full_path)
                .with_context(|| format!("failed to read {}", full_path.display()))?;

            let ext = Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let extracted = match ext {
                "rs" => RustSymbolExtractor::new().extract(&source, name),
                "ts" | "tsx" => TypeScriptSymbolExtractor::new().extract(&source, name),
                _ => bail!("unsupported file extension for symbol extraction: {ext}"),
            };

            let extracted =
                extracted.with_context(|| format!("symbol '{name}' not found in {file_path}"))?;

            if normalize {
                let lang: tree_sitter::Language = match ext {
                    "rs" => LANGUAGE_RUST.into(),
                    "ts" | "tsx" => LANGUAGE_TYPESCRIPT.into(),
                    _ => unreachable!(),
                };
                let normalized = normalize_ast(&extracted, lang);
                hashing::hash_bytes(normalized.as_bytes())
            } else {
                hashing::hash_bytes(extracted.as_bytes())
            }
        }
    }
}

/// Higher-level convenience wrapper that resolves the normalize flag from config
/// before computing the blob hash. This is the entry point the pin command will call.
pub fn compute_blob_hash_for_spec(
    root: &Path,
    config: &Config,
    spec_path: &str,
    file_path: &str,
    symbol: Option<&str>,
) -> Result<String> {
    let normalize = config.certification.should_normalize(spec_path);
    compute_blob_hash(root, file_path, symbol, normalize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_symbol_semantic_hash_roundtrip() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("lib.rs"),
            "pub fn greet() { println!(\"hi\"); }\n",
        )
        .unwrap();

        let h1 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();
        let h2 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 40);
    }

    #[test]
    fn test_comment_only_change_no_drift() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");

        let v1 = "pub fn greet() { println!(\"hi\"); }\n";
        let v2 =
            "// a friendly greeting\npub fn greet() {\n    // say hi\n    println!(\"hi\");\n}\n";

        fs::write(&path, v1).unwrap();
        let h1 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        fs::write(&path, v2).unwrap();
        let h2 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        assert_eq!(h1, h2, "comment-only change should not drift");
    }

    #[test]
    fn test_whitespace_only_change_no_drift() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");

        let v1 = "pub fn greet() { println!(\"hi\"); }\n";
        let v2 = "pub fn greet()  {\n\n    println!(\"hi\");\n\n}\n";

        fs::write(&path, v1).unwrap();
        let h1 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        fs::write(&path, v2).unwrap();
        let h2 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        assert_eq!(h1, h2, "whitespace-only change should not drift");
    }

    #[test]
    fn test_structural_change_produces_drift() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");

        let v1 = "pub fn greet() { println!(\"hi\"); }\n";
        let v2 = "pub fn greet(name: &str) { println!(\"hi {}\", name); }\n";

        fs::write(&path, v1).unwrap();
        let h1 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        fs::write(&path, v2).unwrap();
        let h2 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        assert_ne!(h1, h2, "adding a parameter should produce drift");
    }

    #[test]
    fn test_type_change_produces_drift() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");

        let v1 = "pub fn greet() -> &'static str { \"hi\" }\n";
        let v2 = "pub fn greet() -> String { String::from(\"hi\") }\n";

        fs::write(&path, v1).unwrap();
        let h1 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        fs::write(&path, v2).unwrap();
        let h2 = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        assert_ne!(h1, h2, "changing return type should produce drift");
    }

    #[test]
    fn test_whole_file_hash_no_normalization() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");
        let content = "pub fn greet() { println!(\"hi\"); }\n";
        fs::write(&path, content).unwrap();

        let file_hash = compute_blob_hash(dir.path(), "lib.rs", None, false).unwrap();
        let direct_hash = hashing::hash_file(&path).unwrap();
        assert_eq!(
            file_hash, direct_hash,
            "whole-file hash should use raw content"
        );
    }

    #[test]
    fn test_whole_file_hash_comment_change_drifts() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");

        let v1 = "pub fn greet() { println!(\"hi\"); }\n";
        let v2 = "// added comment\npub fn greet() { println!(\"hi\"); }\n";

        fs::write(&path, v1).unwrap();
        let h1 = compute_blob_hash(dir.path(), "lib.rs", None, false).unwrap();

        fs::write(&path, v2).unwrap();
        let h2 = compute_blob_hash(dir.path(), "lib.rs", None, false).unwrap();

        assert_ne!(
            h1, h2,
            "whole-file hash should change when a comment is added"
        );
    }

    #[test]
    fn test_compute_for_spec_default_normalizes() {
        use super::super::config::Config;

        let dir = TempDir::new().unwrap();
        let source = "// a comment\npub fn greet() { println!(\"hi\"); }\n";
        fs::write(dir.path().join("lib.rs"), source).unwrap();

        let config = Config::default();

        let for_spec_hash = compute_blob_hash_for_spec(
            dir.path(),
            &config,
            "docs/specs/SPEC-001",
            "lib.rs",
            Some("greet"),
        )
        .unwrap();

        let direct_normalized =
            compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        assert_eq!(
            for_spec_hash, direct_normalized,
            "default config should produce a normalized hash"
        );
    }

    #[test]
    fn test_compute_for_spec_override_skips_normalize() {
        use super::super::config::{CertificationConfig, CertificationOverride, Config};
        use std::collections::HashMap;

        let dir = TempDir::new().unwrap();
        // Source with a comment so normalized vs raw hashes differ
        let source = "// a comment\npub fn greet() { println!(\"hi\"); }\n";
        fs::write(dir.path().join("lib.rs"), source).unwrap();

        let mut overrides = HashMap::new();
        overrides.insert(
            "docs/specs/SPEC-007".to_string(),
            CertificationOverride { normalize: false },
        );

        let mut config = Config::default();
        config.certification = CertificationConfig {
            normalize: true,
            overrides,
        };

        let for_spec_hash = compute_blob_hash_for_spec(
            dir.path(),
            &config,
            "docs/specs/SPEC-007",
            "lib.rs",
            Some("greet"),
        )
        .unwrap();

        let direct_raw = compute_blob_hash(dir.path(), "lib.rs", Some("greet"), false).unwrap();
        let direct_normalized =
            compute_blob_hash(dir.path(), "lib.rs", Some("greet"), true).unwrap();

        assert_eq!(
            for_spec_hash, direct_raw,
            "override normalize=false should produce a raw hash"
        );
        assert_ne!(
            for_spec_hash, direct_normalized,
            "raw hash should differ from normalized hash when source has comments"
        );
    }
}
