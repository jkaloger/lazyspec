use lazyspec::tui::terminal_caps::TerminalImageProtocol;
use lazyspec::tui::diagram::{
    DiagramBlock, DiagramCache, DiagramCacheEntry, DiagramLanguage, PreviewSegment, ToolAvailability,
    build_preview_segments, extract_diagram_blocks, fallback_hint,
    is_tool_available, render_diagram, render_diagram_text, source_hash, tool_name,
};

#[test]
fn test_extract_d2_block() {
    let body = "# Title\n\n```d2\na -> b\nb -> c\n```\n\nSome text.\n";
    let blocks = extract_diagram_blocks(body);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].language, DiagramLanguage::D2);
    assert_eq!(blocks[0].source, "a -> b\nb -> c\n");
    assert_eq!(&body[blocks[0].byte_range.clone()], "```d2\na -> b\nb -> c\n```\n");
}

#[test]
fn test_extract_mermaid_block() {
    let body = "```mermaid\ngraph TD;\n  A-->B;\n```\n";
    let blocks = extract_diagram_blocks(body);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].language, DiagramLanguage::Mermaid);
    assert_eq!(blocks[0].source, "graph TD;\n  A-->B;\n");
    assert_eq!(&body[blocks[0].byte_range.clone()], "```mermaid\ngraph TD;\n  A-->B;\n```\n");
}

#[test]
fn test_extract_multiple_blocks() {
    let body = "```d2\nx -> y\n```\n\ntext\n\n```mermaid\nsequenceDiagram\n```\n";
    let blocks = extract_diagram_blocks(body);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].language, DiagramLanguage::D2);
    assert_eq!(blocks[1].language, DiagramLanguage::Mermaid);
}

#[test]
fn test_extract_no_diagram_blocks() {
    let body = "```rust\nfn main() {}\n```\n\n```json\n{}\n```\n";
    let blocks = extract_diagram_blocks(body);
    assert!(blocks.is_empty());
}

#[test]
fn test_extract_nested_backticks() {
    let body = "````\n```d2\na -> b\n```\n````\n";
    let blocks = extract_diagram_blocks(body);
    assert!(blocks.is_empty());
}

#[test]
fn test_tool_name_d2() {
    assert_eq!(tool_name(&DiagramLanguage::D2), "d2");
}

#[test]
fn test_tool_name_mermaid() {
    assert_eq!(tool_name(&DiagramLanguage::Mermaid), "mmdc");
}

fn make_d2_block() -> DiagramBlock {
    DiagramBlock {
        language: DiagramLanguage::D2,
        source: "a -> b\n".to_string(),
        byte_range: 0..16,
    }
}

#[test]
fn test_hint_tool_missing() {
    let block = make_d2_block();
    let result = fallback_hint(&block, false, TerminalImageProtocol::KittyGraphics);
    assert_eq!(result, Some("[d2: install d2 CLI for diagram rendering]".to_string()));
}

#[test]
fn test_hint_no_image_support() {
    let block = make_d2_block();
    let result = fallback_hint(&block, true, TerminalImageProtocol::Unsupported);
    assert_eq!(result, Some("[diagram: terminal does not support inline images]".to_string()));
}

#[test]
fn test_hint_both_missing() {
    let block = make_d2_block();
    let result = fallback_hint(&block, false, TerminalImageProtocol::Unsupported);
    assert_eq!(result, Some("[d2: install d2 CLI for diagram rendering]".to_string()));
}

#[test]
fn test_hint_all_available() {
    let block = make_d2_block();
    let result = fallback_hint(&block, true, TerminalImageProtocol::KittyGraphics);
    assert_eq!(result, None);
}

#[test]
fn test_tool_availability_is_available_d2() {
    let tools = ToolAvailability { d2: true, mmdc: false };
    assert!(tools.is_available(&DiagramLanguage::D2));
    assert!(!tools.is_available(&DiagramLanguage::Mermaid));
}

#[test]
fn test_tool_availability_is_available_mermaid() {
    let tools = ToolAvailability { d2: false, mmdc: true };
    assert!(!tools.is_available(&DiagramLanguage::D2));
    assert!(tools.is_available(&DiagramLanguage::Mermaid));
}

#[test]
fn test_source_hash_deterministic() {
    let a1 = source_hash("a -> b");
    let a2 = source_hash("a -> b");
    assert_eq!(a1, a2);

    let b = source_hash("x -> y");
    assert_ne!(a1, b);
}

#[test]
fn test_render_diagram_produces_png() {
    if !is_tool_available(&DiagramLanguage::D2) {
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let block = DiagramBlock {
        language: DiagramLanguage::D2,
        source: "a -> b\n".to_string(),
        byte_range: 0..16,
    };

    let path = render_diagram(&block, tmp.path()).unwrap();
    assert_eq!(path.extension().unwrap(), "png");
    assert!(path.exists());

    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
}

#[test]
fn test_render_diagram_text_produces_ascii() {
    if !is_tool_available(&DiagramLanguage::D2) {
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let block = DiagramBlock {
        language: DiagramLanguage::D2,
        source: "a -> b\n".to_string(),
        byte_range: 0..16,
    };

    let text = render_diagram_text(&block, tmp.path()).unwrap();
    assert!(!text.is_empty());
}

#[test]
fn test_render_diagram_text_mermaid_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let block = DiagramBlock {
        language: DiagramLanguage::Mermaid,
        source: "graph TD;\n  A-->B;\n".to_string(),
        byte_range: 0..30,
    };

    let result = render_diagram_text(&block, tmp.path());
    assert!(result.is_err());
}

#[test]
fn test_diagram_cache_insert_and_get() {
    let mut cache = DiagramCache::new();
    let hash = source_hash("a -> b\n");
    let path = std::path::PathBuf::from("/tmp/test.png");

    cache.insert(hash, DiagramCacheEntry::Image(path.clone()));

    match cache.get(hash) {
        Some(DiagramCacheEntry::Image(p)) => assert_eq!(p, &path),
        other => panic!("expected Image entry, got {:?}", option_entry_name(other)),
    }

    let other_hash = source_hash("x -> y\n");
    assert!(cache.get(other_hash).is_none());
}

#[test]
fn test_diagram_cache_mark_rendering() {
    let mut cache = DiagramCache::new();
    let hash = source_hash("a -> b\n");

    cache.mark_rendering(hash);
    assert!(matches!(cache.get(hash), Some(DiagramCacheEntry::Rendering)));

    let path = std::path::PathBuf::from("/tmp/test.png");
    cache.insert(hash, DiagramCacheEntry::Image(path.clone()));
    match cache.get(hash) {
        Some(DiagramCacheEntry::Image(p)) => assert_eq!(p, &path),
        other => panic!("expected Image entry, got {:?}", option_entry_name(other)),
    }
}

#[test]
fn test_diagram_cache_implicit_invalidation() {
    let mut cache = DiagramCache::new();
    let hash_a = source_hash("a -> b\n");
    let hash_b = source_hash("x -> y\n");

    cache.insert(hash_a, DiagramCacheEntry::Text("fallback".to_string()));

    assert!(cache.get(hash_a).is_some());
    assert!(cache.get(hash_b).is_none());
}

fn option_entry_name(entry: Option<&DiagramCacheEntry>) -> &'static str {
    match entry {
        None => "None",
        Some(DiagramCacheEntry::Rendering) => "Rendering",
        Some(DiagramCacheEntry::Image(_)) => "Image",
        Some(DiagramCacheEntry::Text(_)) => "Text",
        Some(DiagramCacheEntry::Failed(_)) => "Failed",
    }
}

#[test]
fn test_build_segments_no_diagrams() {
    let body = "# Hello\n\nSome plain text.\n";
    let cache = DiagramCache::new();
    let tools = ToolAvailability { d2: true, mmdc: true };

    let segments = build_preview_segments(body, &cache, TerminalImageProtocol::KittyGraphics, &tools);
    assert_eq!(segments.len(), 1);
    assert!(matches!(&segments[0], PreviewSegment::Markdown(t) if t == body));
}

#[test]
fn test_build_segments_with_cached_image() {
    let body = "# Title\n\n```d2\na -> b\n```\n\nMore text.\n";
    let blocks = extract_diagram_blocks(body);
    assert_eq!(blocks.len(), 1);

    let hash = source_hash(&blocks[0].source);
    let img_path = std::path::PathBuf::from("/tmp/test-diagram.png");

    let mut cache = DiagramCache::new();
    cache.insert(hash, DiagramCacheEntry::Image(img_path.clone()));
    let tools = ToolAvailability { d2: true, mmdc: true };

    let segments = build_preview_segments(body, &cache, TerminalImageProtocol::KittyGraphics, &tools);
    assert_eq!(segments.len(), 3);
    assert!(matches!(&segments[0], PreviewSegment::Markdown(_)));
    assert!(matches!(&segments[1], PreviewSegment::DiagramImage(p) if p == &img_path));
    assert!(matches!(&segments[2], PreviewSegment::Markdown(_)));
}

#[test]
fn test_build_segments_loading_when_uncached() {
    let body = "# Title\n\n```d2\na -> b\n```\n\nMore text.\n";
    let cache = DiagramCache::new();
    let tools = ToolAvailability { d2: true, mmdc: true };

    let segments = build_preview_segments(body, &cache, TerminalImageProtocol::KittyGraphics, &tools);
    assert_eq!(segments.len(), 3);
    assert!(matches!(&segments[0], PreviewSegment::Markdown(_)));
    assert!(matches!(&segments[1], PreviewSegment::DiagramLoading));
    assert!(matches!(&segments[2], PreviewSegment::Markdown(_)));
}

#[test]
fn test_build_segments_tool_unavailable_stays_markdown() {
    let body = "# Title\n\n```d2\na -> b\n```\n\nMore text.\n";
    let cache = DiagramCache::new();
    let tools = ToolAvailability { d2: false, mmdc: false };

    let segments = build_preview_segments(body, &cache, TerminalImageProtocol::KittyGraphics, &tools);
    assert_eq!(segments.len(), 1);
    assert!(matches!(&segments[0], PreviewSegment::Markdown(_)));
}

#[test]
fn test_build_segments_with_cached_text() {
    let body = "# Title\n\n```d2\na -> b\n```\n\nMore text.\n";
    let blocks = extract_diagram_blocks(body);
    let hash = source_hash(&blocks[0].source);

    let mut cache = DiagramCache::new();
    cache.insert(hash, DiagramCacheEntry::Text("  ┌───┐    ┌───┐\n  │ a │───>│ b │\n  └───┘    └───┘".to_string()));
    let tools = ToolAvailability { d2: true, mmdc: true };

    let segments = build_preview_segments(body, &cache, TerminalImageProtocol::KittyGraphics, &tools);
    assert_eq!(segments.len(), 3);
    assert!(matches!(&segments[0], PreviewSegment::Markdown(_)));
    assert!(matches!(&segments[1], PreviewSegment::DiagramText(t) if t.contains("┌───┐")));
    assert!(matches!(&segments[2], PreviewSegment::Markdown(_)));
}
