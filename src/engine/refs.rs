use crate::engine::symbols::{
    RustSymbolExtractor, SymbolExtractor, TypeScriptSymbolExtractor,
};
use anyhow::Result;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

pub const REF_PATTERN: &str = r"@ref\s+([^#@\s]+)(?:#([^@\s]+))?(?:@([a-fA-F0-9]+))?";

fn find_fenced_code_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    let bytes = content.as_bytes();
    while pos < bytes.len() {
        if bytes[pos] == b'`' && pos + 2 < bytes.len() && bytes[pos + 1] == b'`' && bytes[pos + 2] == b'`' {
            let fence_start = pos;
            // skip past the opening ``` and rest of the line
            pos += 3;
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            // find closing ```
            let mut found_close = false;
            while pos < bytes.len() {
                if bytes[pos] == b'\n' && pos + 3 < bytes.len()
                    && bytes[pos + 1] == b'`' && bytes[pos + 2] == b'`' && bytes[pos + 3] == b'`'
                {
                    pos += 4;
                    // skip rest of closing fence line
                    while pos < bytes.len() && bytes[pos] != b'\n' {
                        pos += 1;
                    }
                    ranges.push((fence_start, pos));
                    found_close = true;
                    break;
                }
                pos += 1;
            }
            if !found_close {
                ranges.push((fence_start, bytes.len()));
            }
        } else {
            pos += 1;
        }
    }
    ranges
}

fn is_inside_fence(ranges: &[(usize, usize)], offset: usize) -> bool {
    ranges.iter().any(|&(start, end)| offset >= start && offset < end)
}

pub struct RefExpander {
    root: PathBuf,
    max_lines: usize,
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

    fn resolve_head_short_sha(&self) -> Option<String> {
        let output = Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .current_dir(&self.root)
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    fn resolve_ref(
        &self,
        path: &str,
        symbol: Option<&str>,
        sha: Option<&str>,
        head_sha: Option<&str>,
    ) -> Result<String> {
        let rev = sha.unwrap_or("HEAD");
        let output = Command::new("git")
            .args(&["show", &format!("{}:{}", rev, path)])
            .current_dir(&self.root)
            .output()?;

        if !output.status.success() {
            let label = match symbol {
                Some(sym) => format!("{}#{}", path, sym),
                None => path.to_string(),
            };
            return Ok(format!("> [unresolved: {}]", label));
        }

        let file_content = String::from_utf8_lossy(&output.stdout);

        let content = if let Some(sym) = symbol {
            if sym.bytes().all(|b| b.is_ascii_digit()) {
                let line_num: usize = sym.parse().unwrap_or(0);
                let lines: Vec<&str> = file_content.lines().collect();
                if line_num == 0 || line_num > lines.len() {
                    return Ok(format!("> [unresolved: {}#{}]", path, sym));
                }
                lines[line_num - 1].to_string()
            } else {
                match self.extract_symbol(path, sym, &file_content) {
                    Some(content) => content,
                    None => return Ok(format!("> [unresolved: {}#{}]", path, sym)),
                }
            }
        } else {
            file_content.to_string()
        };

        let display_sha = sha.unwrap_or_else(|| head_sha.unwrap_or("HEAD"));

        let suffix = match symbol {
            Some(sym) if sym.bytes().all(|b| b.is_ascii_digit()) => format!(" (L{})", sym),
            Some(sym) => format!(" ({})", sym),
            None => String::new(),
        };

        let caption = format!("**{}** @ `{}`{}", path, display_sha, suffix);
        let lang = language_from_extension(path);

        let lines: Vec<&str> = content.lines().collect();
        let truncated = if lines.len() > self.max_lines {
            let remaining = lines.len() - self.max_lines;
            let comment = truncation_comment(lang, remaining);
            format!("{}\n{}", lines[..self.max_lines].join("\n"), comment)
        } else {
            content
        };

        Ok(format!("{}\n```{}\n{}\n```", caption, lang, truncated))
    }

    fn extract_symbol(&self, path: &str, symbol: &str, source: &str) -> Option<String> {
        let ext = Path::new(path).extension()?.to_str()?;
        match ext {
            "ts" | "tsx" => TypeScriptSymbolExtractor::new().extract(source, symbol),
            "rs" => RustSymbolExtractor::new().extract(source, symbol),
            _ => None,
        }
    }
}

fn truncation_comment(lang: &str, remaining: usize) -> String {
    match lang {
        "python" | "yaml" | "toml" => format!("# ... ({} more lines)", remaining),
        "markdown" => format!("<!-- ... ({} more lines) -->", remaining),
        _ => format!("// ... ({} more lines)", remaining),
    }
}

pub fn language_from_extension(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "ts" | "tsx" => "ts",
        "js" | "jsx" => "javascript",
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" => "cpp",
        "md" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        _ => "",
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
        // Should NOT contain the full file content
        assert!(
            !result.contains("[package]"),
            "Should not dump full file content for unknown symbol"
        );
    }

    #[test]
    fn test_line_number_ref_returns_single_line() {
        let expander = RefExpander::with_max_lines(std::env::current_dir().unwrap(), 9999);

        // Line 1 of Cargo.toml is [package]
        let content = "See: @ref Cargo.toml#1";
        let result = expander.expand(content).unwrap();
        assert!(result.contains("[package]"), "Should contain the first line");
        // The code block should only have one line of actual content
        // (i.e., it should NOT contain line 2+)
        let lines_in_block: Vec<&str> = result
            .lines()
            .skip_while(|l| !l.starts_with("```"))
            .skip(1) // skip opening ```
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
