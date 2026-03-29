mod code_fence;
mod resolve;

pub use resolve::language_from_extension;

use anyhow::Result;
use regex::Regex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use code_fence::{find_fenced_code_ranges, is_inside_fence};

pub const REF_PATTERN: &str =
    r"@ref\s+([^#@\s]+)(?:#([^@\s]+))?(?:@\{blob:([a-fA-F0-9]+)\}|@([a-fA-F0-9]+))?";

/// A parsed `@ref` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    /// The file path from the ref directive.
    pub path: String,
    /// The optional `#symbol` fragment.
    pub symbol: Option<String>,
    /// The optional `@{blob:<hex>}` pinning hash.
    pub blob_hash: Option<String>,
    /// The legacy optional `@<hex>` commit SHA.
    pub commit_sha: Option<String>,
    /// Byte offsets of the full match in the source text.
    pub span: (usize, usize),
}

/// Parse all `@ref` directives from `content`, skipping those inside fenced code blocks.
pub fn parse_refs(content: &str) -> Vec<Ref> {
    let re = Regex::new(REF_PATTERN).expect("invalid REF_PATTERN regex");
    let fenced_ranges = find_fenced_code_ranges(content);
    let mut refs = Vec::new();

    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        if is_inside_fence(&fenced_ranges, full_match.start()) {
            continue;
        }
        refs.push(Ref {
            path: cap.get(1).unwrap().as_str().to_string(),
            symbol: cap.get(2).map(|m| m.as_str().to_string()),
            blob_hash: cap.get(3).map(|m| m.as_str().to_string()),
            commit_sha: cap.get(4).map(|m| m.as_str().to_string()),
            span: (full_match.start(), full_match.end()),
        });
    }

    refs
}

pub struct RefExpander {
    pub(crate) root: PathBuf,
    pub(crate) max_lines: usize,
}

impl RefExpander {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            max_lines: 25,
        }
    }

    pub fn with_max_lines(root: PathBuf, max_lines: usize) -> Self {
        Self { root, max_lines }
    }

    pub fn expand(&self, content: &str) -> Result<String> {
        let parsed = parse_refs(content);
        let head_sha = self.resolve_head_short_sha();
        let mut result = content.to_string();
        let mut offsets: Vec<(usize, usize, String)> = Vec::new();

        for r in &parsed {
            // blob_hash is parsed and stored but not yet used for resolution;
            // pass only the legacy commit_sha to resolve_ref.
            let sha = r.commit_sha.as_deref();

            let expanded =
                self.resolve_ref(&r.path, r.symbol.as_deref(), sha, head_sha.as_deref())?;
            offsets.push((r.span.0, r.span.1, expanded));
        }

        for (start, end, expanded) in offsets.into_iter().rev() {
            result.replace_range(start..end, &expanded);
        }

        Ok(result)
    }

    pub fn expand_cancellable(&self, content: &str, cancel: &AtomicBool) -> Result<Option<String>> {
        let parsed = parse_refs(content);
        let head_sha = self.resolve_head_short_sha();
        let mut result = content.to_string();
        let mut offsets: Vec<(usize, usize, String)> = Vec::new();

        for r in &parsed {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            // blob_hash is parsed and stored but not yet used for resolution;
            // pass only the legacy commit_sha to resolve_ref.
            let sha = r.commit_sha.as_deref();

            let expanded =
                self.resolve_ref(&r.path, r.symbol.as_deref(), sha, head_sha.as_deref())?;
            offsets.push((r.span.0, r.span.1, expanded));
        }

        for (start, end, expanded) in offsets.into_iter().rev() {
            result.replace_range(start..end, &expanded);
        }

        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn test_expand_cancellable_returns_none_when_cancelled() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);
        let cancel = AtomicBool::new(true);
        let content = "See code:\n\n@ref Cargo.toml\n";
        let result = expander.expand_cancellable(content, &cancel);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_expand_cancellable_returns_expanded_when_not_cancelled() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);
        let cancel = AtomicBool::new(false);
        let content = "See code:\n\n@ref Cargo.toml\n";
        let result = expander.expand_cancellable(content, &cancel);
        assert!(result.is_ok());
        let expanded = result
            .unwrap()
            .expect("should return Some when not cancelled");
        assert!(expanded.contains("[package]") || expanded.contains("```"));
    }

    #[test]
    fn test_language_from_extension_ts() {
        assert_eq!(language_from_extension("src/utils.ts"), "ts");
        assert_eq!(language_from_extension("src/utils.tsx"), "ts");
    }

    #[test]
    fn test_language_from_extension_rs() {
        assert_eq!(language_from_extension("src/foo.rs"), "rust");
    }

    #[test]
    fn test_language_from_extension_py() {
        assert_eq!(language_from_extension("src/utils.py"), "python");
    }

    #[test]
    fn test_ref_regex_parsing_basic() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let cap = re.captures("@ref src/foo.rs#MyStruct").unwrap();
        assert_eq!(cap.get(1).map(|m| m.as_str()), Some("src/foo.rs"));
        assert_eq!(cap.get(2).map(|m| m.as_str()), Some("MyStruct"));
        assert_eq!(cap.get(3).map(|m| m.as_str()), None);
        assert_eq!(cap.get(4).map(|m| m.as_str()), None);
    }

    #[test]
    fn test_ref_regex_parsing_with_sha() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let cap = re
            .captures("@ref src/utils.ts#SomeInterface@abc1234")
            .unwrap();
        assert_eq!(cap.get(1).map(|m| m.as_str()), Some("src/utils.ts"));
        assert_eq!(cap.get(2).map(|m| m.as_str()), Some("SomeInterface"));
        assert_eq!(cap.get(3).map(|m| m.as_str()), None);
        assert_eq!(cap.get(4).map(|m| m.as_str()), Some("abc1234"));
    }

    #[test]
    fn test_ref_regex_parsing_path_only() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let cap = re.captures("@ref src/foo.rs").unwrap();
        assert_eq!(cap.get(1).map(|m| m.as_str()), Some("src/foo.rs"));
        assert_eq!(cap.get(2).map(|m| m.as_str()), None);
        assert_eq!(cap.get(3).map(|m| m.as_str()), None);
        assert_eq!(cap.get(4).map(|m| m.as_str()), None);
    }

    #[test]
    fn test_ref_regex_parsing_multiple() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let content = "Start @ref src/a.rs#Foo@abc end @ref src/b.rs#Bar@def done";
        let matches: Vec<_> = re.captures_iter(content).collect();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].get(1).map(|m| m.as_str()), Some("src/a.rs"));
        assert_eq!(matches[0].get(2).map(|m| m.as_str()), Some("Foo"));
        assert_eq!(matches[0].get(4).map(|m| m.as_str()), Some("abc"));
        assert_eq!(matches[1].get(1).map(|m| m.as_str()), Some("src/b.rs"));
        assert_eq!(matches[1].get(2).map(|m| m.as_str()), Some("Bar"));
        assert_eq!(matches[1].get(4).map(|m| m.as_str()), Some("def"));
    }

    #[test]
    fn test_expand_refs_single_ref() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "See the code:\n\n@ref Cargo.toml\n";
        let result = expander.expand(content);

        assert!(result.is_ok());
        let expanded = result.unwrap();
        assert!(expanded.contains("[package]") || expanded.contains("> [unresolved:"));
    }

    #[test]
    fn test_expand_refs_with_symbol() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "See struct:\n\n@ref src/engine/store.rs#Store\n";
        let result = expander.expand(content);

        assert!(result.is_ok());
        let expanded = result.unwrap();
        assert!(expanded.contains("```rust") || expanded.contains("error"));
    }

    #[test]
    fn test_expand_refs_multiple_refs() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "First @ref Cargo.toml then @ref src/engine/mod.rs";
        let result = expander.expand(content);

        assert!(result.is_ok());
        let expanded = result.unwrap();
        assert!(expanded.contains("```") || expanded.contains("error"));
    }

    #[test]
    fn test_expand_refs_error_handling() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "Ref: @ref nonexistent/file.rs";
        let result = expander.expand(content);

        assert!(result.is_ok());
        let expanded = result.unwrap();
        assert!(expanded.contains("> [unresolved:"));
    }

    #[test]
    fn test_unknown_symbol_produces_unresolved_marker() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "See: @ref Cargo.toml#NonExistentSymbol";
        let result = expander.expand(content).unwrap();
        assert!(
            result.contains("> [unresolved: Cargo.toml#NonExistentSymbol]"),
            "Expected unresolved marker, got: {}",
            result
        );
        assert!(
            !result.contains("[package]"),
            "Should not dump full file content for unknown symbol"
        );
    }

    #[test]
    fn test_expand_with_blob_ref_falls_through_to_head() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);
        let content = "Check this: @ref Cargo.toml@{blob:1234}\n";
        let result = expander.expand(content);
        assert!(
            result.is_ok(),
            "expand should not panic or error on blob ref"
        );
        let expanded = result.unwrap();
        // blob hash is not yet used for resolution, so it falls through to HEAD
        assert!(
            expanded.contains("```toml") || expanded.contains("[package]"),
            "Blob-pinned ref should resolve against HEAD and produce a code fence, got: {}",
            expanded
        );
        assert!(
            !expanded.contains("@ref"),
            "The @ref directive should be replaced in the output, got: {}",
            expanded
        );
    }

    // --- blob ref / parse_refs tests ---

    #[test]
    fn test_parse_symbol_blob_ref_basic() {
        let refs = parse_refs("@ref src/engine/refs.rs#RefExpander@{blob:a1b2c3d4}");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "src/engine/refs.rs");
        assert_eq!(r.symbol.as_deref(), Some("RefExpander"));
        assert_eq!(r.blob_hash.as_deref(), Some("a1b2c3d4"));
        assert_eq!(r.commit_sha, None);
    }

    #[test]
    fn test_parse_symbol_blob_ref_full_sha() {
        let refs =
            parse_refs("@ref src/foo.rs#MyStruct@{blob:abc123def456abc123def456abc123def456abcd}");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "src/foo.rs");
        assert_eq!(r.symbol.as_deref(), Some("MyStruct"));
        assert_eq!(
            r.blob_hash.as_deref(),
            Some("abc123def456abc123def456abc123def456abcd")
        );
        assert_eq!(r.commit_sha, None);
    }

    #[test]
    fn test_parse_symbol_blob_ref_in_sentence() {
        let refs = parse_refs("See @ref src/foo.rs#Bar@{blob:dead0000} for details");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "src/foo.rs");
        assert_eq!(r.symbol.as_deref(), Some("Bar"));
        assert_eq!(r.blob_hash.as_deref(), Some("dead0000"));
        assert_eq!(r.commit_sha, None);
    }

    #[test]
    fn test_parse_file_blob_ref() {
        let refs = parse_refs("@ref config/schema.json@{blob:cafebabe}");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "config/schema.json");
        assert_eq!(r.symbol, None);
        assert_eq!(r.blob_hash.as_deref(), Some("cafebabe"));
        assert_eq!(r.commit_sha, None);
    }

    #[test]
    fn test_parse_file_blob_ref_no_symbol() {
        let refs = parse_refs("@ref Cargo.toml@{blob:1234abcd}");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "Cargo.toml");
        assert_eq!(r.symbol, None);
        assert_eq!(r.blob_hash.as_deref(), Some("1234abcd"));
    }

    #[test]
    fn test_parse_unpinned_ref_with_symbol() {
        let refs = parse_refs("@ref src/foo.rs#MyStruct");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "src/foo.rs");
        assert_eq!(r.symbol.as_deref(), Some("MyStruct"));
        assert_eq!(r.blob_hash, None);
        assert_eq!(r.commit_sha, None);
    }

    #[test]
    fn test_parse_unpinned_ref_path_only() {
        let refs = parse_refs("@ref src/foo.rs");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "src/foo.rs");
        assert_eq!(r.symbol, None);
        assert_eq!(r.blob_hash, None);
        assert_eq!(r.commit_sha, None);
    }

    #[test]
    fn test_parse_legacy_commit_sha_ref() {
        let refs = parse_refs("@ref src/foo.rs#Bar@abc1234");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.path, "src/foo.rs");
        assert_eq!(r.symbol.as_deref(), Some("Bar"));
        assert_eq!(r.commit_sha.as_deref(), Some("abc1234"));
        assert_eq!(r.blob_hash, None);
    }

    #[test]
    fn test_regex_does_not_match_blob_syntax_as_legacy() {
        let refs = parse_refs("@ref src/foo.rs@{blob:aabb}");
        assert_eq!(refs.len(), 1);
        let r = &refs[0];
        assert_eq!(r.commit_sha, None);
        assert_eq!(r.blob_hash.as_deref(), Some("aabb"));
    }

    #[test]
    fn test_multiple_refs_mixed_pinning() {
        let content = "@ref a.rs#Foo then @ref b.rs#Bar@abc123 and @ref c.rs@{blob:dead}";
        let refs = parse_refs(content);
        assert_eq!(refs.len(), 3);

        // unpinned
        assert_eq!(refs[0].path, "a.rs");
        assert_eq!(refs[0].symbol.as_deref(), Some("Foo"));
        assert_eq!(refs[0].blob_hash, None);
        assert_eq!(refs[0].commit_sha, None);

        // legacy SHA
        assert_eq!(refs[1].path, "b.rs");
        assert_eq!(refs[1].symbol.as_deref(), Some("Bar"));
        assert_eq!(refs[1].commit_sha.as_deref(), Some("abc123"));
        assert_eq!(refs[1].blob_hash, None);

        // blob hash
        assert_eq!(refs[2].path, "c.rs");
        assert_eq!(refs[2].symbol, None);
        assert_eq!(refs[2].blob_hash.as_deref(), Some("dead"));
        assert_eq!(refs[2].commit_sha, None);
    }

    #[test]
    fn test_blob_ref_inside_code_fence_is_skipped() {
        let content = "before\n```\n@ref src/foo.rs@{blob:aabb}\n```\nafter";
        let refs = parse_refs(content);
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_line_number_ref_returns_single_line() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "See: @ref Cargo.toml#1";
        let result = expander.expand(content).unwrap();
        assert!(
            result.contains("[package]"),
            "Should contain the first line"
        );
        let lines_in_block: Vec<&str> = result
            .lines()
            .skip_while(|l| !l.starts_with("```"))
            .skip(1)
            .take_while(|l| !l.starts_with("```"))
            .collect();
        assert_eq!(
            lines_in_block.len(),
            1,
            "Line-number ref should return exactly one line, got: {:?}",
            lines_in_block
        );
    }
}
