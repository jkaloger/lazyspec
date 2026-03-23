use pulldown_cmark::Alignment;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

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

    let mut col_widths: Vec<usize> = table.headers.iter().map(|h| h.len()).collect();
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.len());
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
