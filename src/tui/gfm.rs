use pulldown_cmark::{Alignment, BlockQuoteKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, PartialEq)]
pub struct GfmTable {
    pub headers: Vec<String>,
    pub alignments: Vec<Alignment>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GfmSegment {
    Markdown(String),
    Table(GfmTable),
    Admonition { kind: String, body: String },
    Footnote { label: String, body: String },
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

pub fn extract_gfm_segments(body: &str) -> Vec<GfmSegment> {
    let parser = Parser::new_ext(body, parser_options());
    let offset_iter = parser.into_offset_iter();

    // Segments paired with their byte ranges for gap calculation
    let mut ranged_segments: Vec<(std::ops::Range<usize>, GfmSegment)> = Vec::new();
    let mut footnotes: Vec<GfmSegment> = Vec::new();
    // Footnote byte ranges tracked separately for gap exclusion only
    let mut footnote_ranges: Vec<std::ops::Range<usize>> = Vec::new();

    // Stateful parsing for tables
    let mut in_table = false;
    let mut table_alignments: Vec<Alignment> = Vec::new();
    let mut table_headers: Vec<String> = Vec::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut cell_text = String::new();
    let mut in_table_head = false;
    let mut table_start_offset: usize = 0;

    // Stateful parsing for admonitions
    let mut admonition_kind: Option<String> = None;
    let mut admonition_body = String::new();
    let mut admonition_depth: usize = 0;
    let mut admonition_start_offset: usize = 0;

    // Stateful parsing for footnotes
    let mut in_footnote = false;
    let mut footnote_label = String::new();
    let mut footnote_body = String::new();
    let mut footnote_start_offset: usize = 0;

    for (event, range) in offset_iter {
        // Handle footnote definitions
        if in_footnote {
            match &event {
                Event::End(TagEnd::FootnoteDefinition) => {
                    footnote_ranges.push(footnote_start_offset..range.end);
                    footnotes.push(GfmSegment::Footnote {
                        label: footnote_label.clone(),
                        body: footnote_body.trim().to_string(),
                    });
                    in_footnote = false;
                    footnote_label.clear();
                    footnote_body.clear();
                    continue;
                }
                Event::Text(t) | Event::Code(t) => {
                    footnote_body.push_str(t);
                }
                Event::SoftBreak | Event::HardBreak => {
                    footnote_body.push('\n');
                }
                _ => {}
            }
            continue;
        }

        // Handle admonitions (blockquotes with a kind)
        if admonition_kind.is_some() {
            match &event {
                Event::Start(Tag::BlockQuote(_)) => {
                    admonition_depth += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    if admonition_depth == 0 {
                        let seg_range = admonition_start_offset..range.end;
                        ranged_segments.push((seg_range, GfmSegment::Admonition {
                            kind: admonition_kind.take().unwrap(),
                            body: admonition_body.trim().to_string(),
                        }));
                        admonition_body.clear();
                        continue;
                    }
                    admonition_depth -= 1;
                }
                Event::Text(t) | Event::Code(t) => {
                    admonition_body.push_str(t);
                }
                Event::SoftBreak | Event::HardBreak => {
                    admonition_body.push('\n');
                }
                _ => {}
            }
            continue;
        }

        // Handle tables
        if in_table {
            match &event {
                Event::Start(Tag::TableHead) => {
                    in_table_head = true;
                }
                Event::End(TagEnd::TableHead) => {
                    in_table_head = false;
                }
                Event::Start(Tag::TableRow) => {
                    current_row.clear();
                }
                Event::End(TagEnd::TableRow) => {
                    table_rows.push(current_row.clone());
                    current_row.clear();
                }
                Event::Start(Tag::TableCell) => {
                    cell_text.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    if in_table_head {
                        table_headers.push(cell_text.clone());
                    } else {
                        current_row.push(cell_text.clone());
                    }
                    cell_text.clear();
                }
                Event::Text(t) | Event::Code(t) => {
                    cell_text.push_str(t);
                }
                Event::SoftBreak | Event::HardBreak => {
                    cell_text.push(' ');
                }
                Event::End(TagEnd::Table) => {
                    let seg_range = table_start_offset..range.end;
                    ranged_segments.push((seg_range, GfmSegment::Table(GfmTable {
                        headers: table_headers.clone(),
                        alignments: table_alignments.clone(),
                        rows: table_rows.clone(),
                    })));
                    in_table = false;
                    table_headers.clear();
                    table_alignments.clear();
                    table_rows.clear();
                    current_row.clear();
                }
                _ => {}
            }
            continue;
        }

        // Detect start of GFM elements
        match &event {
            Event::Start(Tag::Table(aligns)) => {
                in_table = true;
                table_alignments = aligns.clone();
                table_start_offset = range.start;
            }
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let kind_str = match kind {
                    BlockQuoteKind::Note => "Note",
                    BlockQuoteKind::Warning => "Warning",
                    BlockQuoteKind::Tip => "Tip",
                    BlockQuoteKind::Important => "Important",
                    BlockQuoteKind::Caution => "Caution",
                };
                admonition_kind = Some(kind_str.to_string());
                admonition_depth = 0;
                admonition_start_offset = range.start;
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                in_footnote = true;
                footnote_label = label.to_string();
                footnote_start_offset = range.start;
            }
            _ => {}
        }
    }

    // Merge segment ranges and footnote ranges, sorted by start offset.
    // Segment ranges carry their GfmSegment; footnote ranges are gaps only.
    ranged_segments.sort_by_key(|(r, _)| r.start);
    footnote_ranges.sort_by_key(|r| r.start);

    // Build a combined list of all excluded ranges for gap calculation
    let mut all_ranges: Vec<std::ops::Range<usize>> = ranged_segments
        .iter()
        .map(|(r, _)| r.clone())
        .chain(footnote_ranges.iter().cloned())
        .collect();
    all_ranges.sort_by_key(|r| r.start);

    let mut result: Vec<GfmSegment> = Vec::new();
    let mut cursor = 0;
    let mut seg_idx = 0;

    for r in &all_ranges {
        // Emit any markdown text before this GFM element
        if cursor < r.start {
            let md = body[cursor..r.start].trim();
            if !md.is_empty() {
                result.push(GfmSegment::Markdown(md.to_string()));
            }
        }
        // If this range corresponds to a non-footnote segment, emit it
        if seg_idx < ranged_segments.len() && ranged_segments[seg_idx].0 == *r {
            result.push(ranged_segments[seg_idx].1.clone());
            seg_idx += 1;
        }
        cursor = r.end;
    }

    // Emit trailing markdown
    if cursor < body.len() {
        let md = body[cursor..].trim();
        if !md.is_empty() {
            result.push(GfmSegment::Markdown(md.to_string()));
        }
    }

    // Append footnotes at the end
    result.extend(footnotes);

    // If nothing was extracted and input is non-empty, return a single Markdown segment
    if result.is_empty() && !body.trim().is_empty() {
        result.push(GfmSegment::Markdown(body.trim().to_string()));
    }

    result
}

fn admonition_color(kind: &str) -> Color {
    match kind.to_lowercase().as_str() {
        "note" => Color::Blue,
        "tip" => Color::Green,
        "important" => Color::Magenta,
        "warning" => Color::Yellow,
        "caution" => Color::Red,
        _ => Color::White,
    }
}

fn align_text(text: &str, width: usize, alignment: &Alignment) -> String {
    let text_len = text.len();
    if text_len >= width {
        return text[..width].to_string();
    }
    let padding = width - text_len;
    match alignment {
        Alignment::Right => format!("{}{}", " ".repeat(padding), text),
        Alignment::Center => {
            let left = padding / 2;
            let right = padding - left;
            format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
        }
        _ => format!("{}{}", text, " ".repeat(padding)),
    }
}

pub fn render_table(table: &GfmTable, max_width: u16) -> Vec<Line<'static>> {
    let max_width = max_width as usize;
    let col_count = table.headers.len();
    if col_count == 0 {
        return vec![];
    }

    // Calculate column widths based on content
    let mut col_widths: Vec<usize> = table.headers.iter().map(|h| h.len()).collect();
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }
    }

    // Constrain total width: separators take (col_count - 1) * 3 chars for " │ "
    let separator_width = if col_count > 1 {
        (col_count - 1) * 3
    } else {
        0
    };
    let available = max_width.saturating_sub(separator_width);
    let total_content: usize = col_widths.iter().sum();
    if total_content > available && available > 0 {
        let scale = available as f64 / total_content as f64;
        for w in &mut col_widths {
            *w = (*w as f64 * scale).max(1.0) as usize;
        }
    }

    let alignments = &table.alignments;
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Header row (bold)
    let header_spans: Vec<Span<'static>> = table
        .headers
        .iter()
        .enumerate()
        .flat_map(|(i, h)| {
            let alignment = alignments.get(i).unwrap_or(&Alignment::None);
            let width = col_widths.get(i).copied().unwrap_or(h.len());
            let text = align_text(h, width, alignment);
            let mut spans = vec![Span::styled(
                text,
                Style::default().add_modifier(Modifier::BOLD),
            )];
            if i < col_count - 1 {
                spans.push(Span::raw(" │ ".to_string()));
            }
            spans
        })
        .collect();
    lines.push(Line::from(header_spans));

    // Separator row
    let sep_spans: Vec<Span<'static>> = col_widths
        .iter()
        .enumerate()
        .flat_map(|(i, &w)| {
            let mut spans = vec![Span::raw("─".repeat(w))];
            if i < col_count - 1 {
                spans.push(Span::raw("─┼─".to_string()));
            }
            spans
        })
        .collect();
    lines.push(Line::from(sep_spans));

    // Data rows
    for row in &table.rows {
        let row_spans: Vec<Span<'static>> = row
            .iter()
            .enumerate()
            .flat_map(|(i, cell)| {
                let alignment = alignments.get(i).unwrap_or(&Alignment::None);
                let width = col_widths.get(i).copied().unwrap_or(cell.len());
                let text = align_text(cell, width, alignment);
                let mut spans = vec![Span::raw(text)];
                if i < col_count - 1 {
                    spans.push(Span::raw(" │ ".to_string()));
                }
                spans
            })
            .collect();
        lines.push(Line::from(row_spans));
    }

    lines
}

pub fn render_admonition(kind: &str, body: &str) -> Vec<Line<'static>> {
    let color = admonition_color(kind);
    let label = kind.to_uppercase();
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        label,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )));

    for line in body.lines() {
        lines.push(Line::from(vec![
            Span::styled("▌ ".to_string(), Style::default().fg(color)),
            Span::raw(line.to_string()),
        ]));
    }

    lines
}

pub fn render_footnotes(footnotes: &[(String, String)]) -> Vec<Line<'static>> {
    if footnotes.is_empty() {
        return vec![];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::raw("───────────────".to_string())));

    for (label, definition) in footnotes {
        lines.push(Line::from(vec![
            Span::styled(
                format!("[^{}]: ", label),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(definition.clone()),
        ]));
    }

    lines
}

pub fn render_gfm_segments(segments: &[GfmSegment], max_width: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut collected_footnotes: Vec<(String, String)> = Vec::new();

    for segment in segments {
        match segment {
            GfmSegment::Markdown(text) => {
                let md = tui_markdown::from_str(text);
                for line in md.lines {
                    let owned_spans: Vec<Span<'static>> = line
                        .spans
                        .into_iter()
                        .map(|s| Span::styled(s.content.to_string(), s.style))
                        .collect();
                    lines.push(Line::from(owned_spans));
                }
            }
            GfmSegment::Table(table) => {
                lines.push(Line::default());
                lines.extend(render_table(table, max_width));
                lines.push(Line::default());
            }
            GfmSegment::Admonition { kind, body } => {
                lines.push(Line::default());
                lines.extend(render_admonition(kind, body));
                lines.push(Line::default());
            }
            GfmSegment::Footnote { label, body } => {
                collected_footnotes.push((label.clone(), body.clone()));
            }
        }
    }

    if !collected_footnotes.is_empty() {
        lines.push(Line::default());
        lines.extend(render_footnotes(&collected_footnotes));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Alignment;

    #[test]
    fn test_extract_plain_markdown() {
        let input = "# Hello\n\nSome paragraph with **bold** text.\n";
        let segments = extract_gfm_segments(input);
        assert_eq!(segments.len(), 1);
        match &segments[0] {
            GfmSegment::Markdown(md) => {
                assert!(md.contains("Hello"));
                assert!(md.contains("bold"));
            }
            other => panic!("expected Markdown segment, got {:?}", other),
        }
    }

    #[test]
    fn test_extract_table() {
        let input = "| Name | Age |\n| :--- | ---: |\n| Alice | 30 |\n| Bob | 25 |\n";
        let segments = extract_gfm_segments(input);

        let table = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Table(t) => Some(t),
                _ => None,
            })
            .expect("should contain a Table segment");

        assert_eq!(table.headers, vec!["Name", "Age"]);
        assert_eq!(table.alignments, vec![Alignment::Left, Alignment::Right]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0], vec!["Alice", "30"]);
        assert_eq!(table.rows[1], vec!["Bob", "25"]);
    }

    #[test]
    fn test_extract_admonition() {
        let input = "> [!NOTE]\n> This is a note body.\n";
        let segments = extract_gfm_segments(input);

        let admonition = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Admonition { kind, body } => Some((kind.clone(), body.clone())),
                _ => None,
            })
            .expect("should contain an Admonition segment");

        assert_eq!(admonition.0, "Note");
        assert!(admonition.1.contains("note body"));
    }

    #[test]
    fn test_extract_footnotes() {
        let input = "Text with a reference[^1].\n\n[^1]: This is the footnote definition.\n";
        let segments = extract_gfm_segments(input);

        let footnote = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Footnote { label, body } => Some((label.clone(), body.clone())),
                _ => None,
            })
            .expect("should contain a Footnote segment");

        assert_eq!(footnote.0, "1");
        assert!(footnote.1.contains("footnote definition"));

        // Verify footnote definition text does NOT leak into Markdown segments
        for seg in &segments {
            if let GfmSegment::Markdown(md) = seg {
                assert!(
                    !md.contains("footnote definition"),
                    "footnote definition leaked into Markdown segment: {md}"
                );
            }
        }
    }

    #[test]
    fn test_extract_mixed() {
        let input = "\
# Title

Some intro text.

| Col A | Col B |
| ----- | ----- |
| 1     | 2     |

> [!WARNING]
> Be careful here.

More text at the end.
";
        let segments = extract_gfm_segments(input);

        // Verify we have the right types in order
        let types: Vec<&str> = segments
            .iter()
            .map(|s| match s {
                GfmSegment::Markdown(_) => "Markdown",
                GfmSegment::Table(_) => "Table",
                GfmSegment::Admonition { .. } => "Admonition",
                GfmSegment::Footnote { .. } => "Footnote",
            })
            .collect();

        assert!(types.contains(&"Markdown"), "should have Markdown segments");
        assert!(types.contains(&"Table"), "should have a Table segment");
        assert!(
            types.contains(&"Admonition"),
            "should have an Admonition segment"
        );

        // Table should come before the admonition
        let table_pos = types.iter().position(|&t| t == "Table").unwrap();
        let adm_pos = types.iter().position(|&t| t == "Admonition").unwrap();
        assert!(table_pos < adm_pos, "Table should appear before Admonition");

        // Should have markdown both before and after the GFM elements
        assert_eq!(types.first(), Some(&"Markdown"));
        assert_eq!(types.last(), Some(&"Markdown"));
    }

    #[test]
    fn test_render_table_alignment() {
        let table = GfmTable {
            headers: vec!["Left".into(), "Center".into(), "Right".into()],
            alignments: vec![Alignment::Left, Alignment::Center, Alignment::Right],
            rows: vec![vec!["a".into(), "b".into(), "c".into()]],
        };

        let lines = render_table(&table, 80);
        assert!(lines.len() >= 3, "should have header, separator, and data row");

        // Header line: check bold modifier on first span
        let header = &lines[0];
        assert!(
            header.spans[0].style.add_modifier.contains(Modifier::BOLD),
            "header cells should be bold"
        );

        // Separator line: should contain ─
        let sep_text: String = lines[1].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(sep_text.contains('─'), "separator should use ─ character");
        assert!(sep_text.contains('┼'), "separator should use ┼ at column junctions");

        // Data row: check column separators present
        let data_text: String = lines[2].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(data_text.contains('│'), "data rows should have │ separators");

        // Verify right-alignment: "c" should be right-padded (leading spaces)
        let right_col_span = &lines[2].spans[4]; // after "a", " │ ", "b", " │ "
        let right_text = right_col_span.content.to_string();
        assert!(
            right_text.ends_with('c'),
            "right-aligned column should have text at the end, got: {:?}",
            right_text
        );
    }

    #[test]
    fn test_render_admonition_kinds() {
        let kinds = vec![
            ("Note", "NOTE"),
            ("Warning", "WARNING"),
            ("Tip", "TIP"),
            ("Important", "IMPORTANT"),
            ("Caution", "CAUTION"),
        ];

        for (kind, expected_label) in kinds {
            let lines = render_admonition(kind, "some body text");
            assert!(!lines.is_empty(), "admonition should produce lines");

            let label_text: String = lines[0].spans.iter().map(|s| s.content.to_string()).collect();
            assert_eq!(
                label_text, expected_label,
                "label for kind {:?} should be {:?}",
                kind, expected_label
            );

            // Body lines should have the colored border prefix
            assert!(lines.len() >= 2, "should have label + body lines");
            let body_text: String = lines[1].spans.iter().map(|s| s.content.to_string()).collect();
            assert!(
                body_text.contains("some body text"),
                "body should contain the text"
            );
        }
    }

    #[test]
    fn test_render_footnotes() {
        let footnotes = vec![
            ("1".to_string(), "First footnote.".to_string()),
            ("abc".to_string(), "Another footnote.".to_string()),
        ];

        let lines = render_footnotes(&footnotes);

        // First line is separator
        let sep_text: String = lines[0].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(sep_text.contains('─'), "should have a separator line");

        // Check footnote lines
        let fn1_text: String = lines[1].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(
            fn1_text.contains("[^1]:"),
            "should contain [^1]: prefix, got: {:?}",
            fn1_text
        );
        assert!(fn1_text.contains("First footnote."));

        let fn2_text: String = lines[2].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(fn2_text.contains("[^abc]:"));
        assert!(fn2_text.contains("Another footnote."));
    }
}
