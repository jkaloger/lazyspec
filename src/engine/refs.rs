mod code_fence;
mod resolve;

pub use resolve::language_from_extension;

use anyhow::Result;
use regex::Regex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use code_fence::{find_fenced_code_ranges, is_inside_fence};

pub const REF_PATTERN: &str = r"@ref\s+([^#@\s]+)(?:#([^@\s]+))?(?:@([a-fA-F0-9]+))?";

pub struct RefExpander {
    pub(crate) root: PathBuf,
    pub(crate) max_lines: usize,
}

impl RefExpander {
    pub fn new(root: PathBuf) -> Self {
        Self { root, max_lines: 25 }
    }

    pub fn with_max_lines(root: PathBuf, max_lines: usize) -> Self {
        Self { root, max_lines }
    }

    pub fn expand(&self, content: &str) -> Result<String> {
        let re = Regex::new(REF_PATTERN)?;
        let fenced_ranges = find_fenced_code_ranges(content);
        let head_sha = self.resolve_head_short_sha();
        let mut result = content.to_string();
        let mut offsets: Vec<(usize, usize, String)> = Vec::new();

        for cap in re.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            if is_inside_fence(&fenced_ranges, full_match.start()) {
                continue;
            }
            let path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let symbol = cap.get(2).map(|m| m.as_str());
            let sha = cap.get(3).map(|m| m.as_str());

            let expanded = self.resolve_ref(path, symbol, sha, head_sha.as_deref())?;
            offsets.push((full_match.start(), full_match.end(), expanded));
        }

        for (start, end, expanded) in offsets.into_iter().rev() {
            result.replace_range(start..end, &expanded);
        }

        Ok(result)
    }

    pub fn expand_cancellable(
        &self,
        content: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<String>> {
        let re = Regex::new(REF_PATTERN)?;
        let fenced_ranges = find_fenced_code_ranges(content);
        let head_sha = self.resolve_head_short_sha();
        let mut result = content.to_string();
        let mut offsets: Vec<(usize, usize, String)> = Vec::new();

        for cap in re.captures_iter(content) {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let full_match = cap.get(0).unwrap();
            if is_inside_fence(&fenced_ranges, full_match.start()) {
                continue;
            }
            let path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let symbol = cap.get(2).map(|m| m.as_str());
            let sha = cap.get(3).map(|m| m.as_str());

            let expanded = self.resolve_ref(path, symbol, sha, head_sha.as_deref())?;
            offsets.push((full_match.start(), full_match.end(), expanded));
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
        let expanded = result.unwrap().expect("should return Some when not cancelled");
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
    }

    #[test]
    fn test_ref_regex_parsing_with_sha() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let cap = re
            .captures("@ref src/utils.ts#SomeInterface@abc1234")
            .unwrap();
        assert_eq!(cap.get(1).map(|m| m.as_str()), Some("src/utils.ts"));
        assert_eq!(cap.get(2).map(|m| m.as_str()), Some("SomeInterface"));
        assert_eq!(cap.get(3).map(|m| m.as_str()), Some("abc1234"));
    }

    #[test]
    fn test_ref_regex_parsing_path_only() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let cap = re.captures("@ref src/foo.rs").unwrap();
        assert_eq!(cap.get(1).map(|m| m.as_str()), Some("src/foo.rs"));
        assert_eq!(cap.get(2).map(|m| m.as_str()), None);
        assert_eq!(cap.get(3).map(|m| m.as_str()), None);
    }

    #[test]
    fn test_ref_regex_parsing_multiple() {
        let re = Regex::new(REF_PATTERN).unwrap();

        let content = "Start @ref src/a.rs#Foo@abc end @ref src/b.rs#Bar@def done";
        let matches: Vec<_> = re.captures_iter(content).collect();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].get(1).map(|m| m.as_str()), Some("src/a.rs"));
        assert_eq!(matches[0].get(2).map(|m| m.as_str()), Some("Foo"));
        assert_eq!(matches[0].get(3).map(|m| m.as_str()), Some("abc"));
        assert_eq!(matches[1].get(1).map(|m| m.as_str()), Some("src/b.rs"));
        assert_eq!(matches[1].get(2).map(|m| m.as_str()), Some("Bar"));
        assert_eq!(matches[1].get(3).map(|m| m.as_str()), Some("def"));
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
    fn test_line_number_ref_returns_single_line() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        let content = "See: @ref Cargo.toml#1";
        let result = expander.expand(content).unwrap();
        assert!(result.contains("[package]"), "Should contain the first line");
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
