use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

use crate::tui::infra::terminal_caps::TerminalImageProtocol;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramLanguage {
    D2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramBlock {
    pub language: DiagramLanguage,
    pub source: String,
    pub byte_range: Range<usize>,
}

pub fn tool_name(lang: &DiagramLanguage) -> &'static str {
    match lang {
        DiagramLanguage::D2 => "d2",
    }
}

pub fn is_tool_available(lang: &DiagramLanguage) -> bool {
    std::process::Command::new(tool_name(lang))
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn extract_diagram_blocks(body: &str) -> Vec<DiagramBlock> {
    let mut blocks = Vec::new();
    let mut lines = body.split_inclusive('\n').peekable();
    let mut byte_offset = 0;

    while let Some(line) = lines.next() {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');

        // Skip 4+ backtick fences entirely
        if trimmed.starts_with("````") {
            byte_offset += line.len();
            // Consume until matching close fence
            for inner in lines.by_ref() {
                byte_offset += inner.len();
                let inner_trimmed = inner.trim_end_matches('\n').trim_end_matches('\r');
                if inner_trimmed.starts_with("````") {
                    break;
                }
            }
            continue;
        }

        let language = if trimmed == "```d2" {
            Some(DiagramLanguage::D2)
        } else {
            None
        };

        if let Some(lang) = language {
            let block_start = byte_offset;
            byte_offset += line.len();
            let mut source = String::new();

            for inner in lines.by_ref() {
                let inner_trimmed = inner.trim_end_matches('\n').trim_end_matches('\r');
                byte_offset += inner.len();
                if inner_trimmed == "```" {
                    break;
                }
                source.push_str(inner);
            }

            blocks.push(DiagramBlock {
                language: lang,
                source,
                byte_range: block_start..byte_offset,
            });
        } else {
            byte_offset += line.len();
        }
    }

    blocks
}

pub fn fallback_hint(
    block: &DiagramBlock,
    tool_available: bool,
    protocol: TerminalImageProtocol,
) -> Option<String> {
    if !tool_available {
        let name = tool_name(&block.language);
        return Some(format!(
            "[{}: install {} CLI for diagram rendering]",
            name, name
        ));
    }
    if protocol == TerminalImageProtocol::Unsupported {
        return Some("[diagram: terminal does not support inline images]".to_string());
    }
    None
}

pub struct ToolAvailability {
    pub d2: bool,
}

impl ToolAvailability {
    pub fn detect() -> Self {
        ToolAvailability {
            d2: is_tool_available(&DiagramLanguage::D2),
        }
    }

    pub fn is_available(&self, lang: &DiagramLanguage) -> bool {
        match lang {
            DiagramLanguage::D2 => self.d2,
        }
    }
}

pub fn source_hash(source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

pub fn source_hash_path(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

pub fn render_diagram(block: &DiagramBlock, output_dir: &Path) -> Result<PathBuf> {
    let hash = source_hash(&block.source);
    let output_path = output_dir.join(format!("{:016x}.png", hash));

    if output_path.exists() {
        return Ok(output_path);
    }

    let (input_path, args) = match block.language {
        DiagramLanguage::D2 => {
            let input_path = output_dir.join(format!("{:016x}.d2", hash));
            fs::write(&input_path, &block.source)?;
            let args = vec![
                "--scale".to_string(),
                "2".to_string(),
                input_path.display().to_string(),
                output_path.display().to_string(),
            ];
            (input_path, args)
        }
    };

    let result = Command::new("d2").args(&args).output();

    let _ = fs::remove_file(&input_path);

    let output = result?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("d2 failed (exit {}): {}", output.status, stderr.trim());
    }

    Ok(output_path)
}

pub fn render_diagram_text(block: &DiagramBlock, output_dir: &Path) -> Result<String> {
    let hash = source_hash(&block.source);
    let txt_path = output_dir.join(format!("{:016x}.txt", hash));
    let input_path = output_dir.join(format!("{:016x}.d2", hash));

    fs::write(&input_path, &block.source)?;

    let result = Command::new("d2")
        .arg(input_path.display().to_string())
        .arg(txt_path.display().to_string())
        .output();

    let _ = fs::remove_file(&input_path);

    let output = result?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("d2 failed (exit {}): {}", output.status, stderr.trim());
    }

    let text = fs::read_to_string(&txt_path)?;
    Ok(text)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PreviewSegment {
    Markdown(String),
    DiagramImage(PathBuf),
    DiagramText(String),
    DiagramLoading,
    DiagramError(String),
}

fn should_render_block(
    block: &DiagramBlock,
    tools: &ToolAvailability,
    _protocol: TerminalImageProtocol,
) -> bool {
    tools.is_available(&block.language)
}

pub fn build_preview_segments(
    body: &str,
    cache: &DiagramCache,
    protocol: TerminalImageProtocol,
    tools: &ToolAvailability,
    blocks: &[DiagramBlock],
) -> Vec<PreviewSegment> {
    if blocks.is_empty() {
        return vec![PreviewSegment::Markdown(body.to_string())];
    }

    let renderable: Vec<&DiagramBlock> = blocks
        .iter()
        .filter(|b| should_render_block(b, tools, protocol))
        .collect();

    if renderable.is_empty() {
        let hinted = inject_fallback_hints(body, protocol, tools, blocks);
        return vec![PreviewSegment::Markdown(hinted)];
    }

    let mut segments = Vec::new();
    let mut cursor = 0;

    for block in &renderable {
        if block.byte_range.start > cursor {
            let text = &body[cursor..block.byte_range.start];
            let sub_blocks = extract_diagram_blocks(text);
            let hinted = inject_fallback_hints(text, protocol, tools, &sub_blocks);
            if !hinted.is_empty() {
                segments.push(PreviewSegment::Markdown(hinted));
            }
        }

        let hash = source_hash(&block.source);
        let segment = match cache.get(hash) {
            Some(DiagramCacheEntry::Image(path)) => PreviewSegment::DiagramImage(path.clone()),
            Some(DiagramCacheEntry::Text(text)) => PreviewSegment::DiagramText(text.clone()),
            Some(DiagramCacheEntry::Failed(msg)) => PreviewSegment::DiagramError(msg.clone()),
            Some(DiagramCacheEntry::Rendering) | None => PreviewSegment::DiagramLoading,
        };
        segments.push(segment);

        cursor = block.byte_range.end;
    }

    if cursor < body.len() {
        let text = &body[cursor..];
        let sub_blocks = extract_diagram_blocks(text);
        let hinted = inject_fallback_hints(text, protocol, tools, &sub_blocks);
        if !hinted.is_empty() {
            segments.push(PreviewSegment::Markdown(hinted));
        }
    }

    segments
}

pub fn inject_fallback_hints(
    body: &str,
    protocol: TerminalImageProtocol,
    tools: &ToolAvailability,
    blocks: &[DiagramBlock],
) -> String {
    if blocks.is_empty() {
        return body.to_string();
    }

    let mut result = body.to_string();

    for block in blocks.iter().rev() {
        let tool_available = tools.is_available(&block.language);
        if let Some(hint) = fallback_hint(block, tool_available, protocol) {
            let insert_pos = block.byte_range.end;
            result.insert_str(insert_pos, &format!("{}\n", hint));
        }
    }

    result
}

pub enum DiagramCacheEntry {
    Rendering,
    Image(PathBuf),
    Text(String),
    Failed(String),
}

pub struct DiagramCache {
    cache_dir: PathBuf,
    entries: HashMap<u64, DiagramCacheEntry>,
}

impl Default for DiagramCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramCache {
    pub fn new() -> Self {
        let cache_dir = std::env::temp_dir().join("lazyspec-diagrams");
        let _ = fs::create_dir_all(&cache_dir);
        DiagramCache {
            cache_dir,
            entries: HashMap::new(),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn get(&self, source_hash: u64) -> Option<&DiagramCacheEntry> {
        self.entries.get(&source_hash)
    }

    pub fn insert(&mut self, source_hash: u64, entry: DiagramCacheEntry) {
        self.entries.insert(source_hash, entry);
    }

    pub fn mark_rendering(&mut self, source_hash: u64) {
        self.entries
            .insert(source_hash, DiagramCacheEntry::Rendering);
    }
}
