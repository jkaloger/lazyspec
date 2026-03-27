use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::engine::document::Status;
use crate::engine::git_status::GitFileStatus;
use crate::tui::state::{App, FormField};

use super::colors::status_color;

pub fn draw_help_overlay(f: &mut Frame) {
    let area = f.area();

    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 24.min(area.height.saturating_sub(4));
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
        Line::from(Span::styled("Relations", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  r         Add relation"),
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

pub fn draw_create_form(f: &mut Frame, app: &App) {
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
        let is_focused = form.focused_field == *field && !form.loading;
        let label_style = if form.loading {
            Style::default().fg(Color::DarkGray)
        } else if is_focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let value_style = if form.loading {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let cursor = if is_focused { "_" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<10}", format!("{}:", label)), label_style),
            Span::styled(format!("{}{}", value, cursor), value_style),
        ]));
        lines.push(Line::from(""));
    }

    if let Some(ref msg) = form.status_message {
        lines.push(Line::from(Span::styled(
            format!("  {}", msg),
            Style::default().fg(Color::Yellow),
        )));
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

pub fn draw_delete_confirm(f: &mut Frame, app: &App) {
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

pub fn draw_status_picker(f: &mut Frame, app: &App) {
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
        Status::InProgress,
        Status::Complete,
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

pub fn draw_link_editor(f: &mut Frame, app: &App) {
    use crate::tui::state::forms::REL_TYPES;

    let area = f.area();
    let editor = &app.link_editor;

    let popup_width = 40u16.min(area.width.saturating_sub(4));
    let popup_height = 16u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let rel_label = REL_TYPES
        .get(editor.rel_type_index)
        .unwrap_or(&"implements");

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("  Type: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("< {} >", rel_label),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Find: ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{}_", editor.query)),
    ]));

    lines.push(Line::from(""));

    let max_results = (popup_height as usize).saturating_sub(6);
    for (i, path) in editor.results.iter().take(max_results).enumerate() {
        let label = app
            .store
            .get(path)
            .map(|d| format!("{}: {}", d.id.to_uppercase(), d.title))
            .unwrap_or_else(|| path.display().to_string());

        let prefix = if i == editor.selected { "> " } else { "  " };
        let style = if i == editor.selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(format!("{}{}", prefix, label), style)));
    }

    if editor.results.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no matches)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Tab", Style::default().fg(Color::DarkGray)),
        Span::raw(" type  "),
        Span::styled("Enter", Style::default().fg(Color::DarkGray)),
        Span::raw(" link  "),
        Span::styled("Esc", Style::default().fg(Color::DarkGray)),
        Span::raw(" cancel"),
    ]));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Add Relation "),
    );
    f.render_widget(paragraph, popup_area);
}

#[cfg(feature = "agent")]
pub fn draw_agent_dialog(f: &mut Frame, app: &App) {
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
    let content_height = action_count + 2;
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

pub fn draw_warnings_panel(f: &mut Frame, app: &App) {
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

pub fn draw_search_overlay(f: &mut Frame, app: &App) {
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
            let gutter_span = match app.git_status_cache.get(path) {
                Some(GitFileStatus::New) => {
                    Span::styled("┃", Style::default().fg(Color::Green))
                }
                Some(GitFileStatus::Modified) => {
                    Span::styled("┃", Style::default().fg(Color::Yellow))
                }
                None => Span::raw(" "),
            };
            let line = Line::from(vec![
                gutter_span,
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
