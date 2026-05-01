use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap,
    },
    Frame,
};
use unicode_width::UnicodeWidthChar;

use std::path::PathBuf;

use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{DocMeta, RelationType, Status};
use crate::engine::git_status::GitFileStatus;
#[cfg(feature = "agent")]
use crate::tui::agent::AgentStatus;
use crate::tui::state::{App, DocListNode, FilterField, PreviewTab};

use super::colors::{dimmed, edge_render, node_style, status_color, tag_color};
use super::layout::{calculate_image_height, wrapped_line_count, wrapped_lines_total};

fn get_image_dimensions_cached(app: &mut App, path: &std::path::Path) -> Option<(u32, u32)> {
    if let Some(&dims) = app.image_dimensions_cache.get(path) {
        return Some(dims);
    }
    if let Ok(dims) = image::image_dimensions(path) {
        let dims = (dims.0, dims.1);
        app.image_dimensions_cache.insert(path.to_path_buf(), dims);
        return Some(dims);
    }
    None
}

fn render_markdown_to_lines(text: &str, max_width: u16) -> Vec<Line<'static>> {
    let segments = crate::tui::content::gfm::extract_gfm_segments(text);
    crate::tui::content::gfm::render_gfm_segments(&segments, max_width)
}

fn render_scrollbar(f: &mut Frame, area: Rect, total: usize, visible: usize, position: usize) {
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let mut scrollbar_state = ScrollbarState::new(total)
        .viewport_content_length(visible)
        .position(position);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .track_style(Style::default().fg(Color::DarkGray))
        .thumb_style(Style::default().fg(Color::Cyan));
    f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
}

fn render_image_overlay(
    f: &mut Frame,
    app: &mut App,
    hash: u64,
    path: &std::path::Path,
    img_area: Rect,
) {
    if img_area.height == 0 {
        return;
    }
    if !app.image_states.contains_key(&hash) {
        if let Ok(dyn_img) = image::open(path) {
            let protocol = app.picker.new_resize_protocol(dyn_img);
            app.image_states.insert(hash, protocol);
        }
    }
    if let Some(state) = app.image_states.get_mut(&hash) {
        let widget =
            ratatui_image::StatefulImage::<ratatui_image::protocol::StatefulProtocol>::new();
        f.render_stateful_widget(widget, img_area, state);
    }
}

struct SegmentLines {
    lines: Vec<Line<'static>>,
    image_segments: Vec<(u64, std::path::PathBuf, u16)>,
    wrapped_height: usize,
}

fn render_markdown_segment(
    app: &mut App,
    segments: &[crate::tui::content::diagram::PreviewSegment],
    panel_width: u16,
    panel_height: u16,
    content_width: usize,
) -> SegmentLines {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut image_segments: Vec<(u64, std::path::PathBuf, u16)> = Vec::new();
    let mut wrapped_height: usize = 0;

    for segment in segments {
        match segment {
            crate::tui::content::diagram::PreviewSegment::Markdown(text) => {
                let gfm_lines = render_markdown_to_lines(text, panel_width);
                wrapped_height += wrapped_lines_total(&gfm_lines, content_width);
                lines.extend(gfm_lines);
            }
            crate::tui::content::diagram::PreviewSegment::DiagramImage(path) => {
                let hash = crate::tui::content::diagram::source_hash_path(path);
                let img_height = get_image_dimensions_cached(app, path)
                    .map(|(w, h)| calculate_image_height(w, h, panel_width, panel_height))
                    .unwrap_or(12);
                image_segments.push((hash, path.clone(), img_height));
                for _ in 0..img_height {
                    lines.push(Line::from(""));
                }
                wrapped_height += img_height as usize;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramText(text) => {
                for line_str in text.lines() {
                    let display_line = Line::from(Span::raw(format!(" {}", line_str)));
                    wrapped_height += wrapped_line_count(&display_line, content_width);
                    lines.push(display_line);
                }
            }
            crate::tui::content::diagram::PreviewSegment::DiagramLoading => {
                lines.push(Line::from(Span::styled(
                    " [rendering diagram...]",
                    Style::default().fg(Color::Yellow),
                )));
                wrapped_height += 1;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramError(msg) => {
                lines.push(Line::from(Span::styled(
                    format!(" [diagram error: {}]", msg),
                    Style::default().fg(Color::Red),
                )));
                wrapped_height += 1;
            }
        }
    }

    SegmentLines {
        lines,
        image_segments,
        wrapped_height,
    }
}

fn render_diagram_overlays(
    f: &mut Frame,
    app: &mut App,
    segments: &[crate::tui::content::diagram::PreviewSegment],
    inner: Rect,
    panel_width: u16,
    header_y_offset: u16,
    scroll_offset: u16,
) {
    let content_width = inner.width as usize;
    let mut y_offset = header_y_offset;

    for segment in segments {
        match segment {
            crate::tui::content::diagram::PreviewSegment::Markdown(text) => {
                let gfm_lines = render_markdown_to_lines(text, panel_width);
                y_offset += wrapped_lines_total(&gfm_lines, content_width) as u16;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramImage(path) => {
                let hash = crate::tui::content::diagram::source_hash_path(path);
                let img_height = get_image_dimensions_cached(app, path)
                    .map(|(w, h)| calculate_image_height(w, h, inner.width, inner.height))
                    .unwrap_or(12);

                if y_offset + img_height > scroll_offset && y_offset >= scroll_offset {
                    let scrolled_y = y_offset - scroll_offset;
                    let img_area = Rect::new(
                        inner.x,
                        inner.y.saturating_add(scrolled_y),
                        inner.width,
                        img_height.min(
                            inner
                                .bottom()
                                .saturating_sub(inner.y.saturating_add(scrolled_y)),
                        ),
                    );
                    if img_area.y < inner.bottom() {
                        render_image_overlay(f, app, hash, path, img_area);
                    }
                }
                y_offset += img_height;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramText(text) => {
                for line_str in text.lines() {
                    let display_line = Line::from(Span::raw(format!(" {}", line_str)));
                    y_offset += wrapped_line_count(&display_line, content_width) as u16;
                }
            }
            crate::tui::content::diagram::PreviewSegment::DiagramLoading => {
                y_offset += 1;
            }
            crate::tui::content::diagram::PreviewSegment::DiagramError(_) => {
                y_offset += 1;
            }
        }
    }
}

fn display_name(path: &std::path::Path) -> &str {
    let stem = path.file_stem().and_then(|s| s.to_str());
    match stem {
        Some("index") => path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("?"),
        Some(name) => name,
        None => "?",
    }
}

fn doc_table_widths() -> [Constraint; 7] {
    [
        Constraint::Length(1),  // gutter
        Constraint::Length(4),  // tree
        Constraint::Length(18), // ID
        Constraint::Fill(1),    // title
        Constraint::Length(12), // status
        Constraint::Length(24), // tags
        Constraint::Min(20),    // provenance
    ]
}

fn truncate_with_ellipsis(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let total: usize = s
        .chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    if total <= max_cols {
        return s.to_string();
    }
    if max_cols == 1 {
        return "…".to_string();
    }
    let budget = max_cols - 1;
    let mut acc = 0usize;
    let mut out = String::new();
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if acc + w > budget {
            break;
        }
        acc += w;
        out.push(c);
    }
    out.push('…');
    out
}

fn provenance_cell_text(provenance: &[String], max_cols: usize) -> String {
    if provenance.is_empty() {
        return String::new();
    }
    truncate_with_ellipsis(&provenance.join(", "), max_cols)
}

/// Returns `true` when `elapsed_secs` exceeds twice the given `cache_ttl`.
pub(crate) fn is_cache_stale(elapsed_secs: u64, cache_ttl: u64) -> bool {
    elapsed_secs >= 2 * cache_ttl
}

fn check_doc_stale(path: &std::path::Path, doc_type: &str, config: &Config) -> (bool, bool) {
    let is_gh = config
        .type_by_name(doc_type)
        .map(|td| td.store == StoreBackend::GithubIssues)
        .unwrap_or(false);
    let is_stale = if is_gh {
        let cache_ttl = config
            .documents
            .github
            .as_ref()
            .map(|g| g.cache_ttl)
            .unwrap_or(60);
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|elapsed| is_cache_stale(elapsed.as_secs(), cache_ttl))
            .unwrap_or(false)
    } else {
        false
    };
    (is_gh, is_stale)
}

#[allow(clippy::too_many_arguments)]
fn doc_row_cells(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
    is_stale: bool,
) -> Vec<Cell<'static>> {
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default();

    let id_style = if dim { dim_style } else { normal_style };
    let id_cell = if is_gh {
        let badge_style = if dim {
            dim_style
        } else {
            Style::default().fg(Color::Magenta)
        };
        Cell::new(Line::from(vec![
            Span::styled(format!("{:<18}", id), id_style),
            Span::styled(" [gh]", badge_style),
        ]))
    } else {
        Cell::new(Span::styled(format!("{:<18}", id), id_style))
    };

    let title_text = if is_virtual {
        format!("{} (virtual)", title)
    } else {
        title.to_string()
    };
    let title_style = if dim { dim_style } else { normal_style };
    let title_cell = Cell::new(Span::styled(title_text, title_style));

    let status_style = if dim {
        dim_style
    } else {
        Style::default().fg(status_color(status))
    };
    let status_cell = if is_stale {
        let stale_style = if dim {
            dim_style
        } else {
            Style::default().fg(Color::Red)
        };
        Cell::new(Line::from(vec![
            Span::styled(format!("{:<12}", status), status_style),
            Span::styled(" [!]", stale_style),
        ]))
    } else {
        Cell::new(Span::styled(format!("{:<12}", status), status_style))
    };

    let mut tag_spans: Vec<Span<'static>> = Vec::new();
    for (idx, tag) in tags.iter().take(3).enumerate() {
        if idx > 0 {
            tag_spans.push(Span::raw(" "));
        }
        let tc = if dim { Color::DarkGray } else { tag_color(tag) };
        tag_spans.push(Span::styled(format!("[{}]", tag), Style::default().fg(tc)));
    }
    if tags.len() > 3 {
        tag_spans.push(Span::styled(format!(" +{}", tags.len() - 3), dim_style));
    }
    let tags_cell = Cell::new(Line::from(tag_spans));

    let prov_text = provenance_cell_text(provenance, 20);
    let prov_style = if dim { dim_style } else { normal_style };
    let provenance_cell = Cell::new(Span::styled(prov_text, prov_style));

    vec![id_cell, title_cell, status_cell, tags_cell, provenance_cell]
}

fn doc_row_for_node(
    app: &App,
    node: &DocListNode,
    index: usize,
    dim: bool,
    config: &Config,
) -> Row<'static> {
    let tree_text = if node.depth > 0 {
        let leading = "   ".repeat(node.depth - 1);
        let is_last = match app.doc_tree.get(index + 1) {
            Some(next) => next.depth < node.depth,
            None => true,
        };
        let connector = if is_last { " └─ " } else { " ├─ " };
        format!("{}{}", leading, connector)
    } else if node.is_parent {
        let indicator = if app.is_expanded(&node.path) {
            "▼ "
        } else {
            "▶ "
        };
        format!("  {}", indicator)
    } else {
        "  ".to_string()
    };
    let tree_cell = Cell::new(Span::styled(
        tree_text,
        Style::default().fg(Color::DarkGray),
    ));

    let gutter_cell = match app.git_status_cache.get(&node.path) {
        Some(GitFileStatus::New) => Cell::from("┃").style(Style::default().fg(Color::Green)),
        Some(GitFileStatus::Modified) => Cell::from("┃").style(Style::default().fg(Color::Yellow)),
        None => Cell::from(" "),
    };

    let tags = app
        .store
        .get(&node.path)
        .map(|doc| doc.tags.clone())
        .unwrap_or_default();
    let provenance = app
        .store
        .get(&node.path)
        .map(|doc| doc.provenance.clone())
        .unwrap_or_default();

    let display_id = if node.has_duplicate_id {
        format!("! {}", node.id)
    } else {
        node.id.clone()
    };

    let (is_gh, is_stale) = check_doc_stale(&node.path, node.doc_type.as_str(), config);

    let mut cells = vec![gutter_cell, tree_cell];
    cells.extend(doc_row_cells(
        &display_id,
        &node.title,
        &node.status,
        &tags,
        &provenance,
        node.is_virtual,
        dim,
        is_gh,
        is_stale,
    ));

    let style = if dim {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    Row::new(cells).style(style)
}

pub fn draw_type_panel(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .doc_types
        .iter()
        .map(|dt| {
            let count = app.doc_count(dt);
            let plural = app
                .type_plurals
                .get(&dt.to_string())
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let content = format!("  {}  ({})", plural, count);
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Types "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(app.selected_type));
    f.render_stateful_widget(list, area, &mut state);
}

pub fn draw_doc_list(f: &mut Frame, app: &mut App, area: Rect, config: &Config) {
    app.doc_list_height = area.height.saturating_sub(2) as usize;
    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let rows: Vec<Row> = app
        .doc_tree
        .iter()
        .enumerate()
        .map(|(i, node)| doc_row_for_node(app, node, i, dim, config))
        .collect();

    let widths = doc_table_widths();

    let border_style = if relations_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let highlight_style = if relations_focused {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(" Documents "),
        )
        .row_highlight_style(highlight_style);

    let mut state = TableState::default()
        .with_selected(Some(app.selected_doc))
        .with_offset(app.doc_list_offset);
    f.render_stateful_widget(table, area, &mut state);

    let total_items = app.doc_tree.len();
    if !dim && total_items > app.doc_list_height {
        render_scrollbar(f, area, total_items, app.doc_list_height, app.selected_doc);
    }
}

pub fn draw_preview(f: &mut Frame, app: &mut App, area: Rect) {
    let preview_title = if app.preview_tab == PreviewTab::Preview {
        Line::from(vec![
            Span::styled(
                " Preview ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("| "),
            Span::styled("Relations ", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Preview ", Style::default().fg(Color::DarkGray)),
            Span::raw("| "),
            Span::styled(
                "Relations ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let border_style = if app.preview_tab == PreviewTab::Relations {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(preview_title);

    let doc = app.selected_doc_meta().cloned();
    match app.preview_tab {
        PreviewTab::Preview => render_document_preview(f, app, area, block, doc.as_ref()),
        PreviewTab::Relations => render_relationship_sections(f, app, area, block, doc.as_ref()),
    }
}

pub(super) fn build_preview_header_lines(doc: &DocMeta, expanding: bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            format!(" {}", doc.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw(" Type: "),
            Span::styled(
                format!("{}", doc.doc_type),
                Style::default().fg(Color::White),
            ),
            Span::raw("  Status: "),
            Span::styled(
                format!("{}", doc.status),
                Style::default().fg(status_color(&doc.status)),
            ),
            Span::raw("  Author: "),
            Span::raw(doc.author.clone()),
        ]),
        Line::from(vec![Span::raw(format!(" Date: {}", doc.date))]),
    ];

    if !doc.tags.is_empty() {
        let mut tag_spans: Vec<Span<'static>> = vec![Span::raw(" Tags: ")];
        for (idx, tag) in doc.tags.iter().enumerate() {
            if idx > 0 {
                tag_spans.push(Span::raw(" "));
            }
            tag_spans.push(Span::styled(
                format!("[{}]", tag),
                Style::default().fg(tag_color(tag)),
            ));
        }
        lines.push(Line::from(tag_spans));
    }

    if !doc.provenance.is_empty() {
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" Provenance: ")];
        for (idx, entry) in doc.provenance.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(", "));
            }
            spans.push(Span::raw(entry.clone()));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    if expanding {
        lines.push(Line::from(Span::styled(
            " [expanding refs...]",
            Style::default().fg(Color::Yellow),
        )));
    }

    lines
}

pub fn render_document_preview(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    block: Block,
    doc: Option<&DocMeta>,
) {
    let Some(doc) = doc else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    };

    let body = app
        .expanded_body_cache
        .get(&doc.path)
        .cloned()
        .unwrap_or_default();

    let expanding = app.expansion_in_flight.as_ref() == Some(&doc.path);
    let header_lines = build_preview_header_lines(doc, expanding);
    let mut lines = header_lines.clone();

    let diagram_blocks = match &app.diagram_blocks_cache {
        Some((p, _, b)) if p == &doc.path => b.clone(),
        _ => crate::tui::content::diagram::extract_diagram_blocks(&body),
    };
    let panel_width = area.width.saturating_sub(2);
    let panel_height = area.height.saturating_sub(2);
    let segments = crate::tui::content::diagram::build_preview_segments(
        &body,
        &app.diagram_cache,
        app.terminal_image_protocol,
        &app.tool_availability,
        &diagram_blocks,
    );

    let content_width = area.width.saturating_sub(2) as usize;
    let segment_lines =
        render_markdown_segment(app, &segments, panel_width, panel_height, content_width);
    let has_images = !segment_lines.image_segments.is_empty();
    lines.extend(segment_lines.lines);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);

    if has_images {
        let inner = area.inner(ratatui::layout::Margin {
            horizontal: 1,
            vertical: 1,
        });
        let header_y = wrapped_lines_total(&header_lines, inner.width as usize) as u16;
        let segments_ref = crate::tui::content::diagram::build_preview_segments(
            &body,
            &app.diagram_cache,
            app.terminal_image_protocol,
            &app.tool_availability,
            &diagram_blocks,
        );
        render_diagram_overlays(f, app, &segments_ref, inner, panel_width, header_y, 0);
    }
}

pub fn render_relationship_sections(
    f: &mut Frame,
    app: &App,
    area: Rect,
    block: Block,
    doc: Option<&DocMeta>,
) {
    let Some(doc) = doc else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    };

    let all_items = app.relation_items(doc);

    if all_items.is_empty() {
        let paragraph = Paragraph::new(" No relations.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    }

    let mut chain_paths = Vec::new();
    {
        let mut current_path = doc.path.clone();
        while let Some(current_doc) = app.store.get(&current_path) {
            let implements_target = current_doc.related.iter().find_map(|r| {
                if r.rel_type == RelationType::Implements {
                    if let Some(fwd) = app.store.forward_links.get(&current_doc.path) {
                        for (rel, target) in fwd {
                            if *rel == RelationType::Implements {
                                return Some(target.clone());
                            }
                        }
                    }
                    None
                } else {
                    None
                }
            });
            match implements_target {
                Some(parent) => {
                    chain_paths.push(parent.clone());
                    current_path = parent;
                }
                None => break,
            }
        }
        chain_paths.reverse();
    }

    let mut children_paths = Vec::new();
    if let Some(rev) = app.store.reverse_links.get(&doc.path) {
        for (rel, source) in rev {
            if *rel == RelationType::Implements {
                children_paths.push(source.clone());
            }
        }
    }

    let mut related_paths = Vec::new();
    if let Some(fwd) = app.store.forward_links.get(&doc.path) {
        for (rel, target) in fwd {
            if *rel == RelationType::RelatedTo {
                related_paths.push(target.clone());
            }
        }
    }
    if let Some(rev) = app.store.reverse_links.get(&doc.path) {
        for (rel, source) in rev {
            if *rel == RelationType::RelatedTo {
                related_paths.push(source.clone());
            }
        }
    }

    let mut items: Vec<ListItem> = Vec::new();
    let mut flat_index = 0usize;
    let mut list_index = 0usize;
    let mut selected_flat_index = 0usize;

    let section_header = |label: &str| -> ListItem {
        ListItem::new(Line::from(Span::styled(
            format!("  {}", label),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )))
    };

    let render_item = |path: &std::path::Path| -> ListItem {
        let (title, doc_type_str, status_str, status_clr) =
            if let Some(target_doc) = app.store.get(path) {
                (
                    target_doc.title.clone(),
                    format!("{}", target_doc.doc_type),
                    format!("{}", target_doc.status),
                    status_color(&target_doc.status),
                )
            } else {
                let name = display_name(path);
                (
                    name.to_string(),
                    "?".to_string(),
                    "missing".to_string(),
                    Color::Red,
                )
            };

        ListItem::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{:<35} ", title), Style::default()),
            Span::styled(
                format!("{} ", doc_type_str),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("[{}]", status_str), Style::default().fg(status_clr)),
        ]))
    };

    let sections: Vec<(&str, &Vec<std::path::PathBuf>)> = vec![
        ("chain", &chain_paths),
        ("children", &children_paths),
        ("related", &related_paths),
    ];

    for (label, paths) in &sections {
        if paths.is_empty() {
            continue;
        }

        items.push(section_header(label));
        list_index += 1;

        for path in *paths {
            if flat_index == app.selected_relation {
                selected_flat_index = list_index;
            }
            items.push(render_item(path));
            flat_index += 1;
            list_index += 1;
        }
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("  > ");
    let total_items = list_index;
    let mut state = ListState::default().with_selected(Some(selected_flat_index));
    f.render_stateful_widget(list, area, &mut state);

    let visible_height = area.height.saturating_sub(2) as usize;
    if total_items > visible_height {
        render_scrollbar(f, area, total_items, visible_height, selected_flat_index);
    }
}

pub fn render_fullscreen_document(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.fullscreen_height = area.height.saturating_sub(2) as usize;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let Some(doc) = app.selected_doc_meta() else {
        return;
    };

    let header = Line::from(vec![
        Span::styled(
            format!(" {} ", doc.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{}", doc.status),
            Style::default().fg(status_color(&doc.status)),
        ),
        Span::raw(format!(" | {} | {} ", doc.doc_type, doc.author)),
        Span::styled("[Esc] back", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(header), layout[0]);

    let body = app
        .expanded_body_cache
        .get(&doc.path)
        .cloned()
        .unwrap_or_default();

    let expanding = app.expansion_in_flight.as_ref() == Some(&doc.path);
    let display_body = if expanding {
        format!("[expanding refs...]\n\n{}", body)
    } else {
        body
    };

    let content_width = layout[1].width.saturating_sub(2) as usize;
    let panel_width = layout[1].width.saturating_sub(2);
    let panel_height = layout[1].height.saturating_sub(2);

    let fullscreen_blocks = match &app.diagram_blocks_cache {
        Some((p, _, b)) if p == &doc.path => b.clone(),
        _ => crate::tui::content::diagram::extract_diagram_blocks(&display_body),
    };
    let segments = crate::tui::content::diagram::build_preview_segments(
        &display_body,
        &app.diagram_cache,
        app.terminal_image_protocol,
        &app.tool_availability,
        &fullscreen_blocks,
    );

    let segment_lines =
        render_markdown_segment(app, &segments, panel_width, panel_height, content_width);
    let total_lines = segment_lines.wrapped_height;

    let paragraph = Paragraph::new(segment_lines.lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));
    f.render_widget(paragraph, layout[1]);

    let inner = layout[1].inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    render_diagram_overlays(f, app, &segments, inner, panel_width, 0, app.scroll_offset);

    if total_lines > app.fullscreen_height {
        render_scrollbar(
            f,
            layout[1],
            total_lines,
            app.fullscreen_height,
            app.scroll_offset as usize,
        );
    }
}

pub fn render_filter_panel(f: &mut Frame, app: &mut App, area: Rect, config: &Config) {
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main[1]);

    let status_value = match &app.filter_status {
        None => "all".to_string(),
        Some(s) => format!("{}", s),
    };
    let tag_value = match &app.filter_tag {
        None => "all".to_string(),
        Some(t) => t.clone(),
    };

    let status_style = if app.filter_focused == FilterField::Status {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if app.filter_status.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let tag_style = if app.filter_focused == FilterField::Tag {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if app.filter_tag.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let clear_style = if app.filter_focused == FilterField::ClearAction {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let filter_lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Status: [{}]", status_value),
            status_style,
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  Tag:    [{}]", tag_value),
            tag_style,
        )),
        Line::from(""),
        Line::from(Span::styled("  [clear filters]", clear_style)),
    ];

    let filter_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Filters ");
    let filter_paragraph = Paragraph::new(filter_lines).block(filter_block);
    f.render_widget(filter_paragraph, main[0]);

    app.doc_list_height = right[0].height.saturating_sub(2) as usize;
    let filtered_count = app.filtered_docs_count();
    let total_count = app.store.all_docs().len();

    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let filtered_paths: Vec<PathBuf> = app.filtered_docs_cache.clone().unwrap_or_default();

    let rows: Vec<Row> = filtered_paths
        .iter()
        .filter_map(|p| app.store.get(p))
        .map(|doc| {
            let gutter_cell = match app.git_status_cache.get(&doc.path) {
                Some(GitFileStatus::New) => {
                    Cell::from("┃").style(Style::default().fg(Color::Green))
                }
                Some(GitFileStatus::Modified) => {
                    Cell::from("┃").style(Style::default().fg(Color::Yellow))
                }
                None => Cell::from(" "),
            };
            let tree_cell = Cell::new("");
            let mut cells = vec![gutter_cell, tree_cell];
            let (is_gh, is_stale) = check_doc_stale(&doc.path, doc.doc_type.as_str(), config);
            cells.extend(doc_row_cells(
                &doc.id,
                &doc.title,
                &doc.status,
                &doc.tags,
                &doc.provenance,
                doc.virtual_doc,
                dim,
                is_gh,
                is_stale,
            ));
            let style = if dim {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            Row::new(cells).style(style)
        })
        .collect();

    let widths = doc_table_widths();

    let border_style = if relations_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let highlight_style = if relations_focused {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(format!(
                    " Documents ({} of {}) ",
                    filtered_count, total_count
                )),
        )
        .row_highlight_style(highlight_style);

    let mut state = TableState::default()
        .with_selected(Some(app.selected_doc))
        .with_offset(app.doc_list_offset);
    f.render_stateful_widget(table, right[0], &mut state);

    let doc = app.selected_filtered_doc().cloned();
    let preview_title = if app.preview_tab == PreviewTab::Preview {
        Line::from(vec![
            Span::styled(
                " Preview ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("| "),
            Span::styled("Relations ", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Preview ", Style::default().fg(Color::DarkGray)),
            Span::raw("| "),
            Span::styled(
                "Relations ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let preview_border_style = if app.preview_tab == PreviewTab::Relations {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(preview_border_style)
        .title(preview_title);

    match app.preview_tab {
        PreviewTab::Preview => render_document_preview(f, app, right[1], block, doc.as_ref()),
        PreviewTab::Relations => {
            render_relationship_sections(f, app, right[1], block, doc.as_ref())
        }
    }
}

#[cfg(feature = "agent")]
pub fn draw_agents_screen(f: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = layout[0];
    let footer_area = layout[1];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Agents ");

    if app.agent_spawner.records.is_empty() {
        let paragraph = Paragraph::new(
            "No agents have been invoked yet. Press `a` on a document to start one.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center)
        .block(block);
        f.render_widget(paragraph, main_area);
    } else {
        let rows: Vec<Row> = app
            .agent_spawner
            .records
            .iter()
            .map(|record| {
                let (icon, color) = match record.status {
                    AgentStatus::Running => ("●", Color::Yellow),
                    AgentStatus::Complete => ("✔", Color::Green),
                    AgentStatus::Failed => ("✘", Color::Red),
                };
                Row::new(vec![
                    Cell::from(Span::styled(
                        format!("  {}", icon),
                        Style::default().fg(color),
                    )),
                    Cell::from(Span::raw(format!(
                        "{:<14}",
                        record
                            .session_id
                            .split('-')
                            .next()
                            .unwrap_or(&record.session_id)
                    ))),
                    Cell::from(Span::raw(&*record.doc_title)),
                    Cell::from(Span::raw(&*record.action)),
                    Cell::from(Span::styled(
                        &*record.started_at,
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(4),
            Constraint::Length(14),
            Constraint::Fill(1),
            Constraint::Length(18),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths)
            .block(block)
            .header(
                Row::new(vec!["  ", "Session", "Document", "Action", "Started"]).style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        let mut state = TableState::default().with_selected(Some(app.agent_selected_index));
        f.render_stateful_widget(table, main_area, &mut state);
    }

    let footer = Line::from(vec![
        Span::styled("e", Style::default().fg(Color::Cyan)),
        Span::raw(": open document  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(": resume session  "),
        Span::styled("`", Style::default().fg(Color::Cyan)),
        Span::raw(": switch view"),
    ]);
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        footer_area,
    );
}

#[cfg(feature = "metrics")]
pub fn draw_metrics_skeleton(f: &mut Frame, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let left = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Metrics ");
    f.render_widget(left, layout[0]);

    let right = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Status Flow ");
    f.render_widget(right, layout[1]);
}

pub fn draw_graph(f: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let left = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Graph ");
    f.render_widget(left, layout[0]);

    let items: Vec<ListItem> = app
        .graph_nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let mut spans = Vec::new();

            if node.depth > 0 {
                let leading = "   ".repeat(node.depth - 1);
                let is_last = match app.graph_nodes.get(i + 1) {
                    Some(next) => next.depth <= node.depth,
                    None => true,
                };
                let connector = if is_last {
                    " └─▶ "
                } else {
                    " ├─▶ "
                };
                spans.push(Span::styled(
                    format!("{}{}", leading, connector),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let type_icon = app
                .type_icons
                .get(&node.doc_type.to_string())
                .map(|s| s.as_str())
                .unwrap_or("○");
            spans.push(Span::styled(
                format!("{} ", type_icon),
                Style::default().fg(Color::Gray),
            ));

            spans.push(Span::styled(
                format!("{} ", node.title),
                Style::default().fg(Color::White),
            ));

            spans.push(Span::styled(
                format!("{}", node.status),
                Style::default().fg(status_color(&node.status)),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Dependency Graph "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default().with_selected(Some(app.graph_selected));
    f.render_stateful_widget(list, layout[1], &mut state);
}

/// Sequencing screen — read-only DAG render (RFC-041 / STORY-121 Task 4).
///
/// Layout: layers are rendered top-to-bottom; nodes within a layer are spaced
/// horizontally. Picked top-to-bottom because variable-width labels (`<id>
/// <title>`) pack better as rows, and the dependency-flow direction reads as
/// "earlier work above, later work below", matching typical DAG diagrams.
///
/// Edges:
/// - Adjacent-layer blocks/implements edges are drawn as a small connector
///   row between rows (`──▶` / `╌╌▷`) at the source's column.
/// - Multi-layer edges (source layer + 1 < target layer) are represented by
///   a small badge on the source node ("→…") rather than a routed line. This
///   slice intentionally avoids routing to keep the renderer simple; the AC
///   set is satisfied by edge-style and node-style helpers.
///
/// Out-of-scope nodes are rendered with `Modifier::DIM` and `Color::DarkGray`
/// (AC3/AC4). Under `Scope::All` every node is in scope, so nothing is dimmed
/// (AC5).
pub fn draw_sequencing(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::LightGreen))
        .title(" Sequencing ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let state = &app.sequencing;
    let layers = &state.layout.layers;

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Header: scope summary + any error/info from last action. The status bar
    // is the canonical surface for these (see `mode` extension), but a
    // lightweight in-panel echo helps when the bar is collapsed.
    lines.push(Line::from(vec![Span::styled(
        scope_summary(state),
        Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
    )]));
    if let Some(err) = state.error.as_ref() {
        lines.push(Line::from(vec![Span::styled(
            format!("error: {}", err),
            Style::default().fg(Color::Red),
        )]));
    }
    if let Some(info) = state.info.as_ref() {
        lines.push(Line::from(vec![Span::styled(
            format!("info: {}", info),
            Style::default().fg(Color::Gray),
        )]));
    }
    if let Some((mode, buf)) = state.scope_input.as_ref() {
        let label = match mode {
            crate::tui::state::ScopeInputMode::Under => "scope under",
            crate::tui::state::ScopeInputMode::After => "scope after",
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{}: {}_", label, buf),
            Style::default().fg(Color::Yellow),
        )]));
    }

    if layers.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "no nodes to display (graph empty or all-cyclic)",
            Style::default().fg(Color::DarkGray),
        )]));
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    // Pre-index edges by source for badge generation.
    let mut multi_layer_out: std::collections::HashMap<String, Vec<&str>> =
        std::collections::HashMap::new();
    let layer_of: std::collections::HashMap<&str, usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(i, layer)| layer.iter().map(move |n| (n.as_str(), i)))
        .collect();

    for (src, dst, _kind) in &state.layout.edges {
        let s_layer = layer_of.get(src.as_str()).copied();
        let d_layer = layer_of.get(dst.as_str()).copied();
        if let (Some(s), Some(d)) = (s_layer, d_layer) {
            if d > s + 1 {
                multi_layer_out
                    .entry(src.0.clone())
                    .or_default()
                    .push(dst.as_str());
            }
        }
    }

    for (li, layer) in layers.iter().enumerate() {
        let mut node_spans: Vec<Span<'static>> = Vec::new();
        for (ni, node) in layer.iter().enumerate() {
            if ni > 0 {
                node_spans.push(Span::raw("   "));
            }
            let id = node.as_str();
            let (title, status_opt) = lookup_doc(app, id);
            let base = match status_opt.as_ref() {
                Some(status) => node_style(status),
                None => Style::default().fg(Color::White),
            };
            let in_scope = state.in_scope.contains(node);
            let label = format!("{} {}", id, title);
            let style = dimmed(base, !in_scope);
            node_spans.push(Span::styled(label, style));

            if let Some(targets) = multi_layer_out.get(id) {
                let badge = format!(" →…({})", targets.len());
                node_spans.push(Span::styled(
                    badge,
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        lines.push(Line::from(node_spans));

        // Adjacent-layer edge connector row: render a glyph per edge whose
        // source sits in this layer and target sits in li+1.
        if li + 1 < layers.len() {
            let mut edge_spans: Vec<Span<'static>> = Vec::new();
            let mut first = true;
            for (src, dst, kind) in &state.layout.edges {
                let s_layer = layer_of.get(src.as_str()).copied();
                let d_layer = layer_of.get(dst.as_str()).copied();
                if s_layer == Some(li) && d_layer == Some(li + 1) {
                    if !first {
                        edge_spans.push(Span::raw("  "));
                    }
                    first = false;
                    let render = edge_render(*kind);
                    edge_spans.push(Span::styled(
                        format!("{} {} {}", src.as_str(), render.glyph, dst.as_str()),
                        render.style,
                    ));
                }
            }
            if !edge_spans.is_empty() {
                lines.push(Line::from(edge_spans));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn scope_summary(state: &crate::tui::state::SequencingState) -> String {
    use crate::engine::sequencing::Scope;
    match &state.scope {
        Scope::All => "scope: none (showing all nodes)".to_string(),
        Scope::Under(id) => format!("scope: under {}", id),
        Scope::After(id) => format!("scope: after {}", id),
    }
}

fn lookup_doc(app: &App, id: &str) -> (String, Option<Status>) {
    match app.store.resolve_shorthand(id) {
        Ok(meta) => (meta.title.clone(), Some(meta.status.clone())),
        Err(_) => (String::new(), None),
    }
}

#[cfg(test)]
pub(super) fn doc_row_cells_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
) -> Vec<Cell<'static>> {
    doc_row_cells(
        id, title, status, tags, provenance, is_virtual, dim, false, false,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn doc_row_cells_gh_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    provenance: &[String],
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
) -> Vec<Cell<'static>> {
    doc_row_cells(
        id, title, status, tags, provenance, is_virtual, dim, is_gh, false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_cache_stale_returns_false_within_threshold() {
        assert!(!is_cache_stale(119, 60));
    }

    #[test]
    fn is_cache_stale_returns_true_at_boundary() {
        assert!(is_cache_stale(120, 60));
    }

    #[test]
    fn is_cache_stale_returns_true_beyond_threshold() {
        assert!(is_cache_stale(121, 60));
    }

    #[test]
    fn is_cache_stale_zero_ttl() {
        assert!(is_cache_stale(1, 0));
        assert!(is_cache_stale(0, 0));
    }

    #[test]
    fn is_cache_stale_large_ttl() {
        assert!(!is_cache_stale(3599, 1800));
        assert!(is_cache_stale(3600, 1800));
        assert!(is_cache_stale(3601, 1800));
    }

    #[test]
    fn truncate_no_change_when_fits() {
        assert_eq!(truncate_with_ellipsis("ab", 5), "ab");
    }

    #[test]
    fn truncate_appends_ellipsis_when_overflows() {
        assert_eq!(truncate_with_ellipsis("abcdef", 4), "abc…");
    }

    #[test]
    fn truncate_zero_width_returns_empty() {
        assert_eq!(truncate_with_ellipsis("abc", 0), "");
    }

    #[test]
    fn truncate_one_width_returns_ellipsis() {
        assert_eq!(truncate_with_ellipsis("abc", 1), "…");
    }

    fn cell_text(cell: &Cell) -> String {
        format!("{:?}", cell)
    }

    #[test]
    fn doc_row_cells_appends_provenance_cell() {
        let provenance = vec!["Alice".to_string()];
        let cells = doc_row_cells_for_test(
            "RFC-001",
            "Title",
            &Status::Draft,
            &[],
            &provenance,
            false,
            false,
        );
        assert_eq!(cells.len(), 5);
        let dbg = cell_text(&cells[4]);
        assert!(
            dbg.contains("Alice"),
            "provenance cell should contain joined entries, got: {}",
            dbg
        );
    }

    #[test]
    fn doc_row_cells_provenance_empty_when_list_empty() {
        let cells =
            doc_row_cells_for_test("RFC-001", "Title", &Status::Draft, &[], &[], false, false);
        let dbg = cell_text(&cells[4]);
        assert!(
            !dbg.contains("Alice") && !dbg.contains('…'),
            "empty provenance cell should not show entries, got: {}",
            dbg
        );
        assert_eq!(provenance_cell_text(&[], 20), "");
    }

    #[test]
    fn doc_row_cells_provenance_comma_joined() {
        let provenance = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let text = provenance_cell_text(&provenance, 20);
        assert_eq!(text, "A, B, C");
    }

    #[test]
    fn doc_row_cells_provenance_truncated_overflow() {
        let provenance = vec!["aaaaaaaaaaaaaaa".to_string(), "bbbbbbbbbbbbbbb".to_string()];
        let text = provenance_cell_text(&provenance, 20);
        assert!(
            text.ends_with('…'),
            "overflowing provenance should end with ellipsis, got: {}",
            text
        );
    }

    fn line_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    fn fixture_doc_meta() -> DocMeta {
        use crate::engine::document::DocType;
        use chrono::NaiveDate;
        DocMeta {
            id: "RFC-001".to_string(),
            doc_type: DocType::new("rfc"),
            title: "Test".to_string(),
            status: Status::Draft,
            author: "jkaloger".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 4, 29).unwrap(),
            tags: vec![],
            related: vec![],
            provenance: vec![],
            validate_ignore: false,
            path: PathBuf::from("docs/rfcs/RFC-001.md"),
            virtual_doc: false,
            priority: None,
        }
    }

    #[test]
    fn preview_header_includes_provenance_when_present() {
        let mut doc = fixture_doc_meta();
        doc.provenance = vec!["X".to_string(), "Y".to_string()];
        let lines = build_preview_header_lines(&doc, false);
        let prov_line = lines
            .iter()
            .find(|l| line_text(l).contains("Provenance:"))
            .expect("provenance line should be present");
        let text = line_text(prov_line);
        assert!(text.contains('X'), "should contain X, got: {}", text);
        assert!(text.contains('Y'), "should contain Y, got: {}", text);
    }

    #[test]
    fn preview_header_omits_provenance_when_empty() {
        let doc = fixture_doc_meta();
        let lines = build_preview_header_lines(&doc, false);
        for line in &lines {
            assert!(
                !line_text(line).contains("Provenance:"),
                "no line should mention Provenance when empty"
            );
        }
    }
}
