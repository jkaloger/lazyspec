use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::engine::document::{RelationType, Status};
use crate::tui::app::{App, FormField, PreviewTab};

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

pub fn draw(f: &mut Frame, app: &App) {
    if app.fullscreen_doc {
        draw_fullscreen(f, app);
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }
    if app.create_form.active {
        draw_create_form(f, app);
        if app.show_help {
            draw_help_overlay(f);
        }
        return;
    }
    if app.search_mode {
        draw_search_overlay(f, app);
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

    if app.delete_confirm.active {
        draw_delete_confirm(f, app);
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
        .map(|(i, dt)| {
            let count = app.doc_count(dt);
            let content = format!("  {}s  ({})", dt, count);
            let style = if i == app.selected_type {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Types "),
    );
    f.render_widget(list, area);
}

fn draw_doc_list(f: &mut Frame, app: &App, area: Rect) {
    let relations_focused = app.preview_tab == PreviewTab::Relations;
    let docs = app.docs_for_current_type();
    let items: Vec<ListItem> = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| {
            let filename = doc
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?");
            let dim = relations_focused;
            let status_style = if dim {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(status_color(&doc.status))
            };
            let mut spans = vec![
                Span::styled(
                    format!("  {:<30} ", filename),
                    if dim { Style::default().fg(Color::DarkGray) } else { Style::default() },
                ),
                Span::styled(format!("{:<12}", doc.status), status_style),
            ];
            for (idx, tag) in doc.tags.iter().take(3).enumerate() {
                if idx > 0 {
                    spans.push(Span::raw(" "));
                }
                let tc = if dim { Color::DarkGray } else { tag_color(tag) };
                spans.push(Span::styled(format!("[{}]", tag), Style::default().fg(tc)));
            }
            if doc.tags.len() > 3 {
                spans.push(Span::styled(
                    format!(" +{}", doc.tags.len() - 3),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            let line = Line::from(spans);
            let style = if dim {
                if i == app.selected_doc {
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                }
            } else if i == app.selected_doc {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let (border_style, border_type) = if relations_focused {
        (Style::default().fg(Color::DarkGray), BorderType::Plain)
    } else {
        (Style::default().fg(Color::Cyan), BorderType::Double)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_type(border_type)
            .title(" Documents "),
    );
    f.render_widget(list, area);
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
        .border_style(border_style)
        .title(preview_title);

    match app.preview_tab {
        PreviewTab::Preview => draw_preview_content(f, app, area, block),
        PreviewTab::Relations => draw_relations_content(f, app, area, block),
    }
}

fn draw_preview_content(f: &mut Frame, app: &App, area: Rect, block: Block) {
    if let Some(doc) = app.selected_doc_meta() {
        let body = app.store.get_body(&doc.path).unwrap_or_default();

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

fn draw_relations_content(f: &mut Frame, app: &App, area: Rect, block: Block) {
    let Some(doc) = app.selected_doc_meta() else {
        let paragraph = Paragraph::new(" No document selected.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    };

    let relations = app.store.related_to(&doc.path);

    if relations.is_empty() {
        let paragraph = Paragraph::new(" No relations.")
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();
    let mut flat_index = 0usize;

    let type_order = [
        RelationType::Implements,
        RelationType::Supersedes,
        RelationType::Blocks,
        RelationType::RelatedTo,
    ];

    for rel_type in &type_order {
        let matching: Vec<_> = relations
            .iter()
            .filter(|(rt, _)| *rt == rel_type)
            .collect();

        if matching.is_empty() {
            continue;
        }

        items.push(ListItem::new(Line::from(Span::styled(
            format!("  {}", rel_type),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))));

        for (_, target_path) in &matching {
            let selected = flat_index == app.selected_relation;

            let (title, status_str, status_clr) =
                if let Some(target_doc) = app.store.get(target_path) {
                    (
                        target_doc.title.as_str(),
                        format!("{}", target_doc.status),
                        status_color(&target_doc.status),
                    )
                } else {
                    let name = target_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?");
                    (name, "missing".to_string(), Color::Red)
                };

            let (indicator_span, title_style) = if selected {
                (
                    Span::styled("  > ", Style::default().fg(Color::Cyan)),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    Span::raw("    "),
                    Style::default(),
                )
            };

            items.push(ListItem::new(Line::from(vec![
                indicator_span,
                Span::styled(format!("{:<35} ", title), title_style),
                Span::styled(status_str, Style::default().fg(status_clr)),
            ])));

            flat_index += 1;
        }
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_fullscreen(f: &mut Frame, app: &App) {
    let area = f.area();

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

        let body = match app.store.get_body(&doc.path) {
            Ok(b) => b,
            Err(_) => "Error loading document.".to_string(),
        };

        let text = tui_markdown::from_str(&body);
        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.scroll_offset, 0));
        f.render_widget(paragraph, layout[1]);
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
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?");
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
            .border_style(Style::default().fg(Color::Red))
            .title(" Delete? "),
    );
    f.render_widget(paragraph, popup_area);
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
            .title(" Search ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(input, layout[0]);

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let doc = app.store.get(path);
            let (title, status_str, status_clr) = match doc {
                Some(d) => (
                    d.title.as_str(),
                    format!("{}", d.status),
                    status_color(&d.status),
                ),
                None => ("?", "?".to_string(), Color::White),
            };
            let style = if i == app.search_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::raw(format!("  {:<40} ", title)),
                Span::styled(status_str, Style::default().fg(status_clr)),
            ]);
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Results ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, layout[1]);
}
