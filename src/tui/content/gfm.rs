mod parse;
mod render;

use pulldown_cmark::Alignment;

pub use parse::extract_gfm_segments;
pub use render::{render_admonition, render_footnotes, render_gfm_segments, render_table};

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

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Alignment;
    use ratatui::style::Modifier;

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
    fn admonition_soft_break_renders_as_space() {
        let input = "> [!NOTE]\n> first\n> second";
        let segments = extract_gfm_segments(input);

        let body = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Admonition { body, .. } => Some(body.clone()),
                _ => None,
            })
            .expect("should contain an Admonition segment");

        assert_eq!(body, "first second");
    }

    #[test]
    fn admonition_hard_break_renders_as_newline() {
        let input = "> [!NOTE]\n> first  \n> second";
        let segments = extract_gfm_segments(input);

        let body = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Admonition { body, .. } => Some(body.clone()),
                _ => None,
            })
            .expect("should contain an Admonition segment");

        assert_eq!(body, "first\nsecond");
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

        let table_pos = types.iter().position(|&t| t == "Table").unwrap();
        let adm_pos = types.iter().position(|&t| t == "Admonition").unwrap();
        assert!(table_pos < adm_pos, "Table should appear before Admonition");

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
        assert!(
            lines.len() >= 3,
            "should have header, separator, and data row"
        );

        let header = &lines[0];
        assert!(
            header.spans[0].style.add_modifier.contains(Modifier::BOLD),
            "header cells should be bold"
        );

        let sep_text: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(sep_text.contains('─'), "separator should use ─ character");
        assert!(
            sep_text.contains('┼'),
            "separator should use ┼ at column junctions"
        );

        let data_text: String = lines[2]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            data_text.contains('│'),
            "data rows should have │ separators"
        );

        let right_col_span = &lines[2].spans[4];
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

            let label_text: String = lines[0]
                .spans
                .iter()
                .map(|s| s.content.to_string())
                .collect();
            assert_eq!(
                label_text, expected_label,
                "label for kind {:?} should be {:?}",
                kind, expected_label
            );

            assert!(lines.len() >= 2, "should have label + body lines");
            let body_text: String = lines[1]
                .spans
                .iter()
                .map(|s| s.content.to_string())
                .collect();
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

        let sep_text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(sep_text.contains('─'), "should have a separator line");

        let fn1_text: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            fn1_text.contains("[^1]:"),
            "should contain [^1]: prefix, got: {:?}",
            fn1_text
        );
        assert!(fn1_text.contains("First footnote."));

        let fn2_text: String = lines[2]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(fn2_text.contains("[^abc]:"));
        assert!(fn2_text.contains("Another footnote."));
    }

    #[test]
    fn tui_markdown_preserves_hard_break() {
        let md = tui_markdown::from_str("foo  \nbar\n");
        assert!(
            md.lines.len() >= 2,
            "expected ≥2 lines from hard break, got {}: {:?}",
            md.lines.len(),
            md.lines
                .iter()
                .map(|l| l
                    .spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>())
                .collect::<Vec<_>>()
        );

        let line_texts: Vec<String> = md
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect();
        assert!(
            line_texts.iter().any(|t| t.contains("foo")),
            "expected a line containing 'foo', got {:?}",
            line_texts
        );
        assert!(
            line_texts.iter().any(|t| t.contains("bar")),
            "expected a line containing 'bar', got {:?}",
            line_texts
        );

        let segments = extract_gfm_segments("foo  \nbar");
        let rendered = render_gfm_segments(&segments, 80);
        assert!(
            rendered.len() >= 2,
            "expected render_gfm_segments to produce ≥2 lines, got {}",
            rendered.len()
        );
    }

    #[test]
    fn test_line_level_styles_preserved() {
        let input = "# Heading 1\n\n## Heading 2\n\nBody text.\n";

        let direct = tui_markdown::from_str(input);
        let segments = extract_gfm_segments(input);
        let gfm_lines = render_gfm_segments(&segments, 80);

        assert_eq!(direct.lines.len(), gfm_lines.len(), "line count mismatch");

        for (i, (d, g)) in direct.lines.iter().zip(gfm_lines.iter()).enumerate() {
            assert_eq!(
                d.style, g.style,
                "line-level style mismatch at line {i}: direct={:?} gfm={:?}",
                d.style, g.style
            );
        }

        let h1_style = gfm_lines[0].style;
        assert!(
            h1_style.add_modifier.contains(Modifier::BOLD),
            "H1 should be bold, got {:?}",
            h1_style
        );
    }

    #[test]
    fn code_block_preserves_newlines() {
        let input = "```\nfn a()\nfn b()\n```";
        let segments = extract_gfm_segments(input);
        let lines = render_gfm_segments(&segments, 80);

        let line_texts: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect();

        assert!(
            line_texts.iter().any(|t| t.contains("fn a()")),
            "expected a line containing 'fn a()', got {:?}",
            line_texts
        );
        assert!(
            line_texts.iter().any(|t| t.contains("fn b()")),
            "expected a line containing 'fn b()', got {:?}",
            line_texts
        );
        assert!(
            !line_texts.iter().any(|t| t.contains("fn a()") && t.contains("fn b()")),
            "code block lines should be separate, got {:?}",
            line_texts
        );
    }

    #[test]
    fn mixed_paragraphs_and_hard_breaks() {
        let input = "para1 line  \npara1 line2\n\npara2";
        let segments = extract_gfm_segments(input);
        let lines = render_gfm_segments(&segments, 80);

        let line_texts: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect();

        let non_empty: Vec<&String> = line_texts.iter().filter(|t| !t.trim().is_empty()).collect();
        assert!(
            non_empty.len() >= 3,
            "expected ≥3 non-empty lines, got {}: {:?}",
            non_empty.len(),
            line_texts
        );

        let para1_line_idx = line_texts
            .iter()
            .position(|t| t.contains("para1 line2"))
            .expect("expected a line containing 'para1 line2'");
        let para2_idx = line_texts
            .iter()
            .position(|t| t.contains("para2"))
            .expect("expected a line containing 'para2'");
        assert!(
            line_texts[para1_line_idx + 1..para2_idx]
                .iter()
                .any(|t| t.trim().is_empty()),
            "expected blank line between paragraphs, got {:?}",
            line_texts
        );
    }

    #[test]
    fn hard_break_survives_long_lines() {
        let max_width = 20;
        let long_first =
            "this line is definitely longer than twenty characters wide and keeps going";
        let input = format!("{long_first}  \nsecond");
        let segments = extract_gfm_segments(&input);
        let lines = render_gfm_segments(&segments, max_width);

        // Note: render_gfm_segments emits one Line per source line (split on hard
        // breaks). Line widths may exceed max_width — `Paragraph::wrap` operates
        // within a Line at render time and never merges across Lines, so asserting
        // Line boundary count here is sufficient to verify hard-break preservation.
        assert!(
            lines.len() >= 2,
            "expected ≥2 Lines from hard break, got {}",
            lines.len()
        );

        let line_texts: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect();

        assert!(
            line_texts.iter().any(|t| t.contains("second")),
            "expected a Line containing 'second', got {:?}",
            line_texts
        );
    }
}
