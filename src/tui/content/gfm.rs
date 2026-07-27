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
    use ratatui::text::Line;
    use unicode_width::UnicodeWidthStr;

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
    fn table_extractor_soft_break_renders_as_space() {
        // Plain single-line table cell — pulldown-cmark emits a single
        // Text event, no SoftBreak inside cells. Asserts that ordinary
        // cells contain no '\n' so the HardBreak split doesn't accidentally
        // inject newlines.
        let input = "| col |\n|---|\n| plain text here |\n";
        let segments = extract_gfm_segments(input);

        let table = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Table(t) => Some(t),
                _ => None,
            })
            .expect("should contain a Table segment");

        let cell = &table.rows[0][0];
        assert!(
            !cell.contains('\n'),
            "plain cell should not contain '\\n', got {:?}",
            cell
        );
        assert_eq!(cell, "plain text here");
    }

    #[test]
    fn table_extractor_br_tag_emits_newline() {
        let input = "| col |\n|---|\n| first<br>second |";
        let segments = extract_gfm_segments(input);

        let table = segments
            .iter()
            .find_map(|s| match s {
                GfmSegment::Table(t) => Some(t),
                _ => None,
            })
            .expect("should contain a Table segment");

        let cell = &table.rows[0][0];
        assert!(
            cell.contains('\n'),
            "cell should contain '\\n' between 'first' and 'second', got {:?}",
            cell
        );
        assert_eq!(cell, "first\nsecond");
    }

    #[test]
    fn table_extractor_br_tag_case_insensitive() {
        let variants = ["<BR>", "<br/>", "<br />"];
        for variant in variants {
            let input = format!("| col |\n|---|\n| first{variant}second |");
            let segments = extract_gfm_segments(&input);

            let table = segments
                .iter()
                .find_map(|s| match s {
                    GfmSegment::Table(t) => Some(t),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("should contain a Table segment for variant {variant}"));

            let cell = &table.rows[0][0];
            assert_eq!(
                cell, "first\nsecond",
                "variant {variant} should produce '\\n' in cell, got {:?}",
                cell
            );
        }
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
            !line_texts
                .iter()
                .any(|t| t.contains("fn a()") && t.contains("fn b()")),
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

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>()
    }

    #[test]
    fn table_cell_hard_break_splits_lines() {
        let table = GfmTable {
            headers: vec!["h".into()],
            alignments: vec![Alignment::None],
            rows: vec![vec!["line1\nline2".into()]],
        };

        let lines = render_table(&table, 80);
        assert_eq!(
            lines.len(),
            4,
            "expected header + sep + 2 row visual lines, got {}",
            lines.len()
        );

        let row_line_1 = line_text(&lines[2]);
        let row_line_2 = line_text(&lines[3]);
        assert!(
            row_line_1.contains("line1"),
            "row visual line 1 should contain 'line1', got {:?}",
            row_line_1
        );
        assert!(
            row_line_2.contains("line2"),
            "row visual line 2 should contain 'line2', got {:?}",
            row_line_2
        );
    }

    #[test]
    fn table_cell_soft_wrap_word_boundary() {
        let table = GfmTable {
            headers: vec!["h".into()],
            alignments: vec![Alignment::None],
            rows: vec![vec!["the quick brown fox".into()]],
        };

        let max_width: u16 = 10;
        let lines = render_table(&table, max_width);

        for (i, line) in lines.iter().enumerate().skip(2) {
            let text_len = line_text(line).chars().count();
            assert!(
                text_len <= max_width as usize,
                "row visual line {i} width {text_len} exceeded max_width {max_width}: {:?}",
                line_text(line)
            );
        }

        assert!(
            lines.len() > 3,
            "expected wrapping to produce multiple row visual lines, got {} total lines",
            lines.len()
        );
    }

    #[test]
    fn table_row_height_max_lines() {
        let table = GfmTable {
            headers: vec!["a".into(), "b".into()],
            alignments: vec![Alignment::None, Alignment::None],
            rows: vec![vec!["a\nb\nc".into(), "x".into()]],
        };

        let lines = render_table(&table, 80);
        assert_eq!(
            lines.len(),
            5,
            "expected header + sep + 3 row visual lines, got {}",
            lines.len()
        );

        let row_line_1 = line_text(&lines[2]);
        let row_line_2 = line_text(&lines[3]);
        let row_line_3 = line_text(&lines[4]);

        assert!(row_line_1.contains('a'), "row line 1: {:?}", row_line_1);
        assert!(row_line_1.contains('x'), "row line 1: {:?}", row_line_1);
        assert!(row_line_2.contains('b'), "row line 2: {:?}", row_line_2);
        assert!(row_line_3.contains('c'), "row line 3: {:?}", row_line_3);

        // Cell B is blank on visual lines 2 + 3; assert no stray 'x' in those lines.
        let count_x_line_2 = row_line_2.matches('x').count();
        let count_x_line_3 = row_line_3.matches('x').count();
        assert_eq!(
            count_x_line_2, 0,
            "cell B should be blank on row visual line 2, got {:?}",
            row_line_2
        );
        assert_eq!(
            count_x_line_3, 0,
            "cell B should be blank on row visual line 3, got {:?}",
            row_line_3
        );
    }

    #[test]
    fn table_single_line_unchanged() {
        let table = GfmTable {
            headers: vec!["A".into(), "B".into()],
            alignments: vec![Alignment::None, Alignment::None],
            rows: vec![
                vec!["a1".into(), "b1".into()],
                vec!["a2".into(), "b2".into()],
            ],
        };

        let lines = render_table(&table, 80);
        assert_eq!(
            lines.len(),
            4,
            "expected header + sep + 2 single-line rows = 4 lines, got {}",
            lines.len()
        );
    }

    #[test]
    fn table_combined_hard_soft() {
        let table = GfmTable {
            headers: vec!["h".into()],
            alignments: vec![Alignment::None],
            rows: vec![vec!["short\nthis is a long segment that wraps".into()]],
        };

        let max_width: u16 = 15;
        let lines = render_table(&table, max_width);

        // Expected: header + sep + 1 (short) + ≥2 (wrapped long segment) = ≥4 row visual lines total
        let row_visual_lines = lines.len() - 2;
        assert!(
            row_visual_lines >= 3,
            "expected ≥3 row visual lines (1 short + ≥2 wrapped), got {row_visual_lines}: {:?}",
            lines.iter().map(line_text).collect::<Vec<_>>()
        );

        let first_row_line = line_text(&lines[2]);
        assert!(
            first_row_line.contains("short"),
            "first row visual line should contain 'short', got {:?}",
            first_row_line
        );
    }

    #[test]
    fn table_truncation_respects_char_boundaries() {
        let table = GfmTable {
            headers: vec!["h".into()],
            alignments: vec![Alignment::None],
            rows: vec![vec!["aaa—bbb—ccc—ddd—eee—fff".into()]],
        };

        let max_width: u16 = 10;
        let lines = render_table(&table, max_width);

        for (i, line) in lines.iter().enumerate().skip(2) {
            let text = line_text(line);
            let display_width = UnicodeWidthStr::width(text.as_str());
            assert!(
                display_width <= max_width as usize,
                "row visual line {i} display width {display_width} exceeded max_width {max_width}: {:?}",
                text
            );
        }
    }

    #[test]
    fn table_alignment_preserved_multiline() {
        let table = GfmTable {
            headers: vec!["A".into(), "B".into()],
            alignments: vec![Alignment::None, Alignment::None],
            rows: vec![vec!["one\ntwo\nthree".into(), "x\ny".into()]],
        };

        let lines = render_table(&table, 80);
        // Skip header (0) and separator (1); collect row visual lines.
        let row_lines: Vec<String> = lines.iter().skip(2).map(line_text).collect();
        assert!(
            row_lines.len() >= 3,
            "expected ≥3 row visual lines, got {}",
            row_lines.len()
        );

        let pipe_offsets: Vec<Option<usize>> = row_lines.iter().map(|t| t.find('│')).collect();
        let first = pipe_offsets[0].expect("first row visual line should have '│'");
        for (i, off) in pipe_offsets.iter().enumerate() {
            assert_eq!(
                off,
                &Some(first),
                "row visual line {i} '│' offset {:?} differs from first ({first})",
                off
            );
        }
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
