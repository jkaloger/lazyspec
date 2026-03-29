use crate::engine::symbols::{RustSymbolExtractor, SymbolExtractor, TypeScriptSymbolExtractor};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

use super::RefExpander;

impl RefExpander {
    pub(super) fn resolve_head_short_sha(&self) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&self.root)
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    pub(super) fn resolve_ref(
        &self,
        path: &str,
        symbol: Option<&str>,
        sha: Option<&str>,
        head_sha: Option<&str>,
    ) -> Result<String> {
        let rev = sha.unwrap_or("HEAD");
        let output = Command::new("git")
            .args(["show", &format!("{}:{}", rev, path)])
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
