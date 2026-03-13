use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Wrap},
    Frame,
};

use crate::engine::document::{DocMeta, RelationType, Status};
#[cfg(feature = "agent")]
use crate::tui::agent::AgentStatus;
use crate::tui::app::{App, DocListNode, FilterField, FormField, PreviewTab, ViewMode};

fn status_color(status: &Status) -> Color {
    match status {
        Status::Draft => Color::Yellow,
        Status::Review => Color::Blue,
        Status::Accepted => Color::Green,
        Status::Rejected => Color::Red,
        Status::Superseded => Color::DarkGray,
    }
}

fn tag_color(tag: &str) -> Color {
    const PALETTE: &[Color] = &[
        Color::Magenta,
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Red,
        Color::LightMagenta,
        Color::LightCyan,
        Color::LightGreen,
        Color::LightBlue,
    ];
    let hash = tag.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[(hash as usize) % PALETTE.len()]
}

fn render_scrollbar(f: &mut Frame, area: Rect, total: usize, visible: usize, position: usize) {
    let inner = area.inner(Margin { vertical: 1, horizontal: 0 });
    let mut scrollbar_state = ScrollbarState::new(total)
        .viewport_content_length(visible)
        .position(position);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .track_style(Style::default().fg(Color::DarkGray))
        .thumb_style(Style::default().fg(Color::Cyan));
    f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
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

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.fullscreen_doc {
        draw_fullscreen(f, app);
        if app.show_warnings {
            draw_warnings_panel(f, app);
        }
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }
    if app.create_form.active {
        draw_create_form(f, app);
        if app.show_warnings {
            draw_warnings_panel(f, app);
        }
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }
    if app.search_mode {
        draw_search_overlay(f, app);
        if app.show_warnings {
            draw_warnings_panel(f, app);
        }
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(f.area());

    let title = Line::from(vec![Span::styled(
        "  lazyspec",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]);
    f.render_widget(Paragraph::new(title), outer[0]);

    let mode_indicator = Line::from(vec![Span::styled(
        format!("[{}] ` to cycle ", app.view_mode.name()),
        Style::default().fg(Color::DarkGray),
    )]);
    f.render_widget(
        Paragraph::new(mode_indicator).alignment(Alignment::Right),
        outer[0],
    );

    match app.view_mode {
        ViewMode::Types => {
            let main = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(outer[1]);

            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(main[1]);

            draw_type_panel(f, app, main[0]);
            draw_doc_list(f, app, right[0]);
            draw_preview(f, app, right[1]);
        }
        ViewMode::Filters => draw_filters_mode(f, app, outer[1]),
        ViewMode::Metrics => draw_metrics_skeleton(f, outer[1]),
        ViewMode::Graph => draw_graph(f, app, outer[1]),
        #[cfg(feature = "agent")]
        ViewMode::Agents => draw_agents_screen(f, app, outer[1]),
    }

    if app.delete_confirm.active {
        draw_delete_confirm(f, app);
    }

    if app.status_picker.active {
        draw_status_picker(f, app);
    }

    #[cfg(feature = "agent")]
    if app.agent_dialog.active {
        draw_agent_dialog(f, app);
    }

    if app.show_warnings {
        draw_warnings_panel(f, app);
    }

    if app.show_help {
        draw_help_overlay(f);
    }
}

fn draw_type_panel(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .doc_types
        .iter()
        .enumerate()
        .map(|(_, dt)| {
            let count = app.doc_count(dt);
            let plural = app.type_plurals.get(&dt.to_string()).map(|s| s.as_str()).unwrap_or("unknown");
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

fn doc_table_widths() -> [Constraint; 5] {
    [
        Constraint::Length(4),  // tree
        Constraint::Length(14), // ID
        Constraint::Fill(1),   // title
        Constraint::Length(12), // status
        Constraint::Min(20),   // tags
    ]
}

fn doc_row_cells(
    id: &str,
    title: &str,
    status: &Status,
    tags: &[String],
    is_virtual: bool,
    dim: bool,
) -> Vec<Cell<'static>> {
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default();

    let id_style = if dim { dim_style } else { normal_style };
    let id_cell = Cell::new(Span::styled(format!("{:<14}", id), id_style));

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
    let status_cell = Cell::new(Span::styled(format!("{:<12}", status), status_style));

    let mut tag_spans: Vec<Span<'static>> = Vec::new();
    for (idx, tag) in tags.iter().take(3).enumerate() {
        if idx > 0 {
            tag_spans.push(Span::raw(" "));
        }
        let tc = if dim { Color::DarkGray } else { tag_color(tag) };
        tag_spans.push(Span::styled(format!("[{}]", tag), Style::default().fg(tc)));
    }
    if tags.len() > 3 {
        tag_spans.push(Span::styled(
            format!(" +{}", tags.len() - 3),
            dim_style,
        ));
    }
    let tags_cell = Cell::new(Line::from(tag_spans));

    vec![id_cell, title_cell, status_cell, tags_cell]
}

/// Builds a Table Row for a tree node: tree indicator cell + 4 doc column cells.
fn doc_row_for_node(app: &App, node: &DocListNode, index: usize, dim: bool) -> Row<'static> {
    let tree_text = if node.depth > 0 {
        let leading = "   ".repeat(node.depth - 1);
        let is_last = match app.doc_tree.get(index + 1) {
            Some(next) => next.depth < node.depth,
            None => true,
        };
        let connector = if is_last { " └─ " } else { " ├─ " };
        format!("{}{}", leading, connector)
    } else if node.is_parent {
        let indicator = if app.is_expanded(&node.path) { "▼ " } else { "▶ " };
        format!("  {}", indicator)
    } else {
        "  ".to_string()
    };
    let tree_cell = Cell::new(Span::styled(tree_text, Style::default().fg(Color::DarkGray)));

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

    let mut cells = vec![tree_cell];
    cells.extend(doc_row_cells(
        &display_id,
        &node.title,
        &node.status,
        &tags,
        node.is_virtual,
        dim,
    ));

    let style = if dim {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    Row::new(cells).style(style)
}

fn draw_doc_list(f: &mut Frame, app: &mut App, area: Rect) {
    app.doc_list_height = area.height.saturating_sub(2) as usize;
    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let rows: Vec<Row> = app
        .doc_tree
        .iter()
        .enumerate()
        .map(|(i, node)| doc_row_for_node(app, node, i, dim))
        .collect();

    let widths = doc_table_widths();

    let border_style = if relations_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let highlight_style = if relations_focused {
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
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

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
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

    let doc = app.selected_doc_meta();
    match app.preview_tab {
        PreviewTab::Preview => draw_preview_content(f, app, area, block, doc),
        PreviewTab::Relations => draw_relations_content(f, app, area, block, doc),
    }
}

fn draw_preview_content(f: &mut Frame, app: &App, area: Rect, block: Block, doc: Option<&DocMeta>) {
    if let Some(doc) = doc {
        let body = if let Some(expanded) = app.expanded_body_cache.get(&doc.path) {
            expanded.clone()
        } else {
            app.store.get_body_raw(&doc.path).unwrap_or_default()
        };

        let mut lines = vec![
            Line::from(Span::styled(
                format!(" {}", doc.title),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw(" Type: "),
                Span::styled(format!("{}", doc.doc_type), Style::default().fg(Color::White)),
                Span::raw("  Status: "),
                Span::styled(
                    format!("{}", doc.status),
                    Style::default().fg(status_color(&doc.status)),
                ),
                Span::raw("  Author: "),
                Span::raw(&doc.author),
            ]),
            Line::from(vec![
                Span::raw(format!(" Date: {}", doc.date)),
            ]),
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

        let body_text = tui_markdown::from_str(&body);
        for line in body_text.lines {
            lines.push(line);
        }

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    } else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }
}

fn draw_relations_content(f: &mut Frame, app: &App, area: Rect, block: Block, doc: Option<&DocMeta>) {
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

    // Build the three sections using the same logic as relation_items
    // Chain: walk Implements upward
    let mut chain_paths = Vec::new();
    {
        let mut current_path = doc.path.clone();
        loop {
            let current_doc = match app.store.get(&current_path) {
                Some(d) => d,
                None => break,
            };
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

    // Children: reverse Implements
    let mut children_paths = Vec::new();
    if let Some(rev) = app.store.reverse_links.get(&doc.path) {
        for (rel, source) in rev {
            if *rel == RelationType::Implements {
                children_paths.push(source.clone());
            }
        }
    }

    // Related: forward + reverse RelatedTo
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
                (name.to_string(), "?".to_string(), "missing".to_string(), Color::Red)
            };

        ListItem::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("{:<35} ", title), Style::default()),
            Span::styled(format!("{} ", doc_type_str), Style::default().fg(Color::DarkGray)),
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
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("  > ");
    let total_items = list_index;
    let mut state = ListState::default().with_selected(Some(selected_flat_index));
    f.render_stateful_widget(list, area, &mut state);

    let visible_height = area.height.saturating_sub(2) as usize;
    if total_items > visible_height {
        render_scrollbar(f, area, total_items, visible_height, selected_flat_index);
    }
}

fn draw_fullscreen(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.fullscreen_height = area.height.saturating_sub(2) as usize;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    if let Some(doc) = app.selected_doc_meta() {
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

        let body = if let Some(expanded) = app.expanded_body_cache.get(&doc.path) {
            expanded.clone()
        } else {
            app.store.get_body_raw(&doc.path).unwrap_or_else(|_| "Error loading document.".to_string())
        };

        let expanding = app.expansion_in_flight.as_ref() == Some(&doc.path);
        let display_body = if expanding {
            format!("[expanding refs...]\n\n{}", body)
        } else {
            body
        };

        let text = tui_markdown::from_str(&display_body);
        let content_width = layout[1].width.saturating_sub(2) as usize;
        let total_lines: usize = text.lines.iter().map(|line| {
            let line_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
            if content_width == 0 {
                1
            } else {
                (line_width / content_width) + 1
            }
        }).sum();

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.scroll_offset, 0));
        f.render_widget(paragraph, layout[1]);

        if total_lines > app.fullscreen_height {
            render_scrollbar(f, layout[1], total_lines, app.fullscreen_height, app.scroll_offset as usize);
        }
    }
}

fn draw_help_overlay(f: &mut Frame) {
    let area = f.area();

    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 20.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(Span::styled("Keybindings", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  h/l       Switch type"),
        Line::from("  Space     Expand/collapse tree node"),
        Line::from("  j/k       Navigate up/down"),
        Line::from("  Enter     Open document fullscreen"),
        Line::from("  Esc       Back / close"),
        Line::from("  /         Search"),
        Line::from("  n         Create new document"),
        Line::from("  d         Delete document"),
        Line::from("  Tab       Switch preview tab"),
        Line::from("  g         Jump to top"),
        Line::from("  G         Jump to bottom"),
        Line::from("  q         Quit"),
        Line::from("  ?         Toggle this help"),
        Line::from(""),
        Line::from(Span::styled("Fullscreen", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  j/k       Scroll"),
        Line::from("  Esc/q     Back to dashboard"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Help "));
    f.render_widget(paragraph, popup_area);
}

fn draw_create_form(f: &mut Frame, app: &App) {
    let area = f.area();

    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 14.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let form = &app.create_form;
    let title = format!(" Create {} ", form.doc_type);

    let fields = [
        ("Title", &form.title, FormField::Title),
        ("Author", &form.author, FormField::Author),
        ("Tags", &form.tags, FormField::Tags),
        ("Related", &form.related, FormField::Related),
    ];

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    for (label, value, field) in &fields {
        let is_focused = form.focused_field == *field;
        let label_style = if is_focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let cursor = if is_focused { "_" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<10}", format!("{}:", label)), label_style),
            Span::raw(format!("{}{}", value, cursor)),
        ]));
        lines.push(Line::from(""));
    }

    if let Some(ref err) = form.error {
        lines.push(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Tab", Style::default().fg(Color::DarkGray)),
        Span::raw(" next  "),
        Span::styled("Enter", Style::default().fg(Color::DarkGray)),
        Span::raw(" create  "),
        Span::styled("Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(" cancel"),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title),
    );
    f.render_widget(paragraph, popup_area);
}

fn draw_delete_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let dc = &app.delete_confirm;

    let ref_count = dc.references.len();
    let content_height = if ref_count > 0 {
        6 + ref_count as u16
    } else {
        4
    };
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = content_height.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let mut lines = vec![
        Line::from(""),
        Line::from(format!("  Delete \"{}\"?", dc.doc_title)),
    ];

    if !dc.references.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Referenced by:",
            Style::default().fg(Color::DarkGray),
        )));
        for (rel_type, path) in &dc.references {
            let name = display_name(path);
            lines.push(Line::from(format!("    \u{2022} {} ({})", name, rel_type)));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "         [Enter: delete]  [Esc: cancel]",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Red))
            .title(" Delete? "),
    );
    f.render_widget(paragraph, popup_area);
}

fn draw_status_picker(f: &mut Frame, app: &App) {
    let area = f.area();

    let popup_width = 25u16.min(area.width.saturating_sub(4));
    let popup_height = 9u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let statuses = [
        Status::Draft,
        Status::Review,
        Status::Accepted,
        Status::Rejected,
        Status::Superseded,
    ];

    let mut lines: Vec<Line> = statuses
        .iter()
        .enumerate()
        .map(|(i, status)| {
            let prefix = if i == app.status_picker.selected { "> " } else { "  " };
            let mut style = Style::default().fg(status_color(status));
            if i == app.status_picker.selected {
                style = style.add_modifier(Modifier::BOLD);
            }
            Line::from(Span::styled(format!("{}{}", prefix, status), style))
        })
        .collect();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[j/k] [Enter] [Esc]",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Status "),
    );
    f.render_widget(paragraph, popup_area);
}

#[cfg(feature = "agent")]
fn draw_agent_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let dialog = &app.agent_dialog;

    if let Some(ref buffer) = dialog.text_input {
        let popup_width = (area.width * 50 / 100).max(30).min(area.width.saturating_sub(4));
        let popup_height = 6u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(popup_width)) / 2;
        let y = (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        f.render_widget(Clear, popup_area);

        let title = format!(" Custom Prompt — {} ", dialog.doc_title);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}_", buffer)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Enter", Style::default().fg(Color::DarkGray)),
                Span::raw(" submit  "),
                Span::styled("Esc", Style::default().fg(Color::DarkGray)),
                Span::raw(" back"),
            ]),
        ];

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        );
        f.render_widget(paragraph, popup_area);
        return;
    }

    let action_count = dialog.actions.len() as u16;
    let content_height = action_count + 2; // border chrome
    let popup_width = (area.width * 40 / 100).max(20).min(area.width.saturating_sub(4));
    let popup_height = content_height.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = dialog
        .actions
        .iter()
        .map(|action| ListItem::new(format!("  {}", action)))
        .collect();

    let title = format!(" Agent Actions \u{2014} {} ", dialog.doc_title);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default().with_selected(Some(dialog.selected_index));
    f.render_stateful_widget(list, popup_area, &mut state);
}

fn draw_warnings_panel(f: &mut Frame, app: &App) {
    let area = f.area();
    let parse_errors = app.store.parse_errors();

    let total_count = app.total_warnings_count();

    let popup_width = 70.min(area.width.saturating_sub(4));
    let content_height = if total_count == 0 {
        match &app.fix_result {
            Some(output) => (output.lines().count() as u16).max(1) + 2,
            None => 3,
        }
    } else {
        (total_count as u16) * 2 + 2
    };
    let popup_height = (content_height + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Warnings (f: fix, q/w/Esc: close) ");

    if total_count == 0 {
        let message = match &app.fix_result {
            Some(output) => output.clone(),
            None => "  No warnings".to_string(),
        };
        let lines: Vec<Line> = message.lines()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::DarkGray))))
            .collect();
        let msg = Paragraph::new(lines).block(block);
        f.render_widget(msg, popup_area);
        return;
    }

    let mut items: Vec<ListItem> = parse_errors
        .iter()
        .map(|err| {
            let lines = vec![
                Line::from(Span::styled(
                    format!("  {}", err.path.display()),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    format!("    {}", err.error),
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            ListItem::new(lines)
        })
        .collect();

    for msg in &app.validation_errors {
        let lines = vec![
            Line::from(Span::styled(
                format!("  error: {}", msg),
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                "    validation error".to_string(),
                Style::default().fg(Color::DarkGray),
            )),
        ];
        items.push(ListItem::new(lines));
    }

    for msg in &app.validation_warnings {
        let lines = vec![
            Line::from(Span::styled(
                format!("  warn: {}", msg),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::styled(
                "    validation warning".to_string(),
                Style::default().fg(Color::DarkGray),
            )),
        ];
        items.push(ListItem::new(lines));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default().with_selected(Some(app.warnings_selected));
    f.render_stateful_widget(list, popup_area, &mut state);
}

fn draw_search_overlay(f: &mut Frame, app: &App) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Cyan)),
        Span::raw(&app.search_query),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Search ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(input, layout[0]);

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|path| {
            let doc = app.store.get(path);
            let (title, status_str, status_clr) = match doc {
                Some(d) => (
                    d.title.as_str(),
                    format!("{}", d.status),
                    status_color(&d.status),
                ),
                None => ("?", "?".to_string(), Color::White),
            };
            let line = Line::from(vec![
                Span::raw(format!("  {:<40} ", title)),
                Span::styled(status_str, Style::default().fg(status_clr)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Results ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default().with_selected(Some(app.search_selected));
    f.render_stateful_widget(list, layout[1], &mut state);
}

fn draw_filters_mode(f: &mut Frame, app: &mut App, area: Rect) {
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main[1]);

    // Left panel: filter controls
    let status_value = match &app.filter_status {
        None => "all".to_string(),
        Some(s) => format!("{}", s),
    };
    let tag_value = match &app.filter_tag {
        None => "all".to_string(),
        Some(t) => t.clone(),
    };

    let status_style = if app.filter_focused == FilterField::Status {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else if app.filter_status.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let tag_style = if app.filter_focused == FilterField::Tag {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else if app.filter_tag.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let clear_style = if app.filter_focused == FilterField::ClearAction {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let filter_lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  Status: [{}]", status_value), status_style)),
        Line::from(""),
        Line::from(Span::styled(format!("  Tag:    [{}]", tag_value), tag_style)),
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

    // Right panel: doc list
    app.doc_list_height = right[0].height.saturating_sub(2) as usize;
    let filtered = app.filtered_docs();
    let filtered_count = filtered.len();
    let total_count = app.store.all_docs().len();

    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let dim = relations_focused;

    let rows: Vec<Row> = filtered
        .iter()
        .map(|doc| {
            let tree_cell = Cell::new("");
            let mut cells = vec![tree_cell];
            cells.extend(doc_row_cells(&doc.id, &doc.title, &doc.status, &doc.tags, doc.virtual_doc, dim));
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
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(format!(" Documents ({} of {}) ", filtered_count, total_count)),
        )
        .row_highlight_style(highlight_style);

    let mut state = TableState::default()
        .with_selected(Some(app.selected_doc))
        .with_offset(app.doc_list_offset);
    f.render_stateful_widget(table, right[0], &mut state);

    // Right panel: preview
    let doc = app.selected_filtered_doc();
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
        PreviewTab::Preview => draw_preview_content(f, app, right[1], block, doc),
        PreviewTab::Relations => draw_relations_content(f, app, right[1], block, doc),
    }
}

#[cfg(feature = "agent")]
fn draw_agents_screen(f: &mut Frame, app: &App, area: Rect) {
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
        let paragraph = Paragraph::new("No agents have been invoked yet. Press `a` on a document to start one.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
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
                    Cell::from(Span::styled(format!("  {}", icon), Style::default().fg(color))),
                    Cell::from(Span::raw(format!("{:<14}", record.session_id.split('-').next().unwrap_or(&record.session_id)))),
                    Cell::from(Span::raw(&*record.doc_title)),
                    Cell::from(Span::raw(&*record.action)),
                    Cell::from(Span::styled(&*record.started_at, Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(4),  // status icon
            Constraint::Length(14), // session ID (short)
            Constraint::Fill(1),   // document title
            Constraint::Length(18), // action
            Constraint::Min(20),   // started at
        ];

        let table = Table::new(rows, widths)
            .block(block)
            .header(
                Row::new(vec!["  ", "Session", "Document", "Action", "Started"])
                    .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
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

fn draw_metrics_skeleton(f: &mut Frame, area: Rect) {
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

fn draw_graph(f: &mut Frame, app: &App, area: Rect) {
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
                let connector = if is_last { " └─▶ " } else { " ├─▶ " };
                spans.push(Span::styled(
                    format!("{}{}", leading, connector),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let type_icon = app.type_icons
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
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn display_name_flat_file() {
        assert_eq!(display_name(Path::new("docs/rfcs/RFC-001-foo.md")), "RFC-001-foo");
    }

    #[test]
    fn display_name_subfolder_index() {
        assert_eq!(display_name(Path::new("docs/rfcs/RFC-002-bar/index.md")), "RFC-002-bar");
    }

    fn cell_debug(cell: &Cell) -> String {
        format!("{:?}", cell)
    }

    #[test]
    fn doc_row_cells_standard_document() {
        let tags = vec!["cli".to_string(), "tui".to_string()];
        let cells = doc_row_cells("RFC-001", "Test Title", &Status::Draft, &tags, false, false);

        assert_eq!(cells.len(), 4);

        let id_dbg = cell_debug(&cells[0]);
        assert!(id_dbg.contains("RFC-001"), "ID cell should contain RFC-001, got: {}", id_dbg);
        assert!(!id_dbg.contains("(virtual)"), "Non-virtual doc should not contain (virtual)");

        let title_dbg = cell_debug(&cells[1]);
        assert!(title_dbg.contains("Test Title"), "Title cell should contain Test Title, got: {}", title_dbg);
        assert!(!title_dbg.contains("(virtual)"), "Non-virtual doc should not contain (virtual)");

        let status_dbg = cell_debug(&cells[2]);
        assert!(status_dbg.contains("draft"), "Status cell should contain draft, got: {}", status_dbg);

        let tags_dbg = cell_debug(&cells[3]);
        assert!(tags_dbg.contains("[cli]"), "Tags cell should contain [cli], got: {}", tags_dbg);
        assert!(tags_dbg.contains("[tui]"), "Tags cell should contain [tui], got: {}", tags_dbg);
    }

    #[test]
    fn doc_row_cells_virtual_document() {
        let cells = doc_row_cells("RFC-002", "Virtual Doc", &Status::Draft, &[], true, false);

        assert_eq!(cells.len(), 4);

        let title_dbg = cell_debug(&cells[1]);
        assert!(title_dbg.contains("(virtual)"), "Virtual doc title should contain (virtual), got: {}", title_dbg);
    }

    #[test]
    fn doc_row_cells_tag_overflow() {
        let tags = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let cells = doc_row_cells("RFC-003", "Tags", &Status::Draft, &tags, false, false);

        let tags_dbg = cell_debug(&cells[3]);
        assert!(tags_dbg.contains("[a]"), "Tags cell should contain [a], got: {}", tags_dbg);
        assert!(tags_dbg.contains("[b]"), "Tags cell should contain [b], got: {}", tags_dbg);
        assert!(tags_dbg.contains("[c]"), "Tags cell should contain [c], got: {}", tags_dbg);
        assert!(tags_dbg.contains("+2"), "Tags cell should contain +2 overflow, got: {}", tags_dbg);
        assert!(!tags_dbg.contains("[d]"), "Tags cell should not contain [d], got: {}", tags_dbg);
        assert!(!tags_dbg.contains("[e]"), "Tags cell should not contain [e], got: {}", tags_dbg);
    }

    #[test]
    fn doc_row_cells_dim_when_relations_focused() {
        let tags = vec!["x".to_string()];
        let cells = doc_row_cells("RFC-004", "Dim", &Status::Accepted, &tags, false, true);

        for (i, cell) in cells.iter().enumerate() {
            let dbg = cell_debug(cell);
            let has_dark_gray = dbg.contains("DarkGray") || dbg.contains("dark_gray");
            assert!(
                has_dark_gray,
                "Cell {} should have DarkGray style when dim=true, got: {}",
                i,
                dbg,
            );
        }
    }
}
