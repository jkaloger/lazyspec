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

use std::path::PathBuf;

use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{DocMeta, RelationType, Status};
use crate::engine::git_status::GitFileStatus;
#[cfg(feature = "agent")]
use crate::tui::agent::AgentStatus;
use crate::tui::state::{App, DocListNode, FilterField, PreviewTab};

use super::colors::{status_color, tag_color};
use super::layout::{calculate_image_height, wrapped_line_count, wrapped_lines_total};

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
                let img_height = image::image_dimensions(path)
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
                let img_height = image::image_dimensions(path)
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

fn doc_table_widths() -> [Constraint; 6] {
    [
        Constraint::Length(1),  // gutter
        Constraint::Length(4),  // tree
        Constraint::Length(18), // ID
        Constraint::Fill(1),    // title
        Constraint::Length(12), // status
        Constraint::Min(20),    // tags
    ]
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

    vec![id_cell, title_cell, status_cell, tags_cell]
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

    let mut lines = vec![
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
            Span::raw(&doc.author),
        ]),
        Line::from(vec![Span::raw(format!(" Date: {}", doc.date))]),
    ];

    if !doc.tags.is_empty() {
        let mut tag_spans = vec![Span::raw(" Tags: ")];
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

    lines.push(Line::from(""));

    if app.expansion_in_flight.as_ref() == Some(&doc.path) {
        lines.push(Line::from(Span::styled(
            " [expanding refs...]",
            Style::default().fg(Color::Yellow),
        )));
    }

    let header_lines: Vec<Line> = lines.clone();

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
        render_markdown_segment(&segments, panel_width, panel_height, content_width);
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
        render_markdown_segment(&segments, panel_width, panel_height, content_width);
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

    let filtered_paths: Vec<PathBuf> = app
        .filtered_docs_cache
        .clone()
        .unwrap_or_default();

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

#[cfg(test)]
pub(super) fn doc_row_cells_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    is_virtual: bool,
    dim: bool,
) -> Vec<Cell<'static>> {
    doc_row_cells(id, title, status, tags, is_virtual, dim, false, false)
}

#[cfg(test)]
pub(super) fn doc_row_cells_gh_for_test(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    is_virtual: bool,
    dim: bool,
    is_gh: bool,
) -> Vec<Cell<'static>> {
    doc_row_cells(id, title, status, tags, is_virtual, dim, is_gh, false)
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
}
