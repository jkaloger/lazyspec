use pulldown_cmark::Alignment;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{GfmSegment, GfmTable};

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

fn truncate_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out
}

fn align_text(text: &str, width: usize, alignment: &Alignment) -> String {
    let text_width = UnicodeWidthStr::width(text);
    if text_width >= width {
        return truncate_to_width(text, width);
    }
    let padding = width - text_width;
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

    let mut col_widths: Vec<usize> = table.headers.iter().map(|h| h.len()).collect();
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                let longest_segment = cell.split('\n').map(str::len).max().unwrap_or(0);
                col_widths[i] = col_widths[i].max(longest_segment);
            }
        }
    }

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

    for row in &table.rows {
        let cell_lines: Vec<Vec<String>> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let width = col_widths.get(i).copied().unwrap_or(cell.len());
                wrap_cell(cell, width)
            })
            .collect();

        let row_height = cell_lines.iter().map(|c| c.len()).max().unwrap_or(1).max(1);

        for vis in 0..row_height {
            let row_spans: Vec<Span<'static>> = (0..col_count)
                .flat_map(|i| {
                    let alignment = alignments.get(i).unwrap_or(&Alignment::None);
                    let width = col_widths.get(i).copied().unwrap_or(0);
                    let text_ref: &str = cell_lines
                        .get(i)
                        .and_then(|cl| cl.get(vis))
                        .map(String::as_str)
                        .unwrap_or("");
                    let text = align_text(text_ref, width, alignment);
                    let mut spans = vec![Span::raw(text)];
                    if i < col_count - 1 {
                        spans.push(Span::raw(" │ ".to_string()));
                    }
                    spans
                })
                .collect();
            lines.push(Line::from(row_spans));
        }
    }

    lines
}

fn wrap_cell(cell: &str, width: usize) -> Vec<String> {
    if cell.is_empty() {
        return vec![String::new()];
    }
    if width == 0 {
        return cell.split('\n').map(String::from).collect();
    }
    let mut out: Vec<String> = Vec::new();
    for segment in cell.split('\n') {
        if segment.is_empty() {
            out.push(String::new());
            continue;
        }
        let wrapped = textwrap::wrap(segment, width);
        if wrapped.is_empty() {
            out.push(String::new());
        } else {
            for piece in wrapped {
                out.push(piece.into_owned());
            }
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
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
                    lines.push(Line::from(owned_spans).style(line.style));
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
